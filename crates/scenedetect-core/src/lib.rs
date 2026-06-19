use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::io::Write;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, SceneDetectError>;

#[derive(Debug, Error)]
pub enum SceneDetectError {
    #[error("invalid timecode: {0}")]
    InvalidTimecode(String),
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectionResult {
    pub scene_list: SceneList,
    pub stats: DetectionStats,
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
    let mut weighted_sum = 0.0;
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
            weighted_sum += (prev_luma - curr_luma).abs();
        } else {
            weighted_sum += (prev[0] as f64 - curr[0] as f64).abs() * weights.hue;
            weighted_sum += (prev[1] as f64 - curr[1] as f64).abs() * weights.saturation;
            weighted_sum += (prev[2] as f64 - curr[2] as f64).abs() * weights.luminance;
        }
    }

    let denominator = if luma_only {
        pixel_count
    } else {
        pixel_count * channel_weight_total
    };

    if denominator == 0.0 {
        0.0
    } else {
        weighted_sum / denominator
    }
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

pub fn write_scene_list_csv<W: Write>(scene_list: &SceneList, writer: W) -> Result<()> {
    let mut csv = csv::WriterBuilder::new().flexible(true).from_writer(writer);
    let mut timecode_list = vec!["Timecode List:".to_owned()];
    timecode_list.extend(
        scene_list.scenes.iter().skip(1).map(|scene| {
            Timecode::from_frames(scene.start.0).display_at_rate(scene_list.frame_rate)
        }),
    );
    csv.write_record(timecode_list)?;
    csv.write_record([
        "Scene Number",
        "Start Frame",
        "Start Timecode",
        "Start Time (seconds)",
        "End Frame",
        "End Timecode",
        "End Time (seconds)",
        "Length (frames)",
        "Length (timecode)",
        "Length (seconds)",
    ])?;

    for (idx, scene) in scene_list.scenes.iter().enumerate() {
        let length = scene.end.0.saturating_sub(scene.start.0);
        csv.write_record([
            (idx + 1).to_string(),
            (scene.start.0 + 1).to_string(),
            Timecode::from_frames(scene.start.0).display_at_rate(scene_list.frame_rate),
            seconds_at_rate(scene.start.0, scene_list.frame_rate),
            scene.end.0.to_string(),
            Timecode::from_frames(scene.end.0).display_at_rate(scene_list.frame_rate),
            seconds_at_rate(scene.end.0, scene_list.frame_rate),
            length.to_string(),
            Timecode::from_frames(length).display_at_rate(scene_list.frame_rate),
            seconds_at_rate(length, scene_list.frame_rate),
        ])?;
    }

    csv.flush()?;
    Ok(())
}

fn seconds_at_rate(frames: u64, frame_rate: FrameRate) -> String {
    format!("{:.6}", seconds_value_at_rate(frames, frame_rate))
}

fn seconds_value_at_rate(frames: u64, frame_rate: FrameRate) -> f64 {
    frames as f64 / frame_rate.0
}

pub fn write_stats_csv<W: Write>(stats: &DetectionStats, writer: W) -> Result<()> {
    let mut csv = csv::Writer::from_writer(writer);
    let mut header = vec!["Frame Number".to_owned()];
    header.extend(stats.metric_names.iter().cloned());
    csv.write_record(header)?;

    for row in &stats.rows {
        let mut record = vec![row.frame.0.to_string()];
        for metric in &stats.metric_names {
            record.push(format!(
                "{:.6}",
                row.metrics.get(metric).copied().unwrap_or(0.0)
            ));
        }
        csv.write_record(record)?;
    }

    csv.flush()?;
    Ok(())
}

pub fn write_scene_list_json<W: Write>(scene_list: &SceneList, writer: W) -> Result<()> {
    let output = SceneListExport {
        frame_rate: scene_list.frame_rate.0,
        scene_count: scene_list.scenes.len(),
        scenes: scene_exports(scene_list),
    };
    serde_json::to_writer_pretty(writer, &output)?;
    Ok(())
}

pub fn write_scene_events_ndjson<W: Write>(scene_list: &SceneList, mut writer: W) -> Result<()> {
    for scene in scene_exports(scene_list) {
        let event = SceneEventExport {
            event: "scene",
            scene,
        };
        serde_json::to_writer(&mut writer, &event)?;
        writeln!(writer)?;
    }
    Ok(())
}

#[derive(Debug, Serialize)]
struct SceneListExport {
    frame_rate: f64,
    scene_count: usize,
    scenes: Vec<SceneExport>,
}

#[derive(Debug, Serialize)]
struct SceneEventExport {
    event: &'static str,
    #[serde(flatten)]
    scene: SceneExport,
}

#[derive(Debug, Serialize)]
struct SceneExport {
    scene_number: usize,
    start_frame: u64,
    start_timecode: String,
    start_seconds: f64,
    end_frame: u64,
    end_timecode: String,
    end_seconds: f64,
    length_frames: u64,
    length_timecode: String,
    length_seconds: f64,
}

fn scene_exports(scene_list: &SceneList) -> Vec<SceneExport> {
    scene_list
        .scenes
        .iter()
        .enumerate()
        .map(|(idx, scene)| {
            let length = scene.end.0.saturating_sub(scene.start.0);
            SceneExport {
                scene_number: idx + 1,
                start_frame: scene.start.0 + 1,
                start_timecode: Timecode::from_frames(scene.start.0)
                    .display_at_rate(scene_list.frame_rate),
                start_seconds: seconds_value_at_rate(scene.start.0, scene_list.frame_rate),
                end_frame: scene.end.0,
                end_timecode: Timecode::from_frames(scene.end.0)
                    .display_at_rate(scene_list.frame_rate),
                end_seconds: seconds_value_at_rate(scene.end.0, scene_list.frame_rate),
                length_frames: length,
                length_timecode: Timecode::from_frames(length)
                    .display_at_rate(scene_list.frame_rate),
                length_seconds: seconds_value_at_rate(length, scene_list.frame_rate),
            }
        })
        .collect()
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
}
