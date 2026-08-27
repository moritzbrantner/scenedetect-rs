use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{anyhow, Context, Result};
use scenedetect_core::{
    AdaptiveDetectorConfig, ContentDetectionStats, ContentDetectorConfig, DetectionOptions,
    DetectionResult, DetectionStats, DetectionStatsDecision, DetectorConfig, FrameIndex, FrameRate,
    HashDetectorConfig, HistogramDetectorConfig, SceneList, SceneSpan, StatsRow,
    ThresholdDetectorConfig, Timecode,
};
use scenedetect_ffmpeg::VideoMetadata;
use serde::{Deserialize, Serialize};

const CONTENT_DETECTION_STATS_SCHEMA_VERSION: u32 = 1;
const DETECTION_STATS_SCHEMA_VERSION: u32 = 2;
const MIN_SUPPORTED_DETECTION_STATS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionStatsDocument {
    pub schema_version: u32,
    pub kind: String,
    pub input: DetectionStatsInput,
    pub detector: DetectionStatsDetector,
    pub options: DetectionOptions,
    pub metric_names: Vec<String>,
    pub rows: Vec<DetectionStatsRowDocument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionStatsInput {
    pub path: String,
    pub byte_len: u64,
    pub modified_unix_nanos: u64,
    pub width: u32,
    pub height: u32,
    pub frame_rate: f64,
    pub total_frames: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "name", content = "config", rename_all = "snake_case")]
pub enum DetectionStatsDetector {
    Content(ContentDetectorConfig),
    Adaptive(AdaptiveDetectorConfig),
    Threshold(ThresholdDetectorConfig),
    #[serde(rename = "hist")]
    Histogram(HistogramDetectorConfig),
    Hash(HashDetectorConfig),
}

impl DetectionStatsDetector {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Content(_) => "content",
            Self::Adaptive(_) => "adaptive",
            Self::Threshold(_) => "threshold",
            Self::Histogram(_) => "hist",
            Self::Hash(_) => "hash",
        }
    }
}

