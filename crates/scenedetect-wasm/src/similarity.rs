use std::cell::RefCell;

use image::GrayImage;
use image_analysis_processing::{
    hash_distance, perceptual_hash_luma, PERCEPTUAL_HASH_HIGHFREQ_FACTOR,
    PERCEPTUAL_HASH_SIZE,
};
use scenedetect_core::{FrameIndex, SceneList};
use serde::Serialize;

const OK: i32 = 0;
const ERROR: i32 = -1;
const HASH_BITS: usize = (PERCEPTUAL_HASH_SIZE * PERCEPTUAL_HASH_SIZE) as usize;
const PHASH_SAMPLE_SIDE: usize =
    (PERCEPTUAL_HASH_SIZE * PERCEPTUAL_HASH_HIGHFREQ_FACTOR) as usize;
const MAX_BROWSER_SIGNATURES: usize = 200_000;
const MAX_SCENES_FOR_PAIRWISE_COMPARISON: usize = 1024;
const MAX_REPORTED_PAIRS: usize = 48;

thread_local! {
    static VISUAL_SIGNATURES: RefCell<Vec<FrameVisualSignature>> = const { RefCell::new(Vec::new()) };
    static LAST_RESULT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static LAST_ERROR: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FrameVisualSignature {
    frame: FrameIndex,
    hash: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct SceneSimilarityReport {
    method: &'static str,
    total_scenes: usize,
    scenes_considered: usize,
    truncated: bool,
    pairs: Vec<SceneSimilarityPair>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
struct SceneSimilarityPair {
    first_scene: usize,
    second_scene: usize,
    first_start: FrameIndex,
    second_start: FrameIndex,
    hash_distance: u32,
    similarity: f64,
}

#[derive(Debug, Clone)]
struct SceneHashAggregate {
    one_counts: [u32; HASH_BITS],
    count: u32,
    first_hash: Option<u64>,
}

impl Default for SceneHashAggregate {
    fn default() -> Self {
        Self {
            one_counts: [0; HASH_BITS],
            count: 0,
            first_hash: None,
        }
    }
}

impl SceneHashAggregate {
    fn add(&mut self, hash: u64) {
        if self.first_hash.is_none() {
            self.first_hash = Some(hash);
        }
        for bit in 0..HASH_BITS {
            if hash & (1_u64 << bit) != 0 {
                self.one_counts[bit] += 1;
            }
        }
        self.count += 1;
    }

    fn majority_hash(&self) -> Option<u64> {
        let first_hash = self.first_hash?;
        let mut hash = 0_u64;
        for bit in 0..HASH_BITS {
            let ones = u64::from(self.one_counts[bit]);
            let count = u64::from(self.count);
            let first_bit_is_set = first_hash & (1_u64 << bit) != 0;
            if ones * 2 > count || (ones * 2 == count && first_bit_is_set) {
                hash |= 1_u64 << bit;
            }
        }
        Some(hash)
    }
}

fn set_result(bytes: Vec<u8>) {
    LAST_RESULT.with(|result| *result.borrow_mut() = bytes);
    LAST_ERROR.with(|error| error.borrow_mut().clear());
}

fn set_error(message: impl Into<String>) {
    LAST_ERROR.with(|error| *error.borrow_mut() = message.into().into_bytes());
    LAST_RESULT.with(|result| result.borrow_mut().clear());
}

fn clear_error() {
    LAST_ERROR.with(|error| error.borrow_mut().clear());
}

#[no_mangle]
pub extern "C" fn scenedetect_similarity_result_ptr() -> *const u8 {
    LAST_RESULT.with(|result| result.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn scenedetect_similarity_result_len() -> usize {
    LAST_RESULT.with(|result| result.borrow().len())
}

#[no_mangle]
pub extern "C" fn scenedetect_similarity_error_ptr() -> *const u8 {
    LAST_ERROR.with(|error| error.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn scenedetect_similarity_error_len() -> usize {
    LAST_ERROR.with(|error| error.borrow().len())
}

#[no_mangle]
pub extern "C" fn scenedetect_similarity_reset() -> i32 {
    VISUAL_SIGNATURES.with(|signatures| signatures.borrow_mut().clear());
    set_result(Vec::new());
    OK
}

#[no_mangle]
pub extern "C" fn scenedetect_similarity_push(
    index: u32,
    width: u32,
    height: u32,
    rgb_ptr: *const u8,
    rgb_len: usize,
) -> i32 {
    let result = (|| -> Result<(), String> {
        if width == 0 || height == 0 {
            return Err("similarity frame dimensions must be non-zero".to_owned());
        }
        let expected_len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(3))
            .ok_or_else(|| "similarity frame dimensions overflow RGB buffer size".to_owned())?;
        if rgb_len != expected_len {
            return Err(format!(
                "similarity RGB buffer length mismatch: expected {expected_len}, received {rgb_len}"
            ));
        }
        if rgb_ptr.is_null() {
            return Err("similarity RGB pointer is null".to_owned());
        }
        let rgb = unsafe { std::slice::from_raw_parts(rgb_ptr, rgb_len) };
        let signature = frame_visual_signature(FrameIndex(index as u64), width, height, rgb)?;
        VISUAL_SIGNATURES.with(|signatures| {
            let mut signatures = signatures.borrow_mut();
            if signatures.len() >= MAX_BROWSER_SIGNATURES {
                return Err(format!(
                    "scene similarity accepts at most {MAX_BROWSER_SIGNATURES} analyzed samples"
                ));
            }
            if signatures
                .last()
                .is_some_and(|previous| signature.frame <= previous.frame)
            {
                return Err("scene similarity sample indexes must increase".to_owned());
            }
            signatures.push(signature);
            Ok(())
        })
    })();

    match result {
        Ok(()) => {
            clear_error();
            OK
        }
        Err(error) => {
            set_error(error);
            ERROR
        }
    }
}

#[no_mangle]
pub extern "C" fn scenedetect_similarity_finish(
    scene_list_ptr: *const u8,
    scene_list_len: usize,
) -> i32 {
    let result = (|| -> Result<Vec<u8>, String> {
        if scene_list_len == 0 || scene_list_ptr.is_null() {
            return Err("scene similarity requires a Scene List document".to_owned());
        }
        let bytes = unsafe { std::slice::from_raw_parts(scene_list_ptr, scene_list_len) };
        let scene_list: SceneList =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        let signatures =
            VISUAL_SIGNATURES.with(|signatures| std::mem::take(&mut *signatures.borrow_mut()));
        let report = scene_similarity_report(&scene_list, &signatures)?;
        serde_json::to_vec(&report).map_err(|error| error.to_string())
    })();

    match result {
        Ok(bytes) => {
            set_result(bytes);
            OK
        }
        Err(error) => {
            VISUAL_SIGNATURES.with(|signatures| signatures.borrow_mut().clear());
            set_error(error);
            ERROR
        }
    }
}

fn frame_visual_signature(
    frame: FrameIndex,
    width: u32,
    height: u32,
    rgb: &[u8],
) -> Result<FrameVisualSignature, String> {
    let luma = compact_luma_sample(width, height, rgb);
    let image = GrayImage::from_raw(PHASH_SAMPLE_SIDE as u32, PHASH_SAMPLE_SIDE as u32, luma)
        .ok_or_else(|| "unable to construct compact luma image for scene similarity".to_owned())?;
    let hash = perceptual_hash_luma(&image, PERCEPTUAL_HASH_SIZE);
    let hash = u64::from_str_radix(&hash, 16)
        .map_err(|error| format!("shared perceptual hash is not compact hexadecimal: {error}"))?;
    Ok(FrameVisualSignature { frame, hash })
}

fn compact_luma_sample(width: u32, height: u32, rgb: &[u8]) -> Vec<u8> {
    let width = width as usize;
    let height = height as usize;
    let mut luma = Vec::with_capacity(PHASH_SAMPLE_SIDE * PHASH_SAMPLE_SIDE);
    for sample_y in 0..PHASH_SAMPLE_SIDE {
        let y = sample_coordinate(sample_y, height);
        for sample_x in 0..PHASH_SAMPLE_SIDE {
            let x = sample_coordinate(sample_x, width);
            let offset = (y * width + x) * 3;
            let red = u32::from(rgb[offset]);
            let green = u32::from(rgb[offset + 1]);
            let blue = u32::from(rgb[offset + 2]);
            let value = (77 * red + 150 * green + 29 * blue + 128) >> 8;
            luma.push(value as u8);
        }
    }
    luma
}

fn sample_coordinate(sample: usize, extent: usize) -> usize {
    let numerator = (2 * sample + 1) as u64 * extent as u64;
    let denominator = (2 * PHASH_SAMPLE_SIDE) as u64;
    ((numerator / denominator) as usize).min(extent - 1)
}

fn scene_similarity_report(
    scene_list: &SceneList,
    signatures: &[FrameVisualSignature],
) -> Result<SceneSimilarityReport, String> {
    let total_scenes = scene_list.scenes.len();
    if total_scenes < 2 {
        return Ok(SceneSimilarityReport {
            method: "visual_analysis_phash_scene_majority_v1",
            total_scenes,
            scenes_considered: total_scenes,
            truncated: false,
            pairs: Vec::new(),
        });
    }

    let selected = selected_scene_indices(total_scenes);
    let truncated = selected.len() < total_scenes;
    let fingerprints = selected_scene_hashes(scene_list, signatures, &selected);

    let mut pairs = Vec::with_capacity(MAX_REPORTED_PAIRS);
    for first in 0..fingerprints.len() {
        for second in (first + 1)..fingerprints.len() {
            let (first_scene_index, first_hash) = &fingerprints[first];
            let (second_scene_index, second_hash) = &fingerprints[second];
            let distance = hash_distance(first_hash, second_hash)?;
            let similarity = 1.0 - f64::from(distance) / HASH_BITS as f64;
            let candidate = SceneSimilarityPair {
                first_scene: first_scene_index + 1,
                second_scene: second_scene_index + 1,
                first_start: scene_list.scenes[*first_scene_index].start,
                second_start: scene_list.scenes[*second_scene_index].start,
                hash_distance: distance,
                similarity,
            };
            keep_best_pair(&mut pairs, candidate);
        }
    }

    pairs.sort_by(|left, right| {
        left.hash_distance
            .cmp(&right.hash_distance)
            .then_with(|| left.first_scene.cmp(&right.first_scene))
            .then_with(|| left.second_scene.cmp(&right.second_scene))
    });

    Ok(SceneSimilarityReport {
        method: "visual_analysis_phash_scene_majority_v1",
        total_scenes,
        scenes_considered: selected.len(),
        truncated,
        pairs,
    })
}

fn selected_scene_hashes(
    scene_list: &SceneList,
    signatures: &[FrameVisualSignature],
    selected: &[usize],
) -> Vec<(usize, String)> {
    let mut aggregates = vec![SceneHashAggregate::default(); selected.len()];
    let mut selected_position = 0_usize;

    for signature in signatures {
        while selected_position < selected.len()
            && signature.frame.0 >= scene_list.scenes[selected[selected_position]].end.0
        {
            selected_position += 1;
        }
        if selected_position >= selected.len() {
            break;
        }

        let scene_index = selected[selected_position];
        let scene = &scene_list.scenes[scene_index];
        if signature.frame.0 < scene.start.0 {
            continue;
        }
        aggregates[selected_position].add(signature.hash);
    }

    selected
        .iter()
        .copied()
        .zip(aggregates.iter())
        .filter_map(|(scene_index, aggregate)| {
            aggregate
                .majority_hash()
                .map(|hash| (scene_index, format!("{hash:016x}")))
        })
        .collect()
}

fn selected_scene_indices(total_scenes: usize) -> Vec<usize> {
    if total_scenes <= MAX_SCENES_FOR_PAIRWISE_COMPARISON {
        return (0..total_scenes).collect();
    }

    let mut selected = Vec::with_capacity(MAX_SCENES_FOR_PAIRWISE_COMPARISON);
    for slot in 0..MAX_SCENES_FOR_PAIRWISE_COMPARISON {
        let index = slot * (total_scenes - 1) / (MAX_SCENES_FOR_PAIRWISE_COMPARISON - 1);
        if selected.last().copied() != Some(index) {
            selected.push(index);
        }
    }
    selected
}

fn keep_best_pair(pairs: &mut Vec<SceneSimilarityPair>, candidate: SceneSimilarityPair) {
    if pairs.len() < MAX_REPORTED_PAIRS {
        pairs.push(candidate);
        return;
    }

    let (worst_index, worst) = pairs
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| {
            left.hash_distance
                .cmp(&right.hash_distance)
                .then_with(|| left.first_scene.cmp(&right.first_scene))
                .then_with(|| left.second_scene.cmp(&right.second_scene))
        })
        .expect("bounded similarity pair list is non-empty");

    let candidate_is_better = candidate.hash_distance < worst.hash_distance
        || (candidate.hash_distance == worst.hash_distance
            && (candidate.first_scene, candidate.second_scene)
                < (worst.first_scene, worst.second_scene));
    if candidate_is_better {
        pairs[worst_index] = candidate;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scenedetect_core::{Frame, FrameRate, SceneSpan};

    fn scene_list() -> SceneList {
        SceneList {
            frame_rate: FrameRate(10.0),
            scenes: vec![
                SceneSpan {
                    start: FrameIndex(0),
                    end: FrameIndex(2),
                },
                SceneSpan {
                    start: FrameIndex(2),
                    end: FrameIndex(4),
                },
                SceneSpan {
                    start: FrameIndex(4),
                    end: FrameIndex(6),
                },
            ],
        }
    }

    fn split_frame(index: u64, vertical: bool, brightness_offset: i16) -> Frame {
        let width = 64_u32;
        let height = 64_u32;
        let mut rgb = vec![0_u8; width as usize * height as usize * 3];
        for y in 0..height as usize {
            for x in 0..width as usize {
                let bright_side = if vertical { x < 32 } else { y < 32 };
                let base = if bright_side { 210_i16 } else { 35_i16 };
                let value = (base + brightness_offset).clamp(0, 255) as u8;
                let offset = (y * width as usize + x) * 3;
                rgb[offset..offset + 3].copy_from_slice(&[value, value, value]);
            }
        }
        Frame {
            index: FrameIndex(index),
            width,
            height,
            rgb,
        }
    }

    fn signature(frame: &Frame) -> FrameVisualSignature {
        frame_visual_signature(frame.index, frame.width, frame.height, &frame.rgb).unwrap()
    }

    #[test]
    fn repeated_visual_scenes_rank_as_most_similar() {
        let frames = [
            split_frame(0, true, 0),
            split_frame(1, true, 8),
            split_frame(2, false, 0),
            split_frame(3, false, -8),
            split_frame(4, true, -5),
            split_frame(5, true, 4),
        ];
        let signatures = frames.iter().map(signature).collect::<Vec<_>>();

        let report = scene_similarity_report(&scene_list(), &signatures).unwrap();

        assert_eq!(report.total_scenes, 3);
        assert_eq!(report.scenes_considered, 3);
        assert_eq!(report.pairs[0].first_scene, 1);
        assert_eq!(report.pairs[0].second_scene, 3);
        assert!(report.pairs[0].hash_distance <= 2);
        assert!(report.pairs[1].hash_distance > report.pairs[0].hash_distance);
    }

    #[test]
    fn shared_phash_is_robust_to_uniform_brightness_shift() {
        let original = signature(&split_frame(0, true, 0));
        let brighter = signature(&split_frame(1, true, 25));
        let distance = hash_distance(
            &format!("{:016x}", original.hash),
            &format!("{:016x}", brighter.hash),
        )
        .unwrap();

        assert!(distance <= 4, "unexpected pHash distance: {distance}");
    }

    #[test]
    fn retained_signature_is_compact() {
        assert!(std::mem::size_of::<FrameVisualSignature>() <= 16);
    }

    #[test]
    fn very_large_scene_lists_are_sampled_across_the_full_timeline() {
        let total = MAX_SCENES_FOR_PAIRWISE_COMPARISON + 17;
        let scene_list = SceneList {
            frame_rate: FrameRate(1.0),
            scenes: (0..total)
                .map(|index| SceneSpan {
                    start: FrameIndex(index as u64),
                    end: FrameIndex(index as u64 + 1),
                })
                .collect(),
        };

        let report = scene_similarity_report(&scene_list, &[]).unwrap();

        assert!(report.truncated);
        assert_eq!(report.scenes_considered, MAX_SCENES_FOR_PAIRWISE_COMPARISON);
        assert!(report.pairs.is_empty());
    }

    #[test]
    fn selected_scene_aggregation_is_bounded() {
        let total = MAX_SCENES_FOR_PAIRWISE_COMPARISON + 33;
        let scene_list = SceneList {
            frame_rate: FrameRate(1.0),
            scenes: (0..total)
                .map(|index| SceneSpan {
                    start: FrameIndex(index as u64),
                    end: FrameIndex(index as u64 + 1),
                })
                .collect(),
        };
        let selected = selected_scene_indices(total);
        let signatures = (0..total)
            .map(|index| FrameVisualSignature {
                frame: FrameIndex(index as u64),
                hash: index as u64,
            })
            .collect::<Vec<_>>();

        let fingerprints = selected_scene_hashes(&scene_list, &signatures, &selected);

        assert_eq!(selected.len(), MAX_SCENES_FOR_PAIRWISE_COMPARISON);
        assert_eq!(fingerprints.len(), MAX_SCENES_FOR_PAIRWISE_COMPARISON);
    }

    #[test]
    fn wasm_similarity_exports_round_trip_a_scene_list() {
        scenedetect_similarity_reset();
        let first = split_frame(0, true, 0);
        let second = split_frame(1, true, 10);
        for frame in [&first, &second] {
            assert_eq!(
                scenedetect_similarity_push(
                    frame.index.0 as u32,
                    frame.width,
                    frame.height,
                    frame.rgb.as_ptr(),
                    frame.rgb.len(),
                ),
                OK
            );
        }

        let scene_list = SceneList {
            frame_rate: FrameRate(6.0),
            scenes: vec![
                SceneSpan {
                    start: FrameIndex(0),
                    end: FrameIndex(1),
                },
                SceneSpan {
                    start: FrameIndex(1),
                    end: FrameIndex(2),
                },
            ],
        };
        let bytes = serde_json::to_vec(&scene_list).unwrap();
        assert_eq!(
            scenedetect_similarity_finish(bytes.as_ptr(), bytes.len()),
            OK
        );
        let result = LAST_RESULT.with(|result| result.borrow().clone());
        let value: serde_json::Value = serde_json::from_slice(&result).unwrap();
        assert_eq!(value["method"], "visual_analysis_phash_scene_majority_v1");
        assert_eq!(value["pairs"][0]["hash_distance"], 0);
        assert_eq!(value["pairs"][0]["similarity"], 1.0);
    }
}
