use std::collections::BTreeMap;
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    mut source: S,
    options: DetectionOptions,
) -> Result<DetectionResult>
where
    D: Detector,
    S: FrameSource,
{
    let frame_rate = source.frame_rate();
    let mut frames = Vec::new();
    while let Some(frame) = source.next_frame()? {
        frames.push(frame);
    }
    detect_frames(detector.config(), frame_rate, &frames, options)
}

pub fn detect_frames(
    detector: DetectorConfig,
    frame_rate: FrameRate,
    frames: &[Frame],
    options: DetectionOptions,
) -> Result<DetectionResult> {
    let (boundaries, stats) = match detector {
        DetectorConfig::Content(config) => detect_content(frames, config, options.min_scene_len),
        DetectorConfig::Adaptive(config) => detect_adaptive(frames, config, options.min_scene_len),
        DetectorConfig::Threshold(config) => {
            detect_threshold(frames, config, options.min_scene_len)
        }
    };
    let scene_list = build_scene_list(frame_rate, frames.len() as u64, boundaries, options);
    Ok(DetectionResult { scene_list, stats })
}

fn detect_content(
    frames: &[Frame],
    config: ContentDetectorConfig,
    min_scene_len: u64,
) -> (Vec<SceneBoundary>, DetectionStats) {
    let mut rows = Vec::new();
    let mut boundaries = Vec::new();
    let mut last_candidate_boundary = 0;

    for (idx, frame) in frames.iter().enumerate() {
        let content_val = if idx == 0 {
            0.0
        } else {
            content_score(&frames[idx - 1], frame, &config.weights, config.luma_only)
        };
        let mut metrics = BTreeMap::new();
        metrics.insert("content_val".to_owned(), content_val);
        rows.push(StatsRow {
            frame: frame.index,
            metrics,
        });

        let frame_number = frame.index.0;
        if content_val >= config.threshold {
            if frame_number.saturating_sub(last_candidate_boundary) >= min_scene_len {
                boundaries.push(SceneBoundary { frame: frame.index });
            }
            last_candidate_boundary = frame_number;
        }
    }

    (
        boundaries,
        DetectionStats {
            metric_names: vec!["content_val".to_owned()],
            rows,
        },
    )
}

fn detect_adaptive(
    frames: &[Frame],
    config: AdaptiveDetectorConfig,
    min_scene_len: u64,
) -> (Vec<SceneBoundary>, DetectionStats) {
    let mut content_values = vec![0.0; frames.len()];
    for idx in 1..frames.len() {
        content_values[idx] = content_score(
            &frames[idx - 1],
            &frames[idx],
            &config.weights,
            config.luma_only,
        );
    }

    let mut rows = Vec::new();
    let mut boundaries = Vec::new();
    let mut last_boundary = 0;

    for (idx, frame) in frames.iter().enumerate() {
        let window = config.frame_window;
        let adaptive_ratio = if idx >= window && idx + window < frames.len() {
            let start = idx - window;
            let end = idx + window;
            let mut neighbour_sum = 0.0;
            let mut neighbour_count = 0.0;
            for (neighbour_idx, value) in
                content_values.iter().enumerate().take(end + 1).skip(start)
            {
                if neighbour_idx != idx {
                    neighbour_sum += value;
                    neighbour_count += 1.0;
                }
            }
            content_values[idx] / (neighbour_sum / neighbour_count + 0.000_001)
        } else {
            0.0
        };

        let mut metrics = BTreeMap::new();
        metrics.insert("content_val".to_owned(), content_values[idx]);
        metrics.insert("adaptive_ratio".to_owned(), adaptive_ratio);
        rows.push(StatsRow {
            frame: frame.index,
            metrics,
        });

        let frame_number = frame.index.0;
        if content_values[idx] >= config.min_content_val
            && adaptive_ratio >= config.threshold
            && frame_number.saturating_sub(last_boundary) >= min_scene_len
        {
            boundaries.push(SceneBoundary { frame: frame.index });
            last_boundary = frame_number;
        }
    }

    (
        boundaries,
        DetectionStats {
            metric_names: vec!["content_val".to_owned(), "adaptive_ratio".to_owned()],
            rows,
        },
    )
}

fn detect_threshold(
    frames: &[Frame],
    config: ThresholdDetectorConfig,
    min_scene_len: u64,
) -> (Vec<SceneBoundary>, DetectionStats) {
    let mut rows = Vec::new();
    let mut boundaries = Vec::new();
    let mut last_boundary = 0;

    for (idx, frame) in frames.iter().enumerate() {
        let luma = frame.mean_luma();
        let previous_luma = idx
            .checked_sub(1)
            .map(|previous| frames[previous].mean_luma())
            .unwrap_or(luma);
        let threshold_crossing = previous_luma < config.threshold && luma >= config.threshold;

        let mut metrics = BTreeMap::new();
        metrics.insert("average_rgb".to_owned(), luma);
        rows.push(StatsRow {
            frame: frame.index,
            metrics,
        });

        let frame_number = frame.index.0;
        if threshold_crossing && frame_number.saturating_sub(last_boundary) >= min_scene_len {
            boundaries.push(SceneBoundary { frame: frame.index });
            last_boundary = frame_number;
        }
    }

    (
        boundaries,
        DetectionStats {
            metric_names: vec!["average_rgb".to_owned()],
            rows,
        },
    )
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
            vec!["content_val", "adaptive_ratio"]
        );
    }

    #[test]
    fn threshold_detector_emits_boundary_when_fade_returns_above_threshold() {
        let frames = frames(&[
            [0, 0, 0],
            [0, 0, 0],
            [0, 0, 0],
            [80, 80, 80],
            [120, 120, 120],
        ]);
        let result = detect_frames(
            DetectorConfig::Threshold(ThresholdDetectorConfig {
                threshold: 12.0,
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
        assert_eq!(result.stats.metric_names, vec!["average_rgb"]);
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

        assert_eq!(lines[0], "Frame Number,content_val,adaptive_ratio");
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