impl From<DetectorConfig> for DetectionStatsDetector {
    fn from(config: DetectorConfig) -> Self {
        match config {
            DetectorConfig::Content(config) => Self::Content(config),
            DetectorConfig::Adaptive(config) => Self::Adaptive(config),
            DetectorConfig::Threshold(config) => Self::Threshold(config),
            DetectorConfig::Histogram(config) => Self::Histogram(config),
            DetectorConfig::Hash(config) => Self::Hash(config),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionStatsRowDocument {
    pub frame: u64,
    pub timecode: String,
    pub score: f64,
    pub threshold: f64,
    pub decision: DetectionStatsDecision,
    pub metrics: BTreeMap<String, f64>,
}

impl DetectionStatsDocument {
    pub fn from_content_stats(
        input: &Path,
        metadata: &VideoMetadata,
        stats: ContentDetectionStats,
    ) -> Result<Self> {
        let input = detection_stats_input(input, metadata, stats.total_frames)?;
        let rows = stats
            .rows
            .into_iter()
            .map(|row| DetectionStatsRowDocument {
                frame: row.frame.0,
                timecode: Timecode::from_frames(row.frame.0).display_at_rate(stats.frame_rate),
                score: row.score,
                threshold: row.threshold,
                decision: row.decision,
                metrics: row.metrics,
            })
            .collect();

        Ok(Self {
            schema_version: CONTENT_DETECTION_STATS_SCHEMA_VERSION,
            kind: "detection_stats".to_owned(),
            input,
            detector: DetectionStatsDetector::Content(stats.detector_config),
            options: stats.options,
            metric_names: stats.metric_names,
            rows,
        })
    }

    pub fn from_detection_result(
        input: &Path,
        metadata: &VideoMetadata,
        detector: DetectorConfig,
        options: DetectionOptions,
        result: DetectionResult,
    ) -> Result<Self> {
        let frame_rate = result.scene_list.frame_rate;
        let total_frames = result
            .scene_list
            .scenes
            .last()
            .map(|scene| scene.end.0)
            .unwrap_or(result.stats.rows.len() as u64);
        let accepted_frames = result
            .scene_list
            .scenes
            .iter()
            .skip(1)
            .map(|scene| scene.start.0)
            .collect::<BTreeSet<_>>();
        let rows = result
            .stats
            .rows
            .into_iter()
            .map(|row| {
                let (score, threshold, candidate) = detector_row_semantics(&detector, &row.metrics);
                let decision = if row.frame.0 == 0 {
                    DetectionStatsDecision::NotEvaluated
                } else if accepted_frames.contains(&row.frame.0) {
                    DetectionStatsDecision::Accepted
                } else if candidate {
                    DetectionStatsDecision::SuppressedMinSceneLen
                } else {
                    DetectionStatsDecision::BelowThreshold
                };
                DetectionStatsRowDocument {
                    frame: row.frame.0,
                    timecode: Timecode::from_frames(row.frame.0).display_at_rate(frame_rate),
                    score,
                    threshold,
                    decision,
                    metrics: row.metrics,
                }
            })
            .collect();

        Ok(Self {
            schema_version: DETECTION_STATS_SCHEMA_VERSION,
            kind: "detection_stats".to_owned(),
            input: detection_stats_input(input, metadata, total_frames)?,
            detector: detector.into(),
            options,
            metric_names: result.stats.metric_names,
            rows,
        })
    }

    pub fn validate(&self) -> Result<()> {
        if !(MIN_SUPPORTED_DETECTION_STATS_SCHEMA_VERSION..=DETECTION_STATS_SCHEMA_VERSION)
            .contains(&self.schema_version)
            || self.kind != "detection_stats"
        {
            return Err(anyhow!("invalid Detection Stats document"));
        }
        Ok(())
    }

    pub fn scene_list(&self) -> Result<SceneList> {
        self.validate()?;
        if self.input.total_frames == 0 {
            return Ok(SceneList {
                frame_rate: FrameRate(self.input.frame_rate),
                scenes: Vec::new(),
            });
        }

        let mut starts = vec![0];
        starts.extend(
            self.rows
                .iter()
                .filter(|row| row.decision == DetectionStatsDecision::Accepted)
                .map(|row| row.frame)
                .filter(|frame| *frame > 0 && *frame < self.input.total_frames),
        );
        starts.sort_unstable();
        starts.dedup();

        let mut scenes = starts
            .windows(2)
            .map(|pair| SceneSpan {
                start: FrameIndex(pair[0]),
                end: FrameIndex(pair[1]),
            })
            .collect::<Vec<_>>();
        scenes.push(SceneSpan {
            start: FrameIndex(*starts.last().unwrap_or(&0)),
            end: FrameIndex(self.input.total_frames),
        });
        Ok(SceneList {
            frame_rate: FrameRate(self.input.frame_rate),
            scenes,
        })
    }

    pub fn detection_stats(&self) -> Result<DetectionStats> {
        self.validate()?;
        Ok(DetectionStats {
            metric_names: self.metric_names.clone(),
            rows: self
                .rows
                .iter()
                .map(|row| StatsRow {
                    frame: FrameIndex(row.frame),
                    metrics: row.metrics.clone(),
                })
                .collect(),
        })
    }

    pub fn into_content_stats(self) -> Result<ContentDetectionStats> {
        self.validate()?;
        let detector_config = match self.detector {
            DetectionStatsDetector::Content(config) => config,
            detector => {
                return Err(anyhow!(
                    "native content rendering cannot read {} Detection Stats",
                    detector.name()
                ))
            }
        };

        Ok(ContentDetectionStats {
            frame_rate: FrameRate(self.input.frame_rate),
            total_frames: self.input.total_frames,
            detector_config,
            options: self.options,
            metric_names: self.metric_names,
            rows: self
                .rows
                .into_iter()
                .map(|row| scenedetect_core::RichDetectionStatsRow {
                    frame: FrameIndex(row.frame),
                    score: row.score,
                    threshold: row.threshold,
                    decision: row.decision,
                    metrics: row.metrics,
                })
                .collect(),
        })
    }
}

pub fn detection_stats_path_for_input(input: &Path) -> Result<PathBuf> {
    stem_path(input, "scenedetect.json")
}

pub fn render_output_path_for_input(input: &Path, suffix: &str) -> Result<PathBuf> {
    stem_path(input, suffix)
}

pub fn write_detection_stats(path: &Path, document: &DetectionStatsDocument) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }
    let file = File::create(path)
        .with_context(|| format!("failed to create Detection Stats {}", path.display()))?;
    serde_json::to_writer_pretty(file, document)?;
    Ok(())
}

pub fn read_detection_stats_document_for_input(input: &Path) -> Result<DetectionStatsDocument> {
    Ok(read_detection_stats_document(input)?.1)
}

pub fn read_detection_stats_document(
    input_or_stats: &Path,
) -> Result<(PathBuf, DetectionStatsDocument)> {
    let path = detection_stats_path_for_input_or_stats(input_or_stats)?;
    let default_recovery = default_recovery_command(input_or_stats);
    let file = File::open(&path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            anyhow!(
                "Detection Stats are missing: artifact {}. Recovery: `{}`",
                path.display(),
                default_recovery
            )
        } else {
            anyhow!(
                "failed to read Detection Stats artifact {}: {}. Recovery: `{}`",
                path.display(),
                error,
                default_recovery
            )
        }
    })?;
    let document: DetectionStatsDocument = serde_json::from_reader(file).map_err(|error| {
        anyhow!(
            "failed to parse Detection Stats artifact {} (malformed): {}. Recovery: `{}`",
            path.display(),
            error,
            default_recovery
        )
    })?;
    document.validate().map_err(|error| {
        anyhow!(
            "Detection Stats artifact {} is invalid: {}. Recovery: `{}`",
            path.display(),
            error,
            recovery_command_for_document(&document)
        )
    })?;

    let source_path = if is_detection_stats_path(input_or_stats) {
        PathBuf::from(&document.input.path)
    } else {
        input_or_stats.to_path_buf()
    };
    validate_input_fingerprint(&source_path, &path, &document)?;
    Ok((path, document))
}

