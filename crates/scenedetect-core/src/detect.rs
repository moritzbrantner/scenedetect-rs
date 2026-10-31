mod adaptive;
mod content;
mod edges;
mod hash;
mod histogram;
mod pixel;
mod threshold;

use crate::scene::build_scene_list;
use crate::stats::CollectingStatsSink;
use crate::{
    BoundaryReview, BoundaryReviewOptions, DetectionOptions, DetectionResult, DetectionStatsSink,
    Detector, DetectorConfig, Frame, FrameRate, FrameSource, Result, SceneDetectError, SceneList,
};

pub(crate) use adaptive::{emit_ready_adaptive_rows, AdaptiveSample};
pub(crate) use content::content_score;
pub use content::{
    boundary_review_from_content_detection_stats, detect_content_stats,
    detection_stats_from_content_detection_stats, scene_list_from_content_detection_stats,
};
pub(crate) use hash::{hash_distance, perceptual_hash};
pub(crate) use histogram::{histogram_correlation, luma_histogram};
pub(crate) use pixel::rgb_to_opencv_hsv;
pub(crate) use threshold::{round_half_to_even, FadeType};

use adaptive::{detect_adaptive_boundary_review_streaming, detect_adaptive_streaming};
use content::{detect_content_boundary_review_streaming, detect_content_streaming};
use hash::detect_hash_streaming;
use histogram::detect_histogram_streaming;
use threshold::detect_threshold_streaming;

pub fn detect_scenes<D, S>(
    detector: D,
    source: S,
    options: DetectionOptions,
) -> Result<DetectionResult>
where
    D: Detector,
    S: FrameSource,
{
    let mut stats_sink = CollectingStatsSink::default();
    let scene_list = detect_scenes_streaming(detector, source, options, &mut stats_sink)?;
    Ok(DetectionResult {
        scene_list,
        stats: stats_sink.into_stats(),
    })
}

pub fn detect_frames(
    detector: DetectorConfig,
    frame_rate: FrameRate,
    frames: &[Frame],
    options: DetectionOptions,
) -> Result<DetectionResult> {
    detect_scenes(
        detector,
        SliceFrameSource {
            frame_rate,
            frames: frames.iter(),
        },
        options,
    )
}

pub fn detect_scenes_streaming<D, S, T>(
    detector: D,
    mut source: S,
    options: DetectionOptions,
    stats_sink: &mut T,
) -> Result<SceneList>
where
    D: Detector,
    S: FrameSource,
    T: DetectionStatsSink,
{
    let frame_rate = source.frame_rate();
    let (boundaries, total_frames) = match detector.config() {
        DetectorConfig::Content(config) => {
            detect_content_streaming(&mut source, config, options.min_scene_len, stats_sink)?
        }
        DetectorConfig::Adaptive(config) => {
            detect_adaptive_streaming(&mut source, config, options.min_scene_len, stats_sink)?
        }
        DetectorConfig::Threshold(config) => {
            detect_threshold_streaming(&mut source, config, options.min_scene_len, stats_sink)?
        }
        DetectorConfig::Histogram(config) => {
            detect_histogram_streaming(&mut source, config, options.min_scene_len, stats_sink)?
        }
        DetectorConfig::Hash(config) => {
            detect_hash_streaming(&mut source, config, options.min_scene_len, stats_sink)?
        }
    };
    Ok(build_scene_list(
        frame_rate,
        total_frames,
        boundaries,
        options,
    ))
}

pub fn detect_boundary_review_streaming<D, S, T>(
    detector: D,
    mut source: S,
    options: DetectionOptions,
    review_options: BoundaryReviewOptions,
    stats_sink: &mut T,
) -> Result<BoundaryReview>
where
    D: Detector,
    S: FrameSource,
    T: DetectionStatsSink,
{
    let frame_rate = source.frame_rate();
    match detector.config() {
        DetectorConfig::Content(config) => detect_content_boundary_review_streaming(
            &mut source,
            frame_rate,
            config,
            options,
            review_options,
            stats_sink,
        ),
        DetectorConfig::Adaptive(config) => detect_adaptive_boundary_review_streaming(
            &mut source,
            frame_rate,
            config,
            options,
            review_options,
            stats_sink,
        ),
        DetectorConfig::Threshold(_) => Err(SceneDetectError::UnsupportedBoundaryReview(
            "detect-threshold".to_owned(),
        )),
        DetectorConfig::Histogram(_) => Err(SceneDetectError::UnsupportedBoundaryReview(
            "detect-hist".to_owned(),
        )),
        DetectorConfig::Hash(_) => Err(SceneDetectError::UnsupportedBoundaryReview(
            "detect-hash".to_owned(),
        )),
    }
}

struct SliceFrameSource<'a> {
    frame_rate: FrameRate,
    frames: std::slice::Iter<'a, Frame>,
}

impl FrameSource for SliceFrameSource<'_> {
    fn frame_rate(&self) -> FrameRate {
        self.frame_rate
    }

    fn next_frame(&mut self) -> Result<Option<Frame>> {
        Ok(self.frames.next().cloned())
    }
}
