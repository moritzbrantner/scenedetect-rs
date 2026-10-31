use std::collections::BTreeMap;

use crate::{
    DetectionStatsSink, FrameIndex, FrameSource, Result, SceneBoundary, StatsRow,
    ThresholdDetectorConfig,
};

pub(super) fn detect_threshold_streaming<S, T>(
    source: &mut S,
    config: ThresholdDetectorConfig,
    min_scene_len: u64,
    stats_sink: &mut T,
) -> Result<(Vec<SceneBoundary>, u64)>
where
    S: FrameSource,
    T: DetectionStatsSink,
{
    let metric_names = vec!["average_rgb".to_owned()];
    stats_sink.start(&metric_names)?;

    let mut boundaries = Vec::new();
    let mut last_scene_cut = 0;
    let mut last_fade_frame = None;
    let mut last_fade_type = None;
    let mut total_frames = 0_u64;

    while let Some(frame) = source.next_frame()? {
        if total_frames == 0 {
            last_scene_cut = frame.index.0;
        }
        let luma = frame.mean_luma();

        let mut metrics = BTreeMap::new();
        metrics.insert("average_rgb".to_owned(), luma);
        stats_sink.row(StatsRow {
            frame: frame.index,
            metrics,
        })?;

        let frame_number = frame.index.0;
        match last_fade_type {
            None => {
                last_fade_frame = Some(frame_number);
                last_fade_type = Some(if luma < config.threshold {
                    FadeType::Out
                } else {
                    FadeType::In
                });
            }
            Some(FadeType::In) if luma < config.threshold => {
                last_fade_frame = Some(frame_number);
                last_fade_type = Some(FadeType::Out);
            }
            Some(FadeType::Out) if luma >= config.threshold => {
                if frame_number.saturating_sub(last_scene_cut) >= min_scene_len {
                    let fade_out_frame = last_fade_frame.unwrap_or(frame_number);
                    let duration = frame_number.saturating_sub(fade_out_frame);
                    let split = fade_out_frame
                        + round_half_to_even(duration as f64 * (1.0 + config.fade_bias) / 2.0);
                    boundaries.push(SceneBoundary {
                        frame: FrameIndex(split),
                    });
                    last_scene_cut = frame_number;
                }
                last_fade_frame = Some(frame_number);
                last_fade_type = Some(FadeType::In);
            }
            _ => {}
        }

        total_frames += 1;
    }

    if matches!(last_fade_type, Some(FadeType::Out))
        && config.add_last_scene
        && total_frames.saturating_sub(last_scene_cut) >= min_scene_len
    {
        if let Some(frame) = last_fade_frame {
            boundaries.push(SceneBoundary {
                frame: FrameIndex(frame),
            });
        }
    }

    stats_sink.finish()?;
    Ok((boundaries, total_frames))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FadeType {
    In,
    Out,
}

pub(crate) fn round_half_to_even(value: f64) -> u64 {
    let floor = value.floor();
    let fraction = value - floor;
    if (fraction - 0.5).abs() < f64::EPSILON {
        let floor = floor as u64;
        return if floor.is_multiple_of(2) {
            floor
        } else {
            floor + 1
        };
    }
    value.round() as u64
}