pub fn detection_stats_path_for_input_or_stats(input_or_stats: &Path) -> Result<PathBuf> {
    if is_detection_stats_path(input_or_stats) {
        Ok(input_or_stats.to_path_buf())
    } else {
        detection_stats_path_for_input(input_or_stats)
    }
}

fn validate_input_fingerprint(
    input: &Path,
    stats_path: &Path,
    document: &DetectionStatsDocument,
) -> Result<()> {
    let metadata = match fs::metadata(input) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to read input metadata {}", input.display()))
        }
    };
    let canonical = input
        .canonicalize()
        .with_context(|| format!("failed to canonicalize input {}", input.display()))?;
    let modified = modified_unix_nanos(&metadata)?;
    let changed = canonical.display().to_string() != document.input.path
        || metadata.len() != document.input.byte_len
        || modified != document.input.modified_unix_nanos;
    if changed {
        return Err(anyhow!(
            "Detection Stats artifact {} is stale for changed input {}. Recovery: `{}`",
            stats_path.display(),
            input.display(),
            recovery_command_for_document(document)
        ));
    }
    Ok(())
}

fn is_detection_stats_path(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".scenedetect.json"))
}

fn default_recovery_command(input_or_stats: &Path) -> String {
    if is_detection_stats_path(input_or_stats) {
        "scenedetect-rs detect content -i <video>".to_owned()
    } else {
        format!(
            "scenedetect-rs detect content -i {}",
            shell_quote(input_or_stats)
        )
    }
}

