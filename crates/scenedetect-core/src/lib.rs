mod config;
mod detect;
mod error;
mod frame;
mod output;
mod review;
mod scene;
mod session;
mod session_content;
mod stats;
mod timeline;

#[cfg(test)]
mod tests;

pub use config::{
    AdaptiveDetectorConfig, ContentDetectorConfig, ContentWeights, DetectionOptions, Detector,
    DetectorConfig, HashDetectorConfig, HistogramDetectorConfig, MinSceneLenPolicy,
    ThresholdDetectorConfig,
};
pub use detect::{
    boundary_review_from_content_detection_stats, detect_boundary_review_streaming,
    detect_content_stats, detect_frames, detect_scenes, detect_scenes_streaming,
    detection_stats_from_content_detection_stats, scene_list_from_content_detection_stats,
};
pub use error::{Result, SceneDetectError};
pub use frame::{
    Frame, FrameIndex, FrameRate, FrameSource, FrameTiming, FrameWithTiming, MediaTime, TimeBase,
    Timecode,
};
pub use output::{
    write_boundary_review_csv, write_boundary_review_json, write_scene_events_ndjson,
    write_scene_list_csv, write_scene_list_html, write_scene_list_json, write_stats_csv,
};
pub use review::{
    BoundaryCandidateReview, BoundaryCandidateStatus, BoundaryReview, BoundaryReviewOptions,
    ReviewSceneContext,
};
pub use scene::{DetectionResult, SceneBoundary, SceneList, SceneSpan};
pub use session::DetectionSession;
pub use stats::{
    ContentDetectionStats, CsvStatsSink, DetectionStats, DetectionStatsDecision,
    DetectionStatsSink, NoopStatsSink, RichDetectionStatsRow, StatsRow,
};
pub use timeline::{scene_timeline_from_source, SceneTimeline, SceneTimelineSpan};
