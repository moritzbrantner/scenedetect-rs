use std::collections::BTreeMap;

use super::edges::content_edge_distance;
use super::pixel::rgb_to_opencv_hsv;
use crate::review::{build_boundary_review, BoundaryCandidateSeed};
use crate::scene::build_scene_list;
use crate::{
    BoundaryCandidateStatus, BoundaryReview, BoundaryReviewOptions, ContentDetectionStats,
    ContentDetectorConfig, ContentWeights, DetectionOptions, DetectionStats,
    DetectionStatsDecision, DetectionStatsSink, Frame, FrameRate, FrameSource, Result,
    RichDetectionStatsRow, SceneBoundary, SceneList, StatsRow,
};

pub fn detect_content_stats<S>(
    mut source: S,
    config: ContentDetectorConfig,
    options: DetectionOptions,
) -> Result<ContentDetectionStats>
where
    S: FrameSource,
{
    let frame_rate = source.frame_rate();
    let metric_names = vec![
        "content_val".to_owned(),
        "delta_hue".to_owned(),
        "delta_saturation".to_owned(),
        "delta_luminance".to_owned(),
        "delta_edges".to_owned(),
    ];
    let mut rows = Vec::new();
    let mut last_candidate_boundary = 0;
    let mut previous = None;
    let mut total_frames = 0_u64;

    while let Some(frame) = source.next_frame()? {
        let metrics = match previous.as_ref() {
            Some(previous) => content_metrics(previous, &frame, &config.weights, config.luma_only),
            None => empty_content_metrics(),
        };
        let score = metrics.get("content_val").copied().unwrap_or(0.0);
        let frame_number = frame.index.0;
        let decision = if previous.is_none() {
            DetectionStatsDecision::NotEvaluated
        } else if score >= config.threshold {
            let decision =
                if frame_number.saturating_sub(last_candidate_boundary) >= options.min_scene_len {
                    DetectionStatsDecision::Accepted
                } else {
                    DetectionStatsDecision::SuppressedMinSceneLen
                };
            last_candidate_boundary = frame_number;
            decision
        } else {
            DetectionStatsDecision::BelowThreshold
        };

        rows.push(RichDetectionStatsRow {
            frame: frame.index,
            score,
            threshold: config.threshold,
            decision,
            metrics,
        });

        previous = Some(frame);
        total_frames += 1;
    }

    Ok(ContentDetectionStats {
        frame_rate,
        total_frames,
        detector_config: config,
        options,
        metric_names,
        rows,
    })
}

pub fn scene_list_from_content_detection_stats(stats: &ContentDetectionStats) -> SceneList {
    let boundaries = stats
        .rows
        .iter()
        .filter(|row| row.decision == DetectionStatsDecision::Accepted)
        .map(|row| SceneBoundary { frame: row.frame })
        .collect();
    build_scene_list(
        stats.frame_rate,
        stats.total_frames,
        boundaries,
        stats.options.clone(),
    )
}

pub fn detection_stats_from_content_detection_stats(
    stats: &ContentDetectionStats,
) -> DetectionStats {
    DetectionStats {
        metric_names: stats.metric_names.clone(),
        rows: stats
            .rows
            .iter()
            .map(|row| StatsRow {
                frame: row.frame,
                metrics: row.metrics.clone(),
            })
            .collect(),
    }
}

