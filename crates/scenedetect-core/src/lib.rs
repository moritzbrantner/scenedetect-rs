use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::io::Write;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

mod output;

pub use output::{
    write_boundary_review_csv, write_boundary_review_json, write_scene_events_ndjson,
    write_scene_list_csv, write_scene_list_html, write_scene_list_json, write_stats_csv,
};

pub type Result<T> = std::result::Result<T, SceneDetectError>;

#[derive(Debug, Error)]
pub enum SceneDetectError {
    #[error("invalid timecode: {0}")]
    InvalidTimecode(String),
    #[error("boundary review is not supported for {0}")]
    UnsupportedBoundaryReview(String),
    #[error("frame source error: {0}")]
    FrameSource(String),
    #[error("csv error: {0}")]
    Csv(#[from] csv::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FrameIndex(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrameRate(pub f64);

impl FrameRate {
    pub fn frames_from_seconds(self, seconds: f64) -> u64 {
        (seconds * self.0).round() as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timecode {
    frames: u64,
}

impl Timecode {
    pub fn from_frames(frames: u64) -> Self {
        Self { frames }
    }

    pub fn frames(self) -> u64 {
        self.frames
    }

    pub fn parse_at_rate(input: &str, frame_rate: FrameRate) -> Result<Self> {
        if input.contains(':') {
            let parts: Vec<_> = input.split(':').collect();
            if parts.len() != 3 {
                return Err(SceneDetectError::InvalidTimecode(input.to_owned()));
            }
            let hours = parts[0]
                .parse::<f64>()
                .map_err(|_| SceneDetectError::InvalidTimecode(input.to_owned()))?;
            let minutes = parts[1]
                .parse::<f64>()
                .map_err(|_| SceneDetectError::InvalidTimecode(input.to_owned()))?;
            let seconds = parts[2]
                .parse::<f64>()
                .map_err(|_| SceneDetectError::InvalidTimecode(input.to_owned()))?;
            return Ok(Self::from_frames(
                frame_rate.frames_from_seconds(hours * 3600.0 + minutes * 60.0 + seconds),
            ));
        }

        if let Some(seconds) = input.strip_suffix('s') {
            let seconds = seconds
                .parse::<f64>()
                .map_err(|_| SceneDetectError::InvalidTimecode(input.to_owned()))?;
            return Ok(Self::from_frames(frame_rate.frames_from_seconds(seconds)));
        }

        let frames = input
            .parse::<u64>()
            .map_err(|_| SceneDetectError::InvalidTimecode(input.to_owned()))?;
        Ok(Self::from_frames(frames))
    }

    pub fn display_at_rate(self, frame_rate: FrameRate) -> String {
        let total_seconds = self.frames as f64 / frame_rate.0;
        let hours = (total_seconds / 3600.0).floor() as u64;
        let minutes = ((total_seconds % 3600.0) / 60.0).floor() as u64;
        let seconds = total_seconds % 60.0;
        format!("{hours:02}:{minutes:02}:{seconds:06.3}")
    }
}

impl FromStr for Timecode {
    type Err = SceneDetectError;

    fn from_str(value: &str) -> Result<Self> {
        let frames = value
            .parse::<u64>()
            .map_err(|_| SceneDetectError::InvalidTimecode(value.to_owned()))?;
        Ok(Self::from_frames(frames))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub index: FrameIndex,
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl Frame {
    pub fn solid(index: u64, width: u32, height: u32, rgb: [u8; 3]) -> Self {
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 3);
        for _ in 0..(width as usize * height as usize) {
            pixels.extend_from_slice(&rgb);
        }

        Self {
            index: FrameIndex(index),
            width,
            height,
            rgb: pixels,
        }
    }

    pub fn mean_luma(&self) -> f64 {
        let mut sum = 0.0;
        let mut count = 0_u64;
        for px in self.rgb.chunks_exact(3) {
            sum += 0.299 * px[0] as f64 + 0.587 * px[1] as f64 + 0.114 * px[2] as f64;
            count += 1;
        }
        if count == 0 {
            0.0
        } else {
            sum / count as f64
        }
    }
}

pub trait FrameSource {
    fn frame_rate(&self) -> FrameRate;
    fn next_frame(&mut self) -> Result<Option<Frame>>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneBoundary {
    pub frame: FrameIndex,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneSpan {
    pub start: FrameIndex,
    pub end: FrameIndex,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneList {
    pub frame_rate: FrameRate,
    pub scenes: Vec<SceneSpan>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DetectionStats {
    pub metric_names: Vec<String>,
    pub rows: Vec<StatsRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatsRow {
    pub frame: FrameIndex,
    pub metrics: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionStatsDecision {
    NotEvaluated,
    Accepted,
    SuppressedMinSceneLen,
    BelowThreshold,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RichDetectionStatsRow {
    pub frame: FrameIndex,
    pub score: f64,
    pub threshold: f64,
    pub decision: DetectionStatsDecision,
    pub metrics: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentDetectionStats {
    pub frame_rate: FrameRate,
    pub total_frames: u64,
    pub detector_config: ContentDetectorConfig,
    pub options: DetectionOptions,
    pub metric_names: Vec<String>,
    pub rows: Vec<RichDetectionStatsRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectionResult {
    pub scene_list: SceneList,
    pub stats: DetectionStats,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BoundaryReviewOptions {
    pub review_threshold: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryReview {
    pub frame_rate: FrameRate,
    pub detector: String,
    pub score_metric: String,
    pub detector_threshold: f64,
    pub review_threshold: f64,
    pub scene_list: SceneList,
    pub candidates: Vec<BoundaryCandidateReview>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryCandidateReview {
    pub candidate_number: usize,
    pub status: BoundaryCandidateStatus,
    pub frame: FrameIndex,
    pub score_metric: String,
    pub score: f64,
    pub detector_threshold: f64,
    pub review_threshold: f64,
    pub threshold_distance: f64,
    pub metrics: BTreeMap<String, f64>,
    pub before: ReviewSceneContext,
    pub after: ReviewSceneContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryCandidateStatus {
    Accepted,
    SuppressedMinSceneLen,
    NearMiss,
}

impl BoundaryCandidateStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::SuppressedMinSceneLen => "suppressed_min_scene_len",
            Self::NearMiss => "near_miss",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewSceneContext {
    pub start: FrameIndex,
    pub end: FrameIndex,
}

pub trait DetectionStatsSink {
    fn start(&mut self, metric_names: &[String]) -> Result<()>;
    fn row(&mut self, row: StatsRow) -> Result<()>;
    fn finish(&mut self) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct NoopStatsSink;

impl DetectionStatsSink for NoopStatsSink {
    fn start(&mut self, _metric_names: &[String]) -> Result<()> {
        Ok(())
    }

    fn row(&mut self, _row: StatsRow) -> Result<()> {
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct CsvStatsSink<W: Write> {
    csv: csv::Writer<W>,
    metric_names: Vec<String>,
}

impl<W: Write> CsvStatsSink<W> {
    pub fn new(writer: W) -> Self {
        Self {
            csv: csv::Writer::from_writer(writer),
            metric_names: Vec::new(),
        }
    }
}

impl<W: Write> DetectionStatsSink for CsvStatsSink<W> {
    fn start(&mut self, metric_names: &[String]) -> Result<()> {
        self.metric_names = metric_names.to_vec();
        let mut header = vec!["Frame Number".to_owned()];
        header.extend(self.metric_names.iter().cloned());
        self.csv.write_record(header)?;
        Ok(())
    }

    fn row(&mut self, row: StatsRow) -> Result<()> {
        let mut record = vec![row.frame.0.to_string()];
        for metric in &self.metric_names {
            record.push(format!(
                "{:.6}",
                row.metrics.get(metric).copied().unwrap_or(0.0)
            ));
        }
        self.csv.write_record(record)?;
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        self.csv.flush()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MinSceneLenPolicy {
    Suppress,
    MergeLast,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectionOptions {
    pub min_scene_len: u64,
    pub min_scene_len_policy: MinSceneLenPolicy,
}

impl Default for DetectionOptions {
    fn default() -> Self {
        Self {
            min_scene_len: 15,
            min_scene_len_policy: MinSceneLenPolicy::Suppress,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentDetectorConfig {
    pub threshold: f64,
    pub weights: ContentWeights,
    pub luma_only: bool,
}

impl Default for ContentDetectorConfig {
    fn default() -> Self {
        Self {
            threshold: 27.0,
            weights: ContentWeights::default(),
            luma_only: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveDetectorConfig {
    pub threshold: f64,
    pub min_content_val: f64,
    pub frame_window: usize,
    pub weights: ContentWeights,
    pub luma_only: bool,
}

impl Default for AdaptiveDetectorConfig {
    fn default() -> Self {
        Self {
            threshold: 3.0,
            min_content_val: 15.0,
            frame_window: 2,
            weights: ContentWeights::default(),
            luma_only: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThresholdDetectorConfig {
    pub threshold: f64,
    pub fade_bias: f64,
    pub add_last_scene: bool,
}

impl Default for ThresholdDetectorConfig {
    fn default() -> Self {
        Self {
            threshold: 12.0,
            fade_bias: 0.0,
            add_last_scene: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistogramDetectorConfig {
    pub threshold: f64,
    pub bins: usize,
}

impl Default for HistogramDetectorConfig {
    fn default() -> Self {
        Self {
            threshold: 0.05,
            bins: 256,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HashDetectorConfig {
    pub threshold: f64,
    pub size: usize,
    pub lowpass: usize,
}

impl Default for HashDetectorConfig {
    fn default() -> Self {
        Self {
            threshold: 0.395,
            size: 16,
            lowpass: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentWeights {
    pub hue: f64,
    pub saturation: f64,
    pub luminance: f64,
    pub edges: f64,
}

impl Default for ContentWeights {
    fn default() -> Self {
        Self {
            hue: 1.0,
            saturation: 1.0,
            luminance: 1.0,
            edges: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DetectorConfig {
    Content(ContentDetectorConfig),
    Adaptive(AdaptiveDetectorConfig),
    Threshold(ThresholdDetectorConfig),
    Histogram(HistogramDetectorConfig),
    Hash(HashDetectorConfig),
}

pub trait Detector {
    fn config(&self) -> DetectorConfig;
}

impl Detector for DetectorConfig {
    fn config(&self) -> DetectorConfig {
        self.clone()
    }
}

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

#[derive(Debug)]
struct CollectingStatsSink {
    stats: DetectionStats,
}

impl CollectingStatsSink {
    fn into_stats(self) -> DetectionStats {
        self.stats
    }
}

impl Default for CollectingStatsSink {
    fn default() -> Self {
        Self {
            stats: DetectionStats {
                metric_names: Vec::new(),
                rows: Vec::new(),
            },
        }
    }
}

impl DetectionStatsSink for CollectingStatsSink {
    fn start(&mut self, metric_names: &[String]) -> Result<()> {
        self.stats.metric_names = metric_names.to_vec();
        self.stats.rows.clear();
        Ok(())
    }

    fn row(&mut self, row: StatsRow) -> Result<()> {
        self.stats.rows.push(row);
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        Ok(())
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

#[derive(Debug, Clone)]
struct BoundaryCandidateSeed {
    status: BoundaryCandidateStatus,
    frame: FrameIndex,
    score_metric: String,
    score: f64,
    detector_threshold: f64,
    review_threshold: f64,
    metrics: BTreeMap<String, f64>,
}

impl BoundaryCandidateSeed {
    fn into_review(
        self,
        candidate_number: usize,
        scene_list: &SceneList,
    ) -> BoundaryCandidateReview {
        let (before, after) = review_context(scene_list, self.frame);
        BoundaryCandidateReview {
            candidate_number,
            status: self.status,
            frame: self.frame,
            score_metric: self.score_metric,
            score: self.score,
            detector_threshold: self.detector_threshold,
            review_threshold: self.review_threshold,
            threshold_distance: (self.score - self.detector_threshold).abs(),
            metrics: self.metrics,
            before,
            after,
        }
    }
}

fn detect_content_boundary_review_streaming<S, T>(
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

fn detect_content_streaming<S, T>(
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

fn detect_adaptive_streaming<S, T>(
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

fn detect_adaptive_boundary_review_streaming<S, T>(
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
struct AdaptiveSample {
    position: usize,
    frame: FrameIndex,
    content_val: f64,
}

#[allow(clippy::too_many_arguments)]
fn emit_ready_adaptive_rows<T>(
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

fn detect_threshold_streaming<S, T>(
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

fn detect_histogram_streaming<S, T>(
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

fn detect_hash_streaming<S, T>(
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FadeType {
    In,
    Out,
}

fn round_half_to_even(value: f64) -> u64 {
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

fn content_score(
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
    let mut weighted_sum = 0.0;
    let mut hue_sum = 0.0;
    let mut saturation_sum = 0.0;
    let mut luminance_sum = 0.0;
    let mut pixel_count = 0.0;
    let channel_weight_total = weights.hue + weights.saturation + weights.luminance;

    for (prev, curr) in previous
        .rgb
        .chunks_exact(3)
        .zip(current.rgb.chunks_exact(3))
    {
        pixel_count += 1.0;
        if luma_only {
            let prev_luma =
                0.299 * prev[0] as f64 + 0.587 * prev[1] as f64 + 0.114 * prev[2] as f64;
            let curr_luma =
                0.299 * curr[0] as f64 + 0.587 * curr[1] as f64 + 0.114 * curr[2] as f64;
            let luminance = (prev_luma - curr_luma).abs();
            luminance_sum += luminance;
            weighted_sum += luminance;
        } else {
            let hue = (prev[0] as f64 - curr[0] as f64).abs();
            let saturation = (prev[1] as f64 - curr[1] as f64).abs();
            let luminance = (prev[2] as f64 - curr[2] as f64).abs();
            hue_sum += hue;
            saturation_sum += saturation;
            luminance_sum += luminance;
            weighted_sum += hue * weights.hue;
            weighted_sum += saturation * weights.saturation;
            weighted_sum += luminance * weights.luminance;
        }
    }

    let denominator = if luma_only {
        pixel_count
    } else {
        pixel_count * channel_weight_total
    };

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
        ("delta_edges".to_owned(), 0.0),
    ])
}

fn luma_histogram(frame: &Frame, bins: usize) -> Vec<f64> {
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

fn histogram_correlation(previous: &[f64], current: &[f64]) -> f64 {
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

fn perceptual_hash(frame: &Frame, size: usize, lowpass: usize) -> Vec<bool> {
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

fn hash_distance(previous: &[bool], current: &[bool]) -> f64 {
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

fn rgb_luma(px: &[u8]) -> f64 {
    0.299 * px[0] as f64 + 0.587 * px[1] as f64 + 0.114 * px[2] as f64
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

fn build_scene_list(
    frame_rate: FrameRate,
    total_frames: u64,
    boundaries: Vec<SceneBoundary>,
    options: DetectionOptions,
) -> SceneList {
    if total_frames == 0 {
        return SceneList {
            frame_rate,
            scenes: Vec::new(),
        };
    }

    let mut starts = vec![0];
    starts.extend(boundaries.into_iter().map(|boundary| boundary.frame.0));
    starts.sort_unstable();
    starts.dedup();

    let mut scenes = Vec::new();
    for pair in starts.windows(2) {
        scenes.push(SceneSpan {
            start: FrameIndex(pair[0]),
            end: FrameIndex(pair[1]),
        });
    }
    scenes.push(SceneSpan {
        start: FrameIndex(*starts.last().unwrap_or(&0)),
        end: FrameIndex(total_frames),
    });

    if matches!(options.min_scene_len_policy, MinSceneLenPolicy::MergeLast) && scenes.len() > 1 {
        let last_len = scenes
            .last()
            .map(|scene| scene.end.0 - scene.start.0)
            .unwrap_or(0);
        if last_len < options.min_scene_len {
            let last = scenes.pop().expect("last scene exists");
            if let Some(previous) = scenes.last_mut() {
                previous.end = last.end;
            }
        }
    }

    SceneList { frame_rate, scenes }
}

#[allow(clippy::too_many_arguments)]
fn build_boundary_review(
    frame_rate: FrameRate,
    detector: &str,
    score_metric: &str,
    detector_threshold: f64,
    review_threshold: f64,
    total_frames: u64,
    boundaries: Vec<SceneBoundary>,
    options: DetectionOptions,
    candidate_seeds: Vec<BoundaryCandidateSeed>,
) -> BoundaryReview {
    let scene_list = build_scene_list(frame_rate, total_frames, boundaries, options);
    let mut candidates: Vec<_> = candidate_seeds
        .into_iter()
        .enumerate()
        .map(|(idx, candidate)| candidate.into_review(idx + 1, &scene_list))
        .collect();
    candidates.sort_by(|left, right| {
        left.threshold_distance
            .partial_cmp(&right.threshold_distance)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.frame.cmp(&right.frame))
    });

    BoundaryReview {
        frame_rate,
        detector: detector.to_owned(),
        score_metric: score_metric.to_owned(),
        detector_threshold,
        review_threshold,
        scene_list,
        candidates,
    }
}

fn review_context(
    scene_list: &SceneList,
    frame: FrameIndex,
) -> (ReviewSceneContext, ReviewSceneContext) {
    if scene_list.scenes.is_empty() {
        return (
            ReviewSceneContext {
                start: FrameIndex(0),
                end: FrameIndex(0),
            },
            ReviewSceneContext {
                start: FrameIndex(0),
                end: FrameIndex(0),
            },
        );
    }

    if let Some(index) = scene_list
        .scenes
        .iter()
        .position(|scene| scene.start == frame && frame.0 > 0)
    {
        let before = scene_list.scenes[index - 1].clone();
        let after = scene_list.scenes[index].clone();
        return (
            ReviewSceneContext {
                start: before.start,
                end: before.end,
            },
            ReviewSceneContext {
                start: after.start,
                end: after.end,
            },
        );
    }

    let containing = scene_list
        .scenes
        .iter()
        .find(|scene| frame.0 >= scene.start.0 && frame.0 < scene.end.0)
        .or_else(|| scene_list.scenes.last())
        .expect("non-empty scene list has a last scene");

    (
        ReviewSceneContext {
            start: containing.start,
            end: frame,
        },
        ReviewSceneContext {
            start: frame,
            end: containing.end,
        },
    )
}

impl fmt::Display for FrameIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    struct VecFrameSource {
        frame_rate: FrameRate,
        frames: std::vec::IntoIter<Frame>,
    }

    impl VecFrameSource {
        fn new(frames: Vec<Frame>) -> Self {
            Self {
                frame_rate: FrameRate(10.0),
                frames: frames.into_iter(),
            }
        }
    }

    impl FrameSource for VecFrameSource {
        fn frame_rate(&self) -> FrameRate {
            self.frame_rate
        }

        fn next_frame(&mut self) -> Result<Option<Frame>> {
            Ok(self.frames.next())
        }
    }

    fn frames(colors: &[[u8; 3]]) -> Vec<Frame> {
        colors
            .iter()
            .enumerate()
            .map(|(index, color)| Frame::solid(index as u64, 2, 2, *color))
            .collect()
    }

    fn structural_pattern_frame(index: u64) -> Frame {
        let width = 64;
        let height = 64;
        let mut rgb = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let value = ((x * 37 + y * 17 + x * y * 3) % 256) as u8;
                rgb.extend_from_slice(&[value, value, value]);
            }
        }
        Frame {
            index: FrameIndex(index),
            width,
            height,
            rgb,
        }
    }

    struct CountingFrameSource {
        frame_rate: FrameRate,
        frames: std::vec::IntoIter<Frame>,
        reads: Rc<Cell<usize>>,
    }

    impl CountingFrameSource {
        fn new(frames: Vec<Frame>, reads: Rc<Cell<usize>>) -> Self {
            Self {
                frame_rate: FrameRate(10.0),
                frames: frames.into_iter(),
                reads,
            }
        }
    }

    impl FrameSource for CountingFrameSource {
        fn frame_rate(&self) -> FrameRate {
            self.frame_rate
        }

        fn next_frame(&mut self) -> Result<Option<Frame>> {
            self.reads.set(self.reads.get() + 1);
            Ok(self.frames.next())
        }
    }

    struct StreamingProbeStatsSink {
        reads: Rc<Cell<usize>>,
        eof_read_count: usize,
        saw_row_before_eof: bool,
    }

    impl DetectionStatsSink for StreamingProbeStatsSink {
        fn start(&mut self, _metric_names: &[String]) -> Result<()> {
            Ok(())
        }

        fn row(&mut self, _row: StatsRow) -> Result<()> {
            if self.reads.get() < self.eof_read_count {
                self.saw_row_before_eof = true;
            }
            Ok(())
        }

        fn finish(&mut self) -> Result<()> {
            Ok(())
        }
    }

    struct MemoryStatsSink {
        stats: DetectionStats,
    }

    impl Default for MemoryStatsSink {
        fn default() -> Self {
            Self {
                stats: DetectionStats {
                    metric_names: Vec::new(),
                    rows: Vec::new(),
                },
            }
        }
    }

    impl DetectionStatsSink for MemoryStatsSink {
        fn start(&mut self, metric_names: &[String]) -> Result<()> {
            self.stats.metric_names = metric_names.to_vec();
            self.stats.rows.clear();
            Ok(())
        }

        fn row(&mut self, row: StatsRow) -> Result<()> {
            self.stats.rows.push(row);
            Ok(())
        }

        fn finish(&mut self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn streaming_detection_emits_stats_before_frame_source_is_exhausted() {
        let frames = frames(&[[0, 0, 0], [255, 255, 255], [255, 255, 255]]);
        let eof_read_count = frames.len() + 1;
        let reads = Rc::new(Cell::new(0));
        let source = CountingFrameSource::new(frames, Rc::clone(&reads));
        let mut stats_sink = StreamingProbeStatsSink {
            reads,
            eof_read_count,
            saw_row_before_eof: false,
        };

        let scene_list = detect_scenes_streaming(
            DetectorConfig::Content(ContentDetectorConfig {
                threshold: 20.0,
                ..Default::default()
            }),
            source,
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
            &mut stats_sink,
        )
        .unwrap();

        assert_eq!(
            scene_list.scenes,
            vec![
                SceneSpan {
                    start: FrameIndex(0),
                    end: FrameIndex(1)
                },
                SceneSpan {
                    start: FrameIndex(1),
                    end: FrameIndex(3)
                }
            ]
        );
        assert!(
            stats_sink.saw_row_before_eof,
            "stats rows should be emitted while the Frame Source is still streaming"
        );
    }

    #[test]
    fn streaming_detection_matches_collecting_api_for_supported_detectors() {
        let detector_cases = [
            (
                DetectorConfig::Content(ContentDetectorConfig {
                    threshold: 20.0,
                    ..Default::default()
                }),
                frames(&[[0, 0, 0], [0, 0, 0], [255, 255, 255], [255, 255, 255]]),
            ),
            (
                DetectorConfig::Adaptive(AdaptiveDetectorConfig {
                    threshold: 3.0,
                    min_content_val: 20.0,
                    frame_window: 1,
                    ..Default::default()
                }),
                frames(&[
                    [0, 0, 0],
                    [3, 3, 3],
                    [6, 6, 6],
                    [255, 255, 255],
                    [252, 252, 252],
                    [249, 249, 249],
                ]),
            ),
            (
                DetectorConfig::Threshold(ThresholdDetectorConfig {
                    threshold: 12.0,
                    ..Default::default()
                }),
                frames(&[
                    [0, 0, 0],
                    [0, 0, 0],
                    [128, 128, 128],
                    [255, 255, 255],
                    [255, 255, 255],
                ]),
            ),
            (
                DetectorConfig::Histogram(HistogramDetectorConfig {
                    threshold: 0.05,
                    bins: 256,
                }),
                frames(&[[0, 0, 0], [0, 0, 0], [255, 255, 255], [255, 255, 255]]),
            ),
            (
                DetectorConfig::Hash(HashDetectorConfig {
                    threshold: 0.395,
                    size: 16,
                    lowpass: 2,
                }),
                vec![
                    Frame::solid(0, 64, 64, [0, 0, 0]),
                    Frame::solid(1, 64, 64, [0, 0, 0]),
                    structural_pattern_frame(2),
                    structural_pattern_frame(3),
                ],
            ),
        ];

        for (detector, frames) in detector_cases {
            let options = DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            };
            let collected = detect_scenes(
                detector.clone(),
                VecFrameSource::new(frames.clone()),
                options.clone(),
            )
            .unwrap();
            let mut stats_sink = MemoryStatsSink::default();
            let scene_list = detect_scenes_streaming(
                detector,
                VecFrameSource::new(frames),
                options,
                &mut stats_sink,
            )
            .unwrap();

            assert_eq!(scene_list, collected.scene_list);
            assert_eq!(stats_sink.stats, collected.stats);
        }
    }

    #[test]
    fn timecode_parses_frames_seconds_and_clock_values() {
        let rate = FrameRate(10.0);

        assert_eq!(Timecode::parse_at_rate("12", rate).unwrap().frames(), 12);
        assert_eq!(Timecode::parse_at_rate("1.5s", rate).unwrap().frames(), 15);
        assert_eq!(
            Timecode::parse_at_rate("00:00:02.500", rate)
                .unwrap()
                .frames(),
            25
        );
    }

    #[test]
    fn empty_video_emits_no_scenes_and_no_stats_rows() {
        let result = detect_frames(
            DetectorConfig::Content(ContentDetectorConfig::default()),
            FrameRate(10.0),
            &[],
            DetectionOptions::default(),
        )
        .unwrap();

        assert!(result.scene_list.scenes.is_empty());
        assert_eq!(result.stats.metric_names, vec!["content_val"]);
        assert!(result.stats.rows.is_empty());
    }

    #[test]
    fn single_scene_video_spans_all_decoded_frames() {
        let frames = frames(&[[0, 0, 0], [0, 0, 0], [0, 0, 0]]);
        let result = detect_frames(
            DetectorConfig::Content(ContentDetectorConfig {
                threshold: 20.0,
                ..Default::default()
            }),
            FrameRate(10.0),
            &frames,
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            result.scene_list.scenes,
            vec![SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(3)
            }]
        );
        assert_eq!(result.stats.rows.len(), frames.len());
    }

    #[test]
    fn content_detector_emits_scene_list_for_hard_color_change() {
        let frames = frames(&[
            [0, 0, 0],
            [0, 0, 0],
            [0, 0, 0],
            [255, 255, 255],
            [255, 255, 255],
        ]);
        let result = detect_scenes(
            DetectorConfig::Content(ContentDetectorConfig {
                threshold: 20.0,
                ..Default::default()
            }),
            VecFrameSource::new(frames),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            result.scene_list.scenes,
            vec![
                SceneSpan {
                    start: FrameIndex(0),
                    end: FrameIndex(3)
                },
                SceneSpan {
                    start: FrameIndex(3),
                    end: FrameIndex(5)
                }
            ]
        );
        assert_eq!(result.stats.metric_names, vec!["content_val"]);
    }

    #[test]
    fn content_detector_min_scene_len_suppresses_close_scene_boundaries() {
        let frames = frames(&[
            [0, 0, 0],
            [0, 0, 0],
            [255, 255, 255],
            [255, 255, 255],
            [255, 255, 255],
            [255, 255, 255],
            [0, 0, 0],
            [0, 0, 0],
            [0, 0, 0],
            [0, 0, 0],
            [0, 0, 0],
            [0, 0, 0],
        ]);
        let result = detect_frames(
            DetectorConfig::Content(ContentDetectorConfig {
                threshold: 20.0,
                ..Default::default()
            }),
            FrameRate(10.0),
            &frames,
            DetectionOptions {
                min_scene_len: 5,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            result.scene_list.scenes,
            vec![SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(12)
            }]
        );
    }

    #[test]
    fn content_detector_threshold_controls_scene_boundary_sensitivity() {
        let frames = frames(&[[0, 0, 0], [0, 0, 0], [50, 50, 50], [50, 50, 50]]);

        let lower_threshold = detect_frames(
            DetectorConfig::Content(ContentDetectorConfig {
                threshold: 20.0,
                ..Default::default()
            }),
            FrameRate(10.0),
            &frames,
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let higher_threshold = detect_frames(
            DetectorConfig::Content(ContentDetectorConfig {
                threshold: 80.0,
                ..Default::default()
            }),
            FrameRate(10.0),
            &frames,
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(lower_threshold.scene_list.scenes[0].end, FrameIndex(2));
        assert_eq!(
            higher_threshold.scene_list.scenes,
            vec![SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(4)
            }]
        );
    }

    #[test]
    fn content_detection_stats_derive_scene_list_and_boundary_review() {
        let frames = frames(&[[0, 0, 0], [0, 0, 0], [255, 255, 255], [255, 255, 255]]);
        let stats = detect_content_stats(
            VecFrameSource::new(frames),
            ContentDetectorConfig {
                threshold: 20.0,
                ..Default::default()
            },
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(stats.total_frames, 4);
        assert_eq!(stats.metric_names[0], "content_val");
        assert_eq!(stats.rows[0].decision, DetectionStatsDecision::NotEvaluated);
        assert_eq!(
            stats.rows[1].decision,
            DetectionStatsDecision::BelowThreshold
        );
        assert_eq!(stats.rows[2].decision, DetectionStatsDecision::Accepted);
        assert_eq!(stats.rows[2].threshold, 20.0);
        assert!(stats.rows[2].score >= 20.0);
        assert!(stats.rows[2].metrics.contains_key("delta_luminance"));

        let scene_list = scene_list_from_content_detection_stats(&stats);
        assert_eq!(
            scene_list.scenes,
            vec![
                SceneSpan {
                    start: FrameIndex(0),
                    end: FrameIndex(2)
                },
                SceneSpan {
                    start: FrameIndex(2),
                    end: FrameIndex(4)
                }
            ]
        );

        let review = boundary_review_from_content_detection_stats(
            &stats,
            BoundaryReviewOptions {
                review_threshold: Some(10.0),
            },
        );
        assert_eq!(review.candidates.len(), 1);
        assert_eq!(
            review.candidates[0].status,
            BoundaryCandidateStatus::Accepted
        );
        assert_eq!(review.candidates[0].frame, FrameIndex(2));
    }

    #[test]
    fn content_boundary_review_classifies_candidates_without_changing_scene_list() {
        let frames = frames(&[[0, 0, 0], [50, 50, 50], [200, 200, 200], [50, 50, 50]]);
        let mut stats_sink = NoopStatsSink;
        let review = detect_boundary_review_streaming(
            DetectorConfig::Content(ContentDetectorConfig {
                threshold: 100.0,
                ..Default::default()
            }),
            VecFrameSource::new(frames),
            DetectionOptions {
                min_scene_len: 2,
                ..Default::default()
            },
            BoundaryReviewOptions {
                review_threshold: Some(50.0),
            },
            &mut stats_sink,
        )
        .unwrap();

        assert_eq!(
            review
                .candidates
                .iter()
                .map(|candidate| candidate.status)
                .collect::<Vec<_>>(),
            vec![
                BoundaryCandidateStatus::NearMiss,
                BoundaryCandidateStatus::Accepted,
                BoundaryCandidateStatus::SuppressedMinSceneLen,
            ]
        );
        assert_eq!(
            review.scene_list.scenes,
            vec![
                SceneSpan {
                    start: FrameIndex(0),
                    end: FrameIndex(2)
                },
                SceneSpan {
                    start: FrameIndex(2),
                    end: FrameIndex(4)
                }
            ]
        );
    }

    #[test]
    fn content_boundary_review_defaults_to_eighty_percent_of_detector_threshold() {
        let frames = frames(&[[0, 0, 0], [70, 70, 70], [200, 200, 200]]);
        let mut stats_sink = NoopStatsSink;
        let review = detect_boundary_review_streaming(
            DetectorConfig::Content(ContentDetectorConfig {
                threshold: 100.0,
                ..Default::default()
            }),
            VecFrameSource::new(frames),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
            BoundaryReviewOptions::default(),
            &mut stats_sink,
        )
        .unwrap();

        assert_eq!(review.review_threshold, 80.0);
        assert_eq!(review.candidates.len(), 1);
        assert_eq!(review.candidates[0].frame, FrameIndex(2));
    }

    #[test]
    fn boundary_review_sorts_candidates_by_distance_to_detector_threshold() {
        let frames = frames(&[[0, 0, 0], [80, 80, 80], [190, 190, 190], [90, 90, 90]]);
        let mut stats_sink = NoopStatsSink;
        let review = detect_boundary_review_streaming(
            DetectorConfig::Content(ContentDetectorConfig {
                threshold: 100.0,
                ..Default::default()
            }),
            VecFrameSource::new(frames),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
            BoundaryReviewOptions {
                review_threshold: Some(0.0),
            },
            &mut stats_sink,
        )
        .unwrap();

        assert_eq!(
            review
                .candidates
                .iter()
                .map(|candidate| candidate.frame)
                .collect::<Vec<_>>(),
            vec![FrameIndex(3), FrameIndex(2), FrameIndex(1), FrameIndex(0)]
        );
    }

    #[test]
    fn adaptive_boundary_review_keeps_min_content_value_as_noise_floor() {
        let frames = frames(&[
            [0, 0, 0],
            [1, 1, 1],
            [100, 100, 100],
            [101, 101, 101],
            [102, 102, 102],
        ]);

        let mut strict_stats_sink = NoopStatsSink;
        let strict_review = detect_boundary_review_streaming(
            DetectorConfig::Adaptive(AdaptiveDetectorConfig {
                threshold: 100.0,
                min_content_val: 100.0,
                frame_window: 1,
                ..Default::default()
            }),
            VecFrameSource::new(frames.clone()),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
            BoundaryReviewOptions {
                review_threshold: Some(50.0),
            },
            &mut strict_stats_sink,
        )
        .unwrap();

        let mut relaxed_stats_sink = NoopStatsSink;
        let relaxed_review = detect_boundary_review_streaming(
            DetectorConfig::Adaptive(AdaptiveDetectorConfig {
                threshold: 100.0,
                min_content_val: 90.0,
                frame_window: 1,
                ..Default::default()
            }),
            VecFrameSource::new(frames),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
            BoundaryReviewOptions {
                review_threshold: Some(50.0),
            },
            &mut relaxed_stats_sink,
        )
        .unwrap();

        assert!(strict_review.candidates.is_empty());
        assert_eq!(relaxed_review.candidates.len(), 1);
        assert_eq!(
            relaxed_review.candidates[0].status,
            BoundaryCandidateStatus::NearMiss
        );
    }

    #[test]
    fn content_detector_luma_only_ignores_chroma_only_scene_boundary() {
        let frames = frames(&[[255, 0, 0], [255, 0, 0], [0, 130, 0], [0, 130, 0]]);
        let result = detect_frames(
            DetectorConfig::Content(ContentDetectorConfig {
                threshold: 20.0,
                luma_only: true,
                ..Default::default()
            }),
            FrameRate(10.0),
            &frames,
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            result.scene_list.scenes,
            vec![SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(4)
            }]
        );
        assert!(result.stats.rows[2].metrics["content_val"] < 1.0);
    }

    #[test]
    fn adaptive_detector_uses_local_ratio_to_ignore_background_motion() {
        let frames = frames(&[
            [0, 0, 0],
            [3, 3, 3],
            [6, 6, 6],
            [255, 255, 255],
            [252, 252, 252],
            [249, 249, 249],
        ]);
        let result = detect_frames(
            DetectorConfig::Adaptive(AdaptiveDetectorConfig {
                threshold: 3.0,
                min_content_val: 20.0,
                frame_window: 1,
                ..Default::default()
            }),
            FrameRate(10.0),
            &frames,
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(result.scene_list.scenes[0].end, FrameIndex(3));
        assert_eq!(
            result.stats.metric_names,
            vec!["adaptive_ratio", "content_val"]
        );
    }

    #[test]
    fn adaptive_detector_options_control_scene_boundary_sensitivity() {
        let frames = frames(&[
            [0, 0, 0],
            [80, 80, 80],
            [100, 100, 100],
            [200, 200, 200],
            [220, 220, 220],
            [140, 140, 140],
            [140, 140, 140],
        ]);

        let sensitive = detect_scenes(
            DetectorConfig::Adaptive(AdaptiveDetectorConfig {
                threshold: 3.0,
                min_content_val: 90.0,
                frame_window: 1,
                ..Default::default()
            }),
            VecFrameSource::new(frames.clone()),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let higher_ratio_threshold = detect_scenes(
            DetectorConfig::Adaptive(AdaptiveDetectorConfig {
                threshold: 6.0,
                min_content_val: 90.0,
                frame_window: 1,
                ..Default::default()
            }),
            VecFrameSource::new(frames.clone()),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let higher_min_content_val = detect_scenes(
            DetectorConfig::Adaptive(AdaptiveDetectorConfig {
                threshold: 3.0,
                min_content_val: 101.0,
                frame_window: 1,
                ..Default::default()
            }),
            VecFrameSource::new(frames.clone()),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let wider_frame_window = detect_scenes(
            DetectorConfig::Adaptive(AdaptiveDetectorConfig {
                threshold: 3.0,
                min_content_val: 90.0,
                frame_window: 2,
                ..Default::default()
            }),
            VecFrameSource::new(frames),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            sensitive.scene_list.scenes,
            vec![
                SceneSpan {
                    start: FrameIndex(0),
                    end: FrameIndex(3)
                },
                SceneSpan {
                    start: FrameIndex(3),
                    end: FrameIndex(7)
                }
            ]
        );
        assert_eq!(
            higher_ratio_threshold.scene_list.scenes,
            vec![SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(7)
            }]
        );
        assert_eq!(
            higher_min_content_val.scene_list.scenes,
            higher_ratio_threshold.scene_list.scenes
        );
        assert_eq!(
            wider_frame_window.scene_list.scenes,
            higher_ratio_threshold.scene_list.scenes
        );
    }

    #[test]
    fn threshold_detector_places_fade_return_scene_boundary_between_fade_events() {
        let frames = frames(&[
            [0, 0, 0],
            [0, 0, 0],
            [0, 0, 0],
            [0, 0, 0],
            [128, 128, 128],
            [128, 128, 128],
            [255, 255, 255],
            [255, 255, 255],
            [255, 255, 255],
            [255, 255, 255],
        ]);
        let result = detect_scenes(
            DetectorConfig::Threshold(ThresholdDetectorConfig {
                threshold: 12.0,
                ..Default::default()
            }),
            VecFrameSource::new(frames),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            result.scene_list.scenes,
            vec![
                SceneSpan {
                    start: FrameIndex(0),
                    end: FrameIndex(2)
                },
                SceneSpan {
                    start: FrameIndex(2),
                    end: FrameIndex(10)
                }
            ]
        );
        assert_eq!(result.stats.metric_names, vec!["average_rgb"]);
    }

    #[test]
    fn threshold_detector_options_control_fade_scene_boundaries() {
        let fade_out_then_in = frames(&[
            [20, 20, 20],
            [20, 20, 20],
            [0, 0, 0],
            [0, 0, 0],
            [80, 80, 80],
            [80, 80, 80],
        ]);

        let default_bias = detect_scenes(
            DetectorConfig::Threshold(ThresholdDetectorConfig {
                threshold: 12.0,
                fade_bias: 0.0,
                add_last_scene: true,
            }),
            VecFrameSource::new(fade_out_then_in.clone()),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let no_dark_frames_at_threshold = detect_scenes(
            DetectorConfig::Threshold(ThresholdDetectorConfig {
                threshold: 0.0,
                fade_bias: 0.0,
                add_last_scene: true,
            }),
            VecFrameSource::new(fade_out_then_in.clone()),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let fade_out_bias = detect_scenes(
            DetectorConfig::Threshold(ThresholdDetectorConfig {
                threshold: 12.0,
                fade_bias: -1.0,
                add_last_scene: true,
            }),
            VecFrameSource::new(fade_out_then_in.clone()),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let fade_in_bias = detect_scenes(
            DetectorConfig::Threshold(ThresholdDetectorConfig {
                threshold: 12.0,
                fade_bias: 1.0,
                add_last_scene: true,
            }),
            VecFrameSource::new(fade_out_then_in),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(default_bias.scene_list.scenes[0].end, FrameIndex(3));
        assert_eq!(
            no_dark_frames_at_threshold.scene_list.scenes,
            vec![SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(6)
            }]
        );
        assert_eq!(fade_out_bias.scene_list.scenes[0].end, FrameIndex(2));
        assert_eq!(fade_in_bias.scene_list.scenes[0].end, FrameIndex(4));

        let ends_on_fade_out =
            frames(&[[80, 80, 80], [80, 80, 80], [0, 0, 0], [0, 0, 0], [0, 0, 0]]);
        let with_final_scene = detect_scenes(
            DetectorConfig::Threshold(ThresholdDetectorConfig {
                threshold: 12.0,
                fade_bias: 0.0,
                add_last_scene: true,
            }),
            VecFrameSource::new(ends_on_fade_out.clone()),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let without_final_scene = detect_scenes(
            DetectorConfig::Threshold(ThresholdDetectorConfig {
                threshold: 12.0,
                fade_bias: 0.0,
                add_last_scene: false,
            }),
            VecFrameSource::new(ends_on_fade_out),
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(with_final_scene.scene_list.scenes[0].end, FrameIndex(2));
        assert_eq!(
            without_final_scene.scene_list.scenes,
            vec![SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(5)
            }]
        );
    }

    #[test]
    fn histogram_detector_emits_scene_list_for_hard_luma_change() {
        let frames = frames(&[[0, 0, 0], [0, 0, 0], [255, 255, 255], [255, 255, 255]]);
        let result = detect_frames(
            DetectorConfig::Histogram(HistogramDetectorConfig {
                threshold: 0.05,
                bins: 256,
            }),
            FrameRate(10.0),
            &frames,
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            result.scene_list.scenes,
            vec![
                SceneSpan {
                    start: FrameIndex(0),
                    end: FrameIndex(2)
                },
                SceneSpan {
                    start: FrameIndex(2),
                    end: FrameIndex(4)
                }
            ]
        );
        assert_eq!(result.stats.metric_names, vec!["hist_diff [bins=256]"]);
        assert_eq!(result.stats.rows.len(), frames.len());
        assert_eq!(result.stats.rows[0].metrics["hist_diff [bins=256]"], 0.0);
    }

    #[test]
    fn hash_detector_emits_scene_list_for_structural_pattern_change() {
        let frames = vec![
            Frame::solid(0, 64, 64, [0, 0, 0]),
            Frame::solid(1, 64, 64, [0, 0, 0]),
            structural_pattern_frame(2),
            structural_pattern_frame(3),
        ];
        let result = detect_frames(
            DetectorConfig::Hash(HashDetectorConfig {
                threshold: 0.395,
                size: 16,
                lowpass: 2,
            }),
            FrameRate(10.0),
            &frames,
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            result.scene_list.scenes,
            vec![
                SceneSpan {
                    start: FrameIndex(0),
                    end: FrameIndex(2)
                },
                SceneSpan {
                    start: FrameIndex(2),
                    end: FrameIndex(4)
                }
            ]
        );
        assert_eq!(
            result.stats.metric_names,
            vec!["hash_dist [size=16 lowpass=2]"]
        );
        assert_eq!(result.stats.rows.len(), frames.len());
        assert_eq!(
            result.stats.rows[0].metrics["hash_dist [size=16 lowpass=2]"],
            0.0
        );
    }

    #[test]
    fn hash_detector_does_not_require_uniform_luma_change_as_scene_boundary() {
        let frames = frames(&[[0, 0, 0], [0, 0, 0], [255, 255, 255], [255, 255, 255]]);
        let result = detect_frames(
            DetectorConfig::Hash(HashDetectorConfig {
                threshold: 0.395,
                size: 4,
                lowpass: 2,
            }),
            FrameRate(10.0),
            &frames,
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(
            result.scene_list.scenes,
            vec![SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(4)
            }]
        );
    }

    #[test]
    fn scene_list_csv_uses_pyscenedetect_frame_and_timecode_columns() {
        let scene_list = SceneList {
            frame_rate: FrameRate(10.0),
            scenes: vec![
                SceneSpan {
                    start: FrameIndex(0),
                    end: FrameIndex(3),
                },
                SceneSpan {
                    start: FrameIndex(3),
                    end: FrameIndex(5),
                },
            ],
        };
        let mut output = Vec::new();

        write_scene_list_csv(&scene_list, &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();

        assert!(output.starts_with("Timecode List:,00:00:00.300\n"));
        assert!(output.contains(
            "Scene Number,Start Frame,Start Timecode,Start Time (seconds),End Frame,End Timecode,End Time (seconds),Length (frames),Length (timecode),Length (seconds)"
        ));
        assert!(output
            .contains("1,1,00:00:00.000,0.000000,3,00:00:00.300,0.300000,3,00:00:00.300,0.300000"));
        assert!(output
            .contains("2,4,00:00:00.300,0.300000,5,00:00:00.500,0.500000,2,00:00:00.200,0.200000"));
    }

    #[test]
    fn stats_csv_contains_detector_metric_columns() {
        let frames = frames(&[[0, 0, 0], [3, 3, 3], [255, 255, 255], [252, 252, 252]]);
        let result = detect_frames(
            DetectorConfig::Adaptive(AdaptiveDetectorConfig {
                threshold: 3.0,
                min_content_val: 20.0,
                frame_window: 1,
                ..Default::default()
            }),
            FrameRate(10.0),
            &frames,
            DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
        )
        .unwrap();
        let mut output = Vec::new();

        write_stats_csv(&result.stats, &mut output).unwrap();
        let output = String::from_utf8(output).unwrap();
        let lines: Vec<_> = output.lines().collect();

        assert_eq!(lines[0], "Frame Number,adaptive_ratio,content_val");
        assert_eq!(lines.len(), frames.len() + 1);
        assert!(lines[1].starts_with("0,0.000000,0.000000"));
    }

    #[test]
    fn json_scene_list_and_ndjson_events_use_export_fields() {
        let scene_list = SceneList {
            frame_rate: FrameRate(10.0),
            scenes: vec![
                SceneSpan {
                    start: FrameIndex(0),
                    end: FrameIndex(3),
                },
                SceneSpan {
                    start: FrameIndex(3),
                    end: FrameIndex(5),
                },
            ],
        };
        let mut json = Vec::new();
        let mut ndjson = Vec::new();

        write_scene_list_json(&scene_list, &mut json).unwrap();
        write_scene_events_ndjson(&scene_list, &mut ndjson).unwrap();

        let json: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(json["frame_rate"], 10.0);
        assert_eq!(json["scene_count"], 2);
        assert_eq!(json["scenes"][0]["scene_number"], 1);
        assert_eq!(json["scenes"][0]["start_frame"], 1);
        assert_eq!(json["scenes"][0]["start_timecode"], "00:00:00.000");
        assert_eq!(json["scenes"][0]["end_frame"], 3);
        assert_eq!(json["scenes"][0]["end_timecode"], "00:00:00.300");
        assert_eq!(json["scenes"][0]["length_frames"], 3);

        let ndjson = String::from_utf8(ndjson).unwrap();
        let events: Vec<serde_json::Value> = ndjson
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0]["event"], "scene");
        assert_eq!(events[0]["scene_number"], 1);
        assert_eq!(events[1]["start_frame"], 4);
        assert_eq!(events[1]["length_seconds"], 0.2);
    }

    #[test]
    fn html_scene_list_output_contains_scene_spans_and_timecodes() {
        let scene_list = SceneList {
            frame_rate: FrameRate(10.0),
            scenes: vec![
                SceneSpan {
                    start: FrameIndex(0),
                    end: FrameIndex(3),
                },
                SceneSpan {
                    start: FrameIndex(3),
                    end: FrameIndex(5),
                },
            ],
        };
        let mut html = Vec::new();

        write_scene_list_html(&scene_list, &mut html).unwrap();
        let html = String::from_utf8(html).unwrap();

        assert!(html.contains("<title>Scene List</title>"));
        assert!(html.contains("<h1>Scene List</h1>"));
        assert!(html.contains("<p>Frame rate: 10.000000</p>"));
        assert!(html.contains("<p>Scene count: 2</p>"));
        assert!(html.contains("<th>Start Timecode</th>"));
        assert!(html.contains("<td>1</td>"));
        assert!(html.contains("<td>00:00:00.000</td>"));
        assert!(html.contains("<td>00:00:00.300</td>"));
        assert!(html.contains("<td>00:00:00.500</td>"));
    }

    #[test]
    fn boundary_review_writers_use_ranked_candidate_export_fields() {
        let review = BoundaryReview {
            frame_rate: FrameRate(10.0),
            detector: "content".to_owned(),
            score_metric: "content_val".to_owned(),
            detector_threshold: 20.0,
            review_threshold: 16.0,
            scene_list: SceneList {
                frame_rate: FrameRate(10.0),
                scenes: vec![
                    SceneSpan {
                        start: FrameIndex(0),
                        end: FrameIndex(3),
                    },
                    SceneSpan {
                        start: FrameIndex(3),
                        end: FrameIndex(5),
                    },
                ],
            },
            candidates: vec![BoundaryCandidateReview {
                candidate_number: 2,
                status: BoundaryCandidateStatus::NearMiss,
                frame: FrameIndex(3),
                score_metric: "content_val".to_owned(),
                score: 18.5,
                detector_threshold: 20.0,
                review_threshold: 16.0,
                threshold_distance: 1.5,
                metrics: BTreeMap::from([("content_val".to_owned(), 18.5)]),
                before: ReviewSceneContext {
                    start: FrameIndex(0),
                    end: FrameIndex(3),
                },
                after: ReviewSceneContext {
                    start: FrameIndex(3),
                    end: FrameIndex(5),
                },
            }],
        };
        let mut csv = Vec::new();
        let mut json = Vec::new();

        write_boundary_review_csv(&review, &mut csv).unwrap();
        write_boundary_review_json(&review, &mut json).unwrap();

        let csv = String::from_utf8(csv).unwrap();
        assert!(csv.contains("Rank,Status,Boundary Candidate Number"));
        assert!(csv.contains(
            "1,near_miss,2,4,3,00:00:00.300,0.300000,content_val,18.500000,20.000000,16.000000,1.500000,1,3,4,5,18.500000"
        ));

        let json: serde_json::Value = serde_json::from_slice(&json).unwrap();
        assert_eq!(json["frame_rate"], 10.0);
        assert_eq!(json["detector"], "content");
        assert_eq!(json["sort"], "threshold_distance");
        assert_eq!(json["candidate_count"], 1);
        assert_eq!(json["boundary_candidates"][0]["rank"], 1);
        assert_eq!(json["boundary_candidates"][0]["status"], "near_miss");
        assert_eq!(
            json["boundary_candidates"][0]["boundary_timecode"],
            "00:00:00.300"
        );
        assert_eq!(
            json["boundary_candidates"][0]["metrics"]["content_val"],
            18.5
        );
    }
}