fn recovery_command_for_document(document: &DetectionStatsDocument) -> String {
    let mut command = format!(
        "scenedetect-rs detect {} -i {} --min-scene-len {}",
        document.detector.name(),
        shell_quote(Path::new(&document.input.path)),
        document.options.min_scene_len
    );
    match &document.detector {
        DetectionStatsDetector::Content(config) => {
            command.push_str(&format!(" --threshold {}", config.threshold));
            command.push_str(&format!(
                " --weights {} {} {} {}",
                config.weights.hue,
                config.weights.saturation,
                config.weights.luminance,
                config.weights.edges
            ));
            if config.luma_only {
                command.push_str(" --luma-only");
            }
        }
        DetectionStatsDetector::Adaptive(config) => {
            command.push_str(&format!(
                " --threshold {} --min-content-val {} --frame-window {}",
                config.threshold, config.min_content_val, config.frame_window
            ));
            command.push_str(&format!(
                " --weights {} {} {} {}",
                config.weights.hue,
                config.weights.saturation,
                config.weights.luminance,
                config.weights.edges
            ));
            if config.luma_only {
                command.push_str(" --luma-only");
            }
        }
        DetectionStatsDetector::Threshold(config) => {
            command.push_str(&format!(
                " --threshold {} --fade-bias {}",
                config.threshold, config.fade_bias
            ));
        }
        DetectionStatsDetector::Histogram(config) => {
            command.push_str(&format!(
                " --threshold {} --bins {}",
                config.threshold, config.bins
            ));
        }
        DetectionStatsDetector::Hash(config) => {
            command.push_str(&format!(
                " --threshold {} --size {} --lowpass {}",
                config.threshold, config.size, config.lowpass
            ));
        }
    }
    command.push_str(" --force");
    command
}