pub fn boundary_review_from_content_detection_stats(
    stats: &ContentDetectionStats,
    review_options: BoundaryReviewOptions,
) -> BoundaryReview {
    let review_threshold = review_options
        .review_threshold
        .unwrap_or(stats.detector_config.threshold * 0.8);
    let boundaries = stats
        .rows
        .iter()
        .filter(|row| row.decision == DetectionStatsDecision::Accepted)
        .map(|row| SceneBoundary { frame: row.frame })
        .collect();
    let candidate_seeds = stats
        .rows
        .iter()
        .filter(|row| row.score >= review_threshold)
        .map(|row| {
            let status = match row.decision {
                DetectionStatsDecision::Accepted => BoundaryCandidateStatus::Accepted,
                DetectionStatsDecision::SuppressedMinSceneLen => {
                    BoundaryCandidateStatus::SuppressedMinSceneLen
                }
                DetectionStatsDecision::BelowThreshold | DetectionStatsDecision::NotEvaluated => {
                    BoundaryCandidateStatus::NearMiss
                }
            };
            BoundaryCandidateSeed {
                status,
                frame: row.frame,
                score_metric: "content_val".to_owned(),
                score: row.score,
                detector_threshold: stats.detector_config.threshold,
                review_threshold,
                metrics: row.metrics.clone(),
            }
        })
        .collect();

    build_boundary_review(
        stats.frame_rate,
        "content",
        "content_val",
        stats.detector_config.threshold,
        review_threshold,
        stats.total_frames,
        boundaries,
        stats.options.clone(),
        candidate_seeds,
    )
}

pub(super) fn detect_content_boundary_review_streaming<S, T>(
    source: &mut S,
    frame_rate: FrameRate,
    config: ContentDetectorConfig,
    options: DetectionOptions,
    review_options: BoundaryReviewOptions,
    stats_sink: &mut T,
) -> Result<BoundaryReview>
where
    S: FrameSource,
    T: DetectionStatsSink,
{
    let metric_names = vec!["content_val".to_owned()];
    stats_sink.start(&metric_names)?;

    let review_threshold = review_options
        .review_threshold
        .unwrap_or(config.threshold * 0.8);
    let mut boundaries = Vec::new();
    let mut candidate_seeds = Vec::new();
    let mut last_candidate_boundary = 0;
    let mut previous = None;
    let mut total_frames = 0_u64;

    while let Some(frame) = source.next_frame()? {
        let content_val = match previous.as_ref() {
            Some(previous) => content_score(previous, &frame, &config.weights, config.luma_only),
            None => 0.0,
        };

        let mut metrics = BTreeMap::new();
        metrics.insert("content_val".to_owned(), content_val);
        stats_sink.row(StatsRow {
            frame: frame.index,
            metrics: metrics.clone(),
        })?;

        let frame_number = frame.index.0;
        if content_val >= review_threshold {
            let status = if content_val >= config.threshold {
                let status = if frame_number.saturating_sub(last_candidate_boundary)
                    >= options.min_scene_len
                {
                    boundaries.push(SceneBoundary { frame: frame.index });
                    BoundaryCandidateStatus::Accepted
                } else {
                    BoundaryCandidateStatus::SuppressedMinSceneLen
                };
                last_candidate_boundary = frame_number;
                status
            } else {
                BoundaryCandidateStatus::NearMiss
            };

            candidate_seeds.push(BoundaryCandidateSeed {
                status,
                frame: frame.index,
                score_metric: "content_val".to_owned(),
                score: content_val,
                detector_threshold: config.threshold,
                review_threshold,
                metrics,
            });
        }

        previous = Some(frame);
        total_frames += 1;
    }

    stats_sink.finish()?;
    Ok(build_boundary_review(
        frame_rate,
        "content",
        "content_val",
        config.threshold,
        review_threshold,
        total_frames,
        boundaries,
        options,
        candidate_seeds,
    ))
}

pub(super) fn detect_content_streaming<S, T>(
    source: &mut S,
    config: ContentDetectorConfig,
    min_scene_len: u64,
    stats_sink: &mut T,
) -> Result<(Vec<SceneBoundary>, u64)>
where
    S: FrameSource,
    T: DetectionStatsSink,
{
    let metric_names = vec!["content_val".to_owned()];
    stats_sink.start(&metric_names)?;

    let mut boundaries = Vec::new();
    let mut last_candidate_boundary = 0;
    let mut previous = None;
    let mut total_frames = 0_u64;

    while let Some(frame) = source.next_frame()? {
        let content_val = match previous.as_ref() {
            Some(previous) => content_score(previous, &frame, &config.weights, config.luma_only),
            None => 0.0,
        };

        let mut metrics = BTreeMap::new();
        metrics.insert("content_val".to_owned(), content_val);
        stats_sink.row(StatsRow {
            frame: frame.index,
            metrics,
        })?;

        let frame_number = frame.index.0;
        if content_val >= config.threshold {
            if frame_number.saturating_sub(last_candidate_boundary) >= min_scene_len {
                boundaries.push(SceneBoundary { frame: frame.index });
            }
            last_candidate_boundary = frame_number;
        }

        previous = Some(frame);
        total_frames += 1;
    }

    stats_sink.finish()?;
    Ok((boundaries, total_frames))
}

