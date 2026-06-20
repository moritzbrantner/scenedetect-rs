use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use anyhow::{anyhow, Context, Result};
use scenedetect_core::{
    ContentDetectionStats, ContentDetectorConfig, DetectionOptions, FrameRate, Timecode,
};
use scenedetect_ffmpeg::VideoMetadata;
use serde::{Deserialize, Serialize};

const DETECTION_STATS_SCHEMA_VERSION: u32 = 1;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectionStatsDetector {
    pub name: String,
    pub config: ContentDetectorConfig,
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
            schema_version: DETECTION_STATS_SCHEMA_VERSION,
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
            detector: DetectionStatsDetector {
                name: "content".to_owned(),
                config: stats.detector_config,
            },
            options: stats.options,
            metric_names: stats.metric_names,
            rows,
        })
    }

    pub fn into_content_stats(self) -> Result<ContentDetectionStats> {
        if self.schema_version != DETECTION_STATS_SCHEMA_VERSION || self.kind != "detection_stats" {
            return Err(anyhow!("invalid Detection Stats document"));
        }
        if self.detector.name != "content" {
            return Err(anyhow!(
                "native render currently supports content Detection Stats only"
            ));
        }

        Ok(ContentDetectionStats {
            frame_rate: FrameRate(self.input.frame_rate),
            total_frames: self.input.total_frames,
            detector_config: self.detector.config,
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

pub fn read_detection_stats_for_input(input: &Path) -> Result<ContentDetectionStats> {
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
    document.into_content_stats()
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