fn shell_quote(path: &Path) -> String {
    let value = path.display().to_string();
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn detection_stats_input(
    input: &Path,
    metadata: &VideoMetadata,
    total_frames: u64,
) -> Result<DetectionStatsInput> {
    let file_metadata = fs::metadata(input)
        .with_context(|| format!("failed to read input video metadata {}", input.display()))?;
    let path = input
        .canonicalize()
        .with_context(|| format!("failed to canonicalize input video {}", input.display()))?;
    Ok(DetectionStatsInput {
        path: path.display().to_string(),
        byte_len: file_metadata.len(),
        modified_unix_nanos: modified_unix_nanos(&file_metadata)?,
        width: metadata.width,
        height: metadata.height,
        frame_rate: metadata.frame_rate.0,
        total_frames,
    })
}

fn detector_row_semantics(
    detector: &DetectorConfig,
    metrics: &BTreeMap<String, f64>,
) -> (f64, f64, bool) {
    match detector {
        DetectorConfig::Content(config) => {
            let score = metric_value(metrics, "content_val");
            (score, config.threshold, score >= config.threshold)
        }
        DetectorConfig::Adaptive(config) => {
            let score = metric_value(metrics, "adaptive_ratio");
            let content = metric_value(metrics, "content_val");
            (
                score,
                config.threshold,
                content >= config.min_content_val && score >= config.threshold,
            )
        }
        DetectorConfig::Threshold(config) => {
            let score = metric_value(metrics, "average_rgb");
            (score, config.threshold, false)
        }
        DetectorConfig::Histogram(config) => {
            let correlation = metric_value_prefix(metrics, "hist_diff");
            let score = 1.0 - correlation;
            (score, config.threshold, score >= config.threshold)
        }
        DetectorConfig::Hash(config) => {
            let score = metric_value_prefix(metrics, "hash_dist");
            (score, config.threshold, score >= config.threshold)
        }
    }
}

fn metric_value(metrics: &BTreeMap<String, f64>, name: &str) -> f64 {
    metrics.get(name).copied().unwrap_or(0.0)
}

fn metric_value_prefix(metrics: &BTreeMap<String, f64>, prefix: &str) -> f64 {
    metrics
        .iter()
        .find(|(name, _)| name.starts_with(prefix))
        .map(|(_, value)| *value)
        .unwrap_or(0.0)
}

fn stem_path(input: &Path, suffix: &str) -> Result<PathBuf> {
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let stem = input
        .file_stem()
        .ok_or_else(|| anyhow!("input path has no file stem: {}", input.display()))?
        .to_string_lossy();
    Ok(parent.join(format!("{stem}.{suffix}")))
}

fn modified_unix_nanos(metadata: &fs::Metadata) -> Result<u64> {
    let nanos = metadata
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_err(|error| anyhow!("modified time predates unix epoch: {error}"))?
        .as_nanos();
    u64::try_from(nanos).map_err(|_| anyhow!("modified time does not fit in u64 nanoseconds"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_v1_content_document_remains_readable() {
        let document: DetectionStatsDocument = serde_json::from_str(
            r#"{
              "schema_version": 1,
              "kind": "detection_stats",
              "input": {
                "path": "/tmp/example.mp4",
                "byte_len": 1,
                "modified_unix_nanos": 2,
                "width": 16,
                "height": 16,
                "frame_rate": 10.0,
                "total_frames": 2
              },
              "detector": {
                "name": "content",
                "config": {
                  "threshold": 20.0,
                  "weights": {
                    "hue": 1.0,
                    "saturation": 1.0,
                    "luminance": 1.0,
                    "edges": 0.0
                  },
                  "luma_only": false
                }
              },
              "options": {
                "min_scene_len": 1,
                "min_scene_len_policy": "Suppress"
              },
              "metric_names": ["content_val"],
              "rows": [
                {
                  "frame": 0,
                  "timecode": "00:00:00.000",
                  "score": 0.0,
                  "threshold": 20.0,
                  "decision": "not_evaluated",
                  "metrics": {"content_val": 0.0}
                }
              ]
            }"#,
        )
        .unwrap();

        let stats = document.into_content_stats().unwrap();
        assert_eq!(stats.detector_config.threshold, 20.0);
        assert_eq!(stats.total_frames, 2);
        assert_eq!(stats.rows.len(), 1);
    }

    #[test]
    fn generalized_detector_shape_round_trips_all_canonical_configs() {
        let configs = [
            DetectionStatsDetector::Content(ContentDetectorConfig::default()),
            DetectionStatsDetector::Adaptive(AdaptiveDetectorConfig::default()),
            DetectionStatsDetector::Threshold(ThresholdDetectorConfig::default()),
            DetectionStatsDetector::Histogram(HistogramDetectorConfig::default()),
            DetectionStatsDetector::Hash(HashDetectorConfig::default()),
        ];

        for config in configs {
            let value = serde_json::to_value(&config).unwrap();
            assert_eq!(value["name"], config.name());
            let restored: DetectionStatsDetector = serde_json::from_value(value).unwrap();
            assert_eq!(restored, config);
        }
    }

    #[test]
    fn generalized_rows_reconstruct_scene_list_and_stats() {
        let document = DetectionStatsDocument {
            schema_version: 2,
            kind: "detection_stats".to_owned(),
            input: DetectionStatsInput {
                path: "/tmp/example.mp4".to_owned(),
                byte_len: 1,
                modified_unix_nanos: 2,
                width: 16,
                height: 16,
                frame_rate: 10.0,
                total_frames: 4,
            },
            detector: DetectionStatsDetector::Adaptive(AdaptiveDetectorConfig::default()),
            options: DetectionOptions {
                min_scene_len: 1,
                ..Default::default()
            },
            metric_names: vec!["adaptive_ratio".to_owned(), "content_val".to_owned()],
            rows: vec![
                DetectionStatsRowDocument {
                    frame: 0,
                    timecode: "00:00:00.000".to_owned(),
                    score: 0.0,
                    threshold: 3.0,
                    decision: DetectionStatsDecision::NotEvaluated,
                    metrics: BTreeMap::new(),
                },
                DetectionStatsRowDocument {
                    frame: 2,
                    timecode: "00:00:00.200".to_owned(),
                    score: 4.0,
                    threshold: 3.0,
                    decision: DetectionStatsDecision::Accepted,
                    metrics: BTreeMap::from([("adaptive_ratio".to_owned(), 4.0)]),
                },
            ],
        };

        let scene_list = document.scene_list().unwrap();
        assert_eq!(
            scene_list.scenes,
            vec![
                SceneSpan {
                    start: FrameIndex(0),
                    end: FrameIndex(2),
                },
                SceneSpan {
                    start: FrameIndex(2),
                    end: FrameIndex(4),
                },
            ]
        );
        let stats = document.detection_stats().unwrap();
        assert_eq!(stats.metric_names.len(), 2);
        assert_eq!(stats.rows.len(), 2);
    }
}
