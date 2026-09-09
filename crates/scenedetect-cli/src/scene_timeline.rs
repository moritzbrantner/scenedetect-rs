use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use scenedetect_core::{scene_timeline_from_source, DetectionOptions, MediaTime};
use scenedetect_ffmpeg::FfmpegFrameSource;
use serde::{Deserialize, Serialize};

use crate::native_stats::{self, DetectionStatsDetector, DetectionStatsInput};

pub const SCENE_TIMELINE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneTimelineDocument {
    pub schema_version: u32,
    pub kind: String,
    pub source_detection_stats_schema_version: u32,
    pub input: DetectionStatsInput,
    pub detector: DetectionStatsDetector,
    pub options: DetectionOptions,
    pub scenes: Vec<SceneTimelineSceneDocument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneTimelineSceneDocument {
    pub scene_number: usize,
    pub start_frame: u64,
    pub end_frame: u64,
    pub start_time: Option<MediaTime>,
    pub end_time: Option<MediaTime>,
}

pub fn render(input_or_stats: &Path, output: Option<&Path>) -> Result<PathBuf> {
    let (_, stats) = native_stats::read_detection_stats_document(input_or_stats)?;
    let source_path = PathBuf::from(&stats.input.path);
    let scene_list = stats.scene_list()?;
    let source = FfmpegFrameSource::open(&source_path, None).with_context(|| {
        format!(
            "failed to open input video {} for exact Scene Timeline timing",
            source_path.display()
        )
    })?;
    let timeline = scene_timeline_from_source(&scene_list, source)?;
    let document = SceneTimelineDocument {
        schema_version: SCENE_TIMELINE_SCHEMA_VERSION,
        kind: "scene_timeline".to_owned(),
        source_detection_stats_schema_version: stats.schema_version,
        input: stats.input,
        detector: stats.detector,
        options: stats.options,
        scenes: timeline
            .scenes
            .into_iter()
            .enumerate()
            .map(|(index, scene)| SceneTimelineSceneDocument {
                scene_number: index + 1,
                start_frame: scene.start.0,
                end_frame: scene.end.0,
                start_time: scene.start_time,
                end_time: scene.end_time,
            })
            .collect(),
    };

    let output_path = match output {
        Some(path) => path.to_path_buf(),
        None => native_stats::render_output_path_for_input(&source_path, "timeline.json")?,
    };
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create Scene Timeline output directory {}",
                parent.display()
            )
        })?;
    }
    let file = File::create(&output_path).with_context(|| {
        format!(
            "failed to create Scene Timeline artifact {}",
            output_path.display()
        )
    })?;
    serde_json::to_writer_pretty(file, &document)?;
    Ok(output_path)
}
