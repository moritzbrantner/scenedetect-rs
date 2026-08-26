use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{anyhow, Context, Result};
use scenedetect_core::{
    AdaptiveDetectorConfig, ContentDetectionStats, ContentDetectorConfig, DetectionOptions,
    DetectorConfig, FrameRate, HashDetectorConfig, HistogramDetectorConfig,
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
    pub decision: scenedetect_core::DetectionStatsDecision,
    pub metrics: std::collections::BTreeMap<String, f64>,
}

impl DetectionStatsDocument {
    pub fn from_content_stats(
        input: &Path,
        metadata: &VideoMetadata,
        stats: ContentDetectionStats,
    ) -> Result<Self> {
        let file_metadata = fs::metadata(input)
            .with_context(|| format!("failed to read input video metadata {}", input.display()))?;
        let path = input
            .canonicalize()
            .with_context(|| format!("failed to canonicalize input video {}", input.display()))?;
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
            input: DetectionStatsInput {
                path: path.display().to_string(),
                byte_len: file_metadata.len(),
                modified_unix_nanos: modified_unix_nanos(&file_metadata)?,
                width: metadata.width,
                height: metadata.height,
                frame_rate: stats.frame_rate.0,
                total_frames: stats.total_frames,
            },
            detector: DetectionStatsDetector::Content(stats.detector_config),
            options: stats.options,
            metric_names: stats.metric_names,
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
                    frame: scenedetect_core::FrameIndex(row.frame),
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
    let path = detection_stats_path_for_input(input)?;
    let file = File::open(&path).with_context(|| {
        format!(
            "Detection Stats are missing for {}; run `scenedetect-rs detect content -i {}` first",
            input.display(),
            input.display()
        )
    })?;
    let document: DetectionStatsDocument = serde_json::from_reader(file)
        .with_context(|| format!("failed to parse Detection Stats {}", path.display()))?;
    document.validate()?;
    Ok(document)
}

pub fn read_detection_stats_for_input(input: &Path) -> Result<ContentDetectionStats> {
    read_detection_stats_document_for_input(input)?.into_content_stats()
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
}