pub(crate) fn content_score(
    previous: &Frame,
    current: &Frame,
    weights: &ContentWeights,
    luma_only: bool,
) -> f64 {
    content_metrics(previous, current, weights, luma_only)
        .get("content_val")
        .copied()
        .unwrap_or(0.0)
}

fn empty_content_metrics() -> BTreeMap<String, f64> {
    BTreeMap::from([
        ("content_val".to_owned(), 0.0),
        ("delta_hue".to_owned(), 0.0),
        ("delta_saturation".to_owned(), 0.0),
        ("delta_luminance".to_owned(), 0.0),
        ("delta_edges".to_owned(), 0.0),
    ])
}

fn content_metrics(
    previous: &Frame,
    current: &Frame,
    weights: &ContentWeights,
    luma_only: bool,
) -> BTreeMap<String, f64> {
    let (hue_weight, saturation_weight, luminance_weight, edge_weight) = if luma_only {
        (0.0, 0.0, 1.0, 0.0)
    } else {
        (
            weights.hue,
            weights.saturation,
            weights.luminance,
            weights.edges,
        )
    };
    let channel_weight_total =
        hue_weight.abs() + saturation_weight.abs() + luminance_weight.abs() + edge_weight.abs();
    let mut weighted_sum = 0.0;
    let mut hue_sum = 0.0;
    let mut saturation_sum = 0.0;
    let mut luminance_sum = 0.0;
    let mut pixel_count = 0.0;

    for (prev, curr) in previous
        .rgb
        .chunks_exact(3)
        .zip(current.rgb.chunks_exact(3))
    {
        pixel_count += 1.0;
        let (prev_hue, prev_saturation, prev_luminance) = rgb_to_opencv_hsv(prev);
        let (curr_hue, curr_saturation, curr_luminance) = rgb_to_opencv_hsv(curr);
        let hue = (prev_hue as f64 - curr_hue as f64).abs();
        let saturation = (prev_saturation as f64 - curr_saturation as f64).abs();
        let luminance = (prev_luminance as f64 - curr_luminance as f64).abs();
        hue_sum += hue;
        saturation_sum += saturation;
        luminance_sum += luminance;
        weighted_sum += hue * hue_weight;
        weighted_sum += saturation * saturation_weight;
        weighted_sum += luminance * luminance_weight;
    }

    // PySceneDetect only needs the comparatively expensive edge map when the
    // configured edge component participates in the detector score. Default
    // weights therefore retain the HSV-only fast path.
    let delta_edges = if edge_weight != 0.0 {
        content_edge_distance(previous, current)
    } else {
        0.0
    };
    weighted_sum += delta_edges * edge_weight * pixel_count;

    let denominator = pixel_count * channel_weight_total;
    let content_val = if denominator == 0.0 {
        0.0
    } else {
        weighted_sum / denominator
    };
    let component_denominator = if pixel_count == 0.0 { 1.0 } else { pixel_count };

    BTreeMap::from([
        ("content_val".to_owned(), content_val),
        ("delta_hue".to_owned(), hue_sum / component_denominator),
        (
            "delta_saturation".to_owned(),
            saturation_sum / component_denominator,
        ),
        (
            "delta_luminance".to_owned(),
            luminance_sum / component_denominator,
        ),
        ("delta_edges".to_owned(), delta_edges),
    ])
}
