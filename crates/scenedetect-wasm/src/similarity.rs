use scenedetect_core::{Frame, FrameIndex, SceneList};
use serde::Serialize;

const GRID_SIZE: usize = 4;
const FEATURE_COUNT: usize = GRID_SIZE * GRID_SIZE * 3;
const MAX_SCENES_FOR_PAIRWISE_COMPARISON: usize = 1024;
const MAX_REPORTED_PAIRS: usize = 48;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrameVisualSignature {
    pub(crate) frame: FrameIndex,
    values: [u8; FEATURE_COUNT],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct SceneSimilarityReport {
    pub(crate) method: &'static str,
    pub(crate) total_scenes: usize,
    pub(crate) scenes_considered: usize,
    pub(crate) truncated: bool,
    pub(crate) pairs: Vec<SceneSimilarityPair>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct SceneSimilarityPair {
    pub(crate) first_scene: usize,
    pub(crate) second_scene: usize,
    pub(crate) first_start: FrameIndex,
    pub(crate) second_start: FrameIndex,
    pub(crate) similarity: f64,
}

#[derive(Debug, Clone)]
struct SceneAggregate {
    sums: [u64; FEATURE_COUNT],
    count: u64,
}

impl Default for SceneAggregate {
    fn default() -> Self {
        Self {
            sums: [0; FEATURE_COUNT],
            count: 0,
        }
    }
}

pub(crate) fn frame_visual_signature(frame: &Frame) -> FrameVisualSignature {
    let mut values = [0_u8; FEATURE_COUNT];
    let width = frame.width as usize;
    let height = frame.height as usize;

    if width == 0 || height == 0 {
        return FrameVisualSignature {
            frame: frame.index,
            values,
        };
    }

    let mut target = 0;
    for grid_y in 0..GRID_SIZE {
        let y = ((2 * grid_y + 1) * height / (2 * GRID_SIZE)).min(height - 1);
        for grid_x in 0..GRID_SIZE {
            let x = ((2 * grid_x + 1) * width / (2 * GRID_SIZE)).min(width - 1);
            let offset = (y * width + x) * 3;
            if let Some(pixel) = frame.rgb.get(offset..offset + 3) {
                values[target] = pixel[0];
                values[target + 1] = pixel[1];
                values[target + 2] = pixel[2];
            }
            target += 3;
        }
    }

    FrameVisualSignature {
        frame: frame.index,
        values,
    }
}

pub(crate) fn scene_similarity_report(
    scene_list: &SceneList,
    signatures: &[FrameVisualSignature],
) -> SceneSimilarityReport {
    let total_scenes = scene_list.scenes.len();
    if total_scenes < 2 {
        return SceneSimilarityReport {
            method: "4x4_rgb_scene_mean_mad_v1",
            total_scenes,
            scenes_considered: total_scenes,
            truncated: false,
            pairs: Vec::new(),
        };
    }

    let mut aggregates = vec![SceneAggregate::default(); total_scenes];
    let mut scene_index = 0_usize;
    for signature in signatures {
        while scene_index < total_scenes
            && signature.frame.0 >= scene_list.scenes[scene_index].end.0
        {
            scene_index += 1;
        }
        if scene_index >= total_scenes {
            break;
        }
        let scene = &scene_list.scenes[scene_index];
        if signature.frame.0 < scene.start.0 {
            continue;
        }
        let aggregate = &mut aggregates[scene_index];
        for (sum, value) in aggregate.sums.iter_mut().zip(signature.values) {
            *sum += value as u64;
        }
        aggregate.count += 1;
    }

    let selected = selected_scene_indices(total_scenes);
    let truncated = selected.len() < total_scenes;
    let fingerprints = selected
        .iter()
        .filter_map(|scene_index| {
            let aggregate = &aggregates[*scene_index];
            if aggregate.count == 0 {
                return None;
            }
            let values = aggregate
                .sums
                .iter()
                .map(|sum| *sum as f64 / aggregate.count as f64)
                .collect::<Vec<_>>();
            Some((*scene_index, values))
        })
        .collect::<Vec<_>>();

    let mut pairs = Vec::with_capacity(MAX_REPORTED_PAIRS);
    for first in 0..fingerprints.len() {
        for second in (first + 1)..fingerprints.len() {
            let (first_scene_index, first_values) = &fingerprints[first];
            let (second_scene_index, second_values) = &fingerprints[second];
            let similarity = fingerprint_similarity(first_values, second_values);
            let candidate = SceneSimilarityPair {
                first_scene: first_scene_index + 1,
                second_scene: second_scene_index + 1,
                first_start: scene_list.scenes[*first_scene_index].start,
                second_start: scene_list.scenes[*second_scene_index].start,
                similarity,
            };
            keep_best_pair(&mut pairs, candidate);
        }
    }

    pairs.sort_by(|left, right| {
        right
            .similarity
            .total_cmp(&left.similarity)
            .then_with(|| left.first_scene.cmp(&right.first_scene))
            .then_with(|| left.second_scene.cmp(&right.second_scene))
    });

    SceneSimilarityReport {
        method: "4x4_rgb_scene_mean_mad_v1",
        total_scenes,
        scenes_considered: selected.len(),
        truncated,
        pairs,
    }
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

fn fingerprint_similarity(left: &[f64], right: &[f64]) -> f64 {
    let count = left.len().min(right.len());
    if count == 0 {
        return 0.0;
    }
    let mean_absolute_difference = left
        .iter()
        .zip(right.iter())
        .take(count)
        .map(|(left, right)| (left - right).abs())
        .sum::<f64>()
        / count as f64;
    (1.0 - mean_absolute_difference / 255.0).clamp(0.0, 1.0)
}

fn keep_best_pair(pairs: &mut Vec<SceneSimilarityPair>, candidate: SceneSimilarityPair) {
    if pairs.len() < MAX_REPORTED_PAIRS {
        pairs.push(candidate);
        return;
    }

    let (worst_index, worst) = pairs
        .iter()
        .enumerate()
        .min_by(|(_, left), (_, right)| {
            left.similarity
                .total_cmp(&right.similarity)
                .then_with(|| right.first_scene.cmp(&left.first_scene))
                .then_with(|| right.second_scene.cmp(&left.second_scene))
        })
        .expect("bounded similarity pair list is non-empty");

    let candidate_is_better = candidate.similarity > worst.similarity
        || (candidate.similarity == worst.similarity
            && (candidate.first_scene, candidate.second_scene)
                < (worst.first_scene, worst.second_scene));
    if candidate_is_better {
        pairs[worst_index] = candidate;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use scenedetect_core::{FrameRate, SceneSpan};

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

    #[test]
    fn repeated_visual_scenes_rank_as_most_similar() {
        let frames = [
            Frame::solid(0, 8, 8, [220, 30, 30]),
            Frame::solid(1, 8, 8, [220, 30, 30]),
            Frame::solid(2, 8, 8, [20, 40, 220]),
            Frame::solid(3, 8, 8, [20, 40, 220]),
            Frame::solid(4, 8, 8, [220, 30, 30]),
            Frame::solid(5, 8, 8, [220, 30, 30]),
        ];
        let signatures = frames.iter().map(frame_visual_signature).collect::<Vec<_>>();

        let report = scene_similarity_report(&scene_list(), &signatures);

        assert_eq!(report.total_scenes, 3);
        assert_eq!(report.scenes_considered, 3);
        assert_eq!(report.pairs[0].first_scene, 1);
        assert_eq!(report.pairs[0].second_scene, 3);
        assert_eq!(report.pairs[0].similarity, 1.0);
        assert!(report.pairs[1].similarity < 0.8);
    }

    #[test]
    fn signature_preserves_spatial_color_layout() {
        let mut rgb = vec![0_u8; 8 * 8 * 3];
        for y in 0..8 {
            for x in 0..8 {
                let offset = (y * 8 + x) * 3;
                rgb[offset..offset + 3].copy_from_slice(if x < 4 {
                    &[255, 0, 0]
                } else {
                    &[0, 0, 255]
                });
            }
        }
        let frame = Frame {
            index: FrameIndex(7),
            width: 8,
            height: 8,
            rgb,
        };

        let signature = frame_visual_signature(&frame);

        assert_eq!(signature.frame, FrameIndex(7));
        assert_ne!(&signature.values[0..3], &signature.values[9..12]);
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

        let report = scene_similarity_report(&scene_list, &[]);

        assert!(report.truncated);
        assert_eq!(report.scenes_considered, MAX_SCENES_FOR_PAIRWISE_COMPARISON);
        assert!(report.pairs.is_empty());
    }
}
