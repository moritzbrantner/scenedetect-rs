use std::collections::BTreeMap;

use super::pixel::rgb_luma;
use crate::{
    DetectionStatsSink, Frame, FrameSource, HistogramDetectorConfig, Result, SceneBoundary,
    StatsRow,
};

pub(super) fn detect_histogram_streaming<S, T>(
    source: &mut S,
    config: HistogramDetectorConfig,
    min_scene_len: u64,
    stats_sink: &mut T,
) -> Result<(Vec<SceneBoundary>, u64)>
where
    S: FrameSource,
    T: DetectionStatsSink,
{
    let bins = config.bins.max(1);
    let metric_name = format!("hist_diff [bins={bins}]");
    stats_sink.start(std::slice::from_ref(&metric_name))?;

    let mut boundaries = Vec::new();
    let mut last_cut = 0;
    let mut previous_histogram: Option<Vec<f64>> = None;
    let mut total_frames = 0_u64;
    let correlation_threshold = 1.0 - config.threshold;

    while let Some(frame) = source.next_frame()? {
        let histogram = luma_histogram(&frame, bins);
        let hist_diff = previous_histogram
            .as_ref()
            .map_or(0.0, |previous| histogram_correlation(previous, &histogram));

        let mut metrics = BTreeMap::new();
        metrics.insert(metric_name.clone(), hist_diff);
        stats_sink.row(StatsRow {
            frame: frame.index,
            metrics,
        })?;

        let frame_number = frame.index.0;
        if previous_histogram.is_some()
            && hist_diff <= correlation_threshold
            && frame_number.saturating_sub(last_cut) >= min_scene_len
        {
            boundaries.push(SceneBoundary { frame: frame.index });
            last_cut = frame_number;
        }

        previous_histogram = Some(histogram);
        total_frames += 1;
    }

    stats_sink.finish()?;
    Ok((boundaries, total_frames))
}

pub(crate) fn luma_histogram(frame: &Frame, bins: usize) -> Vec<f64> {
    let mut histogram = vec![0.0; bins];
    for px in frame.rgb.chunks_exact(3) {
        let luma = rgb_luma(px);
        let bin = ((luma * bins as f64) / 256.0)
            .floor()
            .clamp(0.0, (bins - 1) as f64) as usize;
        histogram[bin] += 1.0;
    }

    let norm = histogram
        .iter()
        .map(|value| value * value)
        .sum::<f64>()
        .sqrt();
    if norm > 0.0 {
        for value in &mut histogram {
            *value /= norm;
        }
    }
    histogram
}

pub(crate) fn histogram_correlation(previous: &[f64], current: &[f64]) -> f64 {
    let count = previous.len().min(current.len());
    if count == 0 {
        return 1.0;
    }

    let mean_previous = previous.iter().take(count).sum::<f64>() / count as f64;
    let mean_current = current.iter().take(count).sum::<f64>() / count as f64;
    let mut numerator = 0.0;
    let mut previous_sq = 0.0;
    let mut current_sq = 0.0;

    for (previous, current) in previous.iter().zip(current.iter()).take(count) {
        let previous_delta = previous - mean_previous;
        let current_delta = current - mean_current;
        numerator += previous_delta * current_delta;
        previous_sq += previous_delta * previous_delta;
        current_sq += current_delta * current_delta;
    }

    let denominator = (previous_sq * current_sq).sqrt();
    if denominator == 0.0 {
        1.0
    } else {
        numerator / denominator
    }
}
