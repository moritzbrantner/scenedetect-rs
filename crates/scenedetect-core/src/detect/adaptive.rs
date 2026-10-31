use std::collections::{BTreeMap, VecDeque};

use super::content::content_score;
use crate::review::{build_boundary_review, BoundaryCandidateSeed};
use crate::{
    AdaptiveDetectorConfig, BoundaryCandidateStatus, BoundaryReview, BoundaryReviewOptions,
    DetectionOptions, DetectionStatsSink, FrameIndex, FrameRate, FrameSource, Result,
    SceneBoundary, StatsRow,
};

pub(super) fn detect_adaptive_streaming<S, T>(
    source: &mut S,
    config: AdaptiveDetectorConfig,
    min_scene_len: u64,
    stats_sink: &mut T,
) -> Result<(Vec<SceneBoundary>, u64)>
where
    S: FrameSource,
    T: DetectionStatsSink,
{
    let metric_names = vec!["adaptive_ratio".to_owned(), "content_val".to_owned()];
    stats_sink.start(&metric_names)?;

    let mut boundaries = Vec::new();
    let mut last_boundary = 0;
    let mut previous = None;
    let mut samples = VecDeque::new();
    let mut total_frames = 0_usize;
    let mut next_emit = 0_usize;

    while let Some(frame) = source.next_frame()? {
        let content_val = match previous.as_ref() {
            Some(previous) => content_score(previous, &frame, &config.weights, config.luma_only),
            None => 0.0,
        };
        samples.push_back(AdaptiveSample {
            position: total_frames,
            frame: frame.index,
            content_val,
        });
        previous = Some(frame);
        total_frames += 1;

        emit_ready_adaptive_rows(
            &mut samples,
            &mut next_emit,
            total_frames,
            None,
            &config,
            min_scene_len,
            &mut last_boundary,
            &mut boundaries,
            stats_sink,
        )?;
    }

    emit_ready_adaptive_rows(
        &mut samples,
        &mut next_emit,
        total_frames,
        Some(total_frames),
        &config,
        min_scene_len,
        &mut last_boundary,
        &mut boundaries,
        stats_sink,
    )?;

    stats_sink.finish()?;
    Ok((boundaries, total_frames as u64))
}

pub(super) fn detect_adaptive_boundary_review_streaming<S, T>(
    source: &mut S,
    frame_rate: FrameRate,
    config: AdaptiveDetectorConfig,
    options: DetectionOptions,
    review_options: BoundaryReviewOptions,
    stats_sink: &mut T,
) -> Result<BoundaryReview>
where
    S: FrameSource,
    T: DetectionStatsSink,
{
    let metric_names = vec!["adaptive_ratio".to_owned(), "content_val".to_owned()];
    stats_sink.start(&metric_names)?;

    let review_threshold = review_options
        .review_threshold
        .unwrap_or(config.threshold * 0.8);
    let mut boundaries = Vec::new();
    let mut candidate_seeds = Vec::new();
    let mut last_boundary = 0;
    let mut previous = None;
    let mut samples = VecDeque::new();
    let mut total_frames = 0_usize;
    let mut next_emit = 0_usize;

    while let Some(frame) = source.next_frame()? {
        let content_val = match previous.as_ref() {
            Some(previous) => content_score(previous, &frame, &config.weights, config.luma_only),
            None => 0.0,
        };
        samples.push_back(AdaptiveSample {
            position: total_frames,
            frame: frame.index,
            content_val,
        });
        previous = Some(frame);
        total_frames += 1;

        emit_ready_adaptive_review_rows(
            &mut samples,
            &mut next_emit,
            total_frames,
            None,
            &config,
            options.min_scene_len,
            review_threshold,
            &mut last_boundary,
            &mut boundaries,
            &mut candidate_seeds,
            stats_sink,
        )?;
    }

    emit_ready_adaptive_review_rows(
        &mut samples,
        &mut next_emit,
        total_frames,
        Some(total_frames),
        &config,
        options.min_scene_len,
        review_threshold,
        &mut last_boundary,
        &mut boundaries,
        &mut candidate_seeds,
        stats_sink,
    )?;

    stats_sink.finish()?;
    Ok(build_boundary_review(
        frame_rate,
        "adaptive",
        "adaptive_ratio",
        config.threshold,
        review_threshold,
        total_frames as u64,
        boundaries,
        options,
        candidate_seeds,
    ))
}

