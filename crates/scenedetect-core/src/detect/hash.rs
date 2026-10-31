use std::collections::BTreeMap;

use super::pixel::rgb_luma;
use crate::{
    DetectionStatsSink, Frame, FrameSource, HashDetectorConfig, Result, SceneBoundary, StatsRow,
};

pub(super) fn detect_hash_streaming<S, T>(
    source: &mut S,
    config: HashDetectorConfig,
    min_scene_len: u64,
    stats_sink: &mut T,
) -> Result<(Vec<SceneBoundary>, u64)>
where
    S: FrameSource,
    T: DetectionStatsSink,
{
    let size = config.size.max(1);
    let lowpass = config.lowpass.max(1);
    let metric_name = format!("hash_dist [size={size} lowpass={lowpass}]");
    stats_sink.start(std::slice::from_ref(&metric_name))?;

    let mut boundaries = Vec::new();
    let mut last_cut = 0;
    let mut previous_hash: Option<Vec<bool>> = None;
    let mut total_frames = 0_u64;

    while let Some(frame) = source.next_frame()? {
        let frame_hash = perceptual_hash(&frame, size, lowpass);
        let hash_dist = previous_hash
            .as_ref()
            .map_or(0.0, |previous| hash_distance(previous, &frame_hash));

        let mut metrics = BTreeMap::new();
        metrics.insert(metric_name.clone(), hash_dist);
        stats_sink.row(StatsRow {
            frame: frame.index,
            metrics,
        })?;

        let frame_number = frame.index.0;
        if previous_hash.is_some()
            && hash_dist >= config.threshold
            && frame_number.saturating_sub(last_cut) >= min_scene_len
        {
            boundaries.push(SceneBoundary { frame: frame.index });
            last_cut = frame_number;
        }

        previous_hash = Some(frame_hash);
        total_frames += 1;
    }

    stats_sink.finish()?;
    Ok((boundaries, total_frames))
}

pub(crate) fn perceptual_hash(frame: &Frame, size: usize, lowpass: usize) -> Vec<bool> {
    let imsize = size.saturating_mul(lowpass).max(1);
    let gray = grayscale(frame);
    let resized = resize_area(&gray, frame.width as usize, frame.height as usize, imsize);
    let max_value = resized
        .iter()
        .copied()
        .fold(0.0_f64, |max, value| max.max(value))
        .max(1.0);
    let normalized: Vec<_> = resized.into_iter().map(|value| value / max_value).collect();
    let dct = dct_2d(&normalized, imsize, size);

    let mut sorted = dct.clone();
    sorted.sort_by(|left, right| left.total_cmp(right));
    let median = if sorted.is_empty() {
        0.0
    } else if sorted.len().is_multiple_of(2) {
        let upper = sorted.len() / 2;
        (sorted[upper - 1] + sorted[upper]) / 2.0
    } else {
        sorted[sorted.len() / 2]
    };

    dct.into_iter().map(|value| value > median).collect()
}

pub(crate) fn hash_distance(previous: &[bool], current: &[bool]) -> f64 {
    let count = previous.len().min(current.len());
    if count == 0 {
        return 0.0;
    }
    let differing = previous
        .iter()
        .zip(current.iter())
        .take(count)
        .filter(|(previous, current)| previous != current)
        .count();
    differing as f64 / count as f64
}

fn grayscale(frame: &Frame) -> Vec<f64> {
    frame.rgb.chunks_exact(3).map(rgb_luma).collect()
}

fn resize_area(gray: &[f64], width: usize, height: usize, output_size: usize) -> Vec<f64> {
    if width == 0 || height == 0 {
        return vec![0.0; output_size * output_size];
    }

    let mut resized = vec![0.0; output_size * output_size];
    for out_y in 0..output_size {
        for out_x in 0..output_size {
            let start_x = out_x * width / output_size;
            let end_x = ((out_x + 1) * width).div_ceil(output_size).min(width);
            let start_y = out_y * height / output_size;
            let end_y = ((out_y + 1) * height).div_ceil(output_size).min(height);
            let mut sum = 0.0;
            let mut count = 0.0;
            for y in start_y..end_y {
                for x in start_x..end_x {
                    sum += gray[y * width + x];
                    count += 1.0;
                }
            }
            resized[out_y * output_size + out_x] = if count == 0.0 { 0.0 } else { sum / count };
        }
    }
    resized
}

fn dct_2d(input: &[f64], input_size: usize, output_size: usize) -> Vec<f64> {
    let mut output = Vec::with_capacity(output_size * output_size);
    let scale_0 = (1.0 / input_size as f64).sqrt();
    let scale_n = (2.0 / input_size as f64).sqrt();

    for v in 0..output_size {
        for u in 0..output_size {
            let mut sum = 0.0;
            for y in 0..input_size {
                let y_basis = ((std::f64::consts::PI * (2 * y + 1) as f64 * v as f64)
                    / (2.0 * input_size as f64))
                    .cos();
                for x in 0..input_size {
                    let x_basis = ((std::f64::consts::PI * (2 * x + 1) as f64 * u as f64)
                        / (2.0 * input_size as f64))
                        .cos();
                    sum += input[y * input_size + x] * x_basis * y_basis;
                }
            }
            let alpha_u = if u == 0 { scale_0 } else { scale_n };
            let alpha_v = if v == 0 { scale_0 } else { scale_n };
            let value = alpha_u * alpha_v * sum;
            output.push(if value.abs() < 1.0e-12 { 0.0 } else { value });
        }
    }

    output
}