#[derive(Debug, Clone)]
pub(crate) struct AdaptiveSample {
    pub(crate) position: usize,
    pub(crate) frame: FrameIndex,
    pub(crate) content_val: f64,
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_ready_adaptive_rows<T>(
    samples: &mut VecDeque<AdaptiveSample>,
    next_emit: &mut usize,
    total_seen: usize,
    total_frames: Option<usize>,
    config: &AdaptiveDetectorConfig,
    min_scene_len: u64,
    last_boundary: &mut u64,
    boundaries: &mut Vec<SceneBoundary>,
    stats_sink: &mut T,
) -> Result<()>
where
    T: DetectionStatsSink,
{
    loop {
        if *next_emit >= total_seen {
            break;
        }

        let window = config.frame_window;
        let ratio_is_ready = match total_frames {
            Some(total_frames) => *next_emit >= window && *next_emit + window < total_frames,
            None => *next_emit >= window && *next_emit + window < total_seen,
        };
        let edge_ratio_is_known = *next_emit < window
            || total_frames.is_some_and(|total_frames| *next_emit + window >= total_frames);

        if !ratio_is_ready && !edge_ratio_is_known {
            break;
        }

        let sample = sample_at(samples, *next_emit).clone();
        let adaptive_ratio = if ratio_is_ready {
            adaptive_ratio(samples, *next_emit, window)
        } else {
            0.0
        };

        let mut metrics = BTreeMap::new();
        metrics.insert("content_val".to_owned(), sample.content_val);
        metrics.insert("adaptive_ratio".to_owned(), adaptive_ratio);
        stats_sink.row(StatsRow {
            frame: sample.frame,
            metrics,
        })?;

        let frame_number = sample.frame.0;
        if sample.content_val >= config.min_content_val
            && adaptive_ratio >= config.threshold
            && frame_number.saturating_sub(*last_boundary) >= min_scene_len
        {
            boundaries.push(SceneBoundary {
                frame: sample.frame,
            });
            *last_boundary = frame_number;
        }

        *next_emit += 1;
        discard_unneeded_adaptive_samples(samples, *next_emit, window);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn emit_ready_adaptive_review_rows<T>(
    samples: &mut VecDeque<AdaptiveSample>,
    next_emit: &mut usize,
    total_seen: usize,
    total_frames: Option<usize>,
    config: &AdaptiveDetectorConfig,
    min_scene_len: u64,
    review_threshold: f64,
    last_boundary: &mut u64,
    boundaries: &mut Vec<SceneBoundary>,
    candidate_seeds: &mut Vec<BoundaryCandidateSeed>,
    stats_sink: &mut T,
) -> Result<()>
where
    T: DetectionStatsSink,
{
    loop {
        if *next_emit >= total_seen {
            break;
        }

        let window = config.frame_window;
        let ratio_is_ready = match total_frames {
            Some(total_frames) => *next_emit >= window && *next_emit + window < total_frames,
            None => *next_emit >= window && *next_emit + window < total_seen,
        };
        let edge_ratio_is_known = *next_emit < window
            || total_frames.is_some_and(|total_frames| *next_emit + window >= total_frames);

        if !ratio_is_ready && !edge_ratio_is_known {
            break;
        }

        let sample = sample_at(samples, *next_emit).clone();
        let adaptive_ratio = if ratio_is_ready {
            adaptive_ratio(samples, *next_emit, window)
        } else {
            0.0
        };

        let mut metrics = BTreeMap::new();
        metrics.insert("content_val".to_owned(), sample.content_val);
        metrics.insert("adaptive_ratio".to_owned(), adaptive_ratio);
        stats_sink.row(StatsRow {
            frame: sample.frame,
            metrics: metrics.clone(),
        })?;

        let frame_number = sample.frame.0;
        if sample.content_val >= config.min_content_val && adaptive_ratio >= review_threshold {
            let status = if adaptive_ratio >= config.threshold {
                if frame_number.saturating_sub(*last_boundary) >= min_scene_len {
                    boundaries.push(SceneBoundary {
                        frame: sample.frame,
                    });
                    *last_boundary = frame_number;
                    BoundaryCandidateStatus::Accepted
                } else {
                    BoundaryCandidateStatus::SuppressedMinSceneLen
                }
            } else {
                BoundaryCandidateStatus::NearMiss
            };

            candidate_seeds.push(BoundaryCandidateSeed {
                status,
                frame: sample.frame,
                score_metric: "adaptive_ratio".to_owned(),
                score: adaptive_ratio,
                detector_threshold: config.threshold,
                review_threshold,
                metrics,
            });
        }

        *next_emit += 1;
        discard_unneeded_adaptive_samples(samples, *next_emit, window);
    }

    Ok(())
}

fn sample_at(samples: &VecDeque<AdaptiveSample>, position: usize) -> &AdaptiveSample {
    samples
        .iter()
        .find(|sample| sample.position == position)
        .expect("adaptive sample window contains requested position")
}

fn adaptive_ratio(samples: &VecDeque<AdaptiveSample>, position: usize, window: usize) -> f64 {
    let center = sample_at(samples, position);
    let start = position - window;
    let end = position + window;
    let mut neighbour_sum = 0.0;
    let mut neighbour_count = 0.0;

    for neighbour_position in start..=end {
        if neighbour_position != position {
            neighbour_sum += sample_at(samples, neighbour_position).content_val;
            neighbour_count += 1.0;
        }
    }

    center.content_val / (neighbour_sum / neighbour_count + 0.000_001)
}

fn discard_unneeded_adaptive_samples(
    samples: &mut VecDeque<AdaptiveSample>,
    next_emit: usize,
    window: usize,
) {
    while samples
        .front()
        .is_some_and(|sample| sample.position < next_emit.saturating_sub(window))
    {
        samples.pop_front();
    }
}
