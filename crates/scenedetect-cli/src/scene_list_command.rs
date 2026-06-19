use std::fs::{self, File};
use std::path::PathBuf;

use anyhow::{Context, Result};
use scenedetect_core::{
    detect_scenes_streaming, write_scene_list_html, CsvStatsSink, DetectionOptions, DetectorConfig,
    FrameRate, NoopStatsSink, SceneList,
};
use scenedetect_ffmpeg::FfmpegFrameSource;

use crate::artifacts;

pub(crate) struct ExportHtmlStdoutRequest {
    pub(crate) input: PathBuf,
    pub(crate) detector: DetectorConfig,
    pub(crate) options: DetectionOptions,
    pub(crate) scene_list_request: artifacts::SceneListRequest,
    pub(crate) scene_list_artifact: Option<PathBuf>,
    pub(crate) stats: Option<PathBuf>,
    pub(crate) force: bool,
    pub(crate) quiet: bool,
    pub(crate) frame_rate_override: Option<FrameRate>,
}

pub(crate) fn run_export_html_stdout(request: ExportHtmlStdoutRequest) -> Result<()> {
    let scene_list = get_or_create_scene_list(&request)?;
    if !request.quiet {
        write_scene_list_html(&scene_list, std::io::stdout())?;
    }
    Ok(())
}

fn get_or_create_scene_list(request: &ExportHtmlStdoutRequest) -> Result<SceneList> {
    if can_reuse_scene_list(request) {
        if let Some(path) = request.scene_list_artifact.as_deref() {
            if let Some(scene_list) =
                artifacts::read_scene_list_artifact(path, &request.scene_list_request)?
            {
                if !request.quiet {
                    eprintln!("reusing Scene List Artifact: {}", path.display());
                }
                return Ok(scene_list);
            }
        }
    }

    let source = FfmpegFrameSource::open(&request.input, request.frame_rate_override)
        .with_context(|| format!("failed to open input video {}", request.input.display()))?;
    let scene_list = if let Some(stats_path) = request.stats.as_ref() {
        if let Some(parent) = stats_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::create(stats_path)
            .with_context(|| format!("failed to create stats file {}", stats_path.display()))?;
        let mut stats_sink = CsvStatsSink::new(file);
        detect_scenes_streaming(
            request.detector.clone(),
            source,
            request.options.clone(),
            &mut stats_sink,
        )?
    } else {
        let mut stats_sink = NoopStatsSink;
        detect_scenes_streaming(
            request.detector.clone(),
            source,
            request.options.clone(),
            &mut stats_sink,
        )?
    };

    if let Some(path) = request.scene_list_artifact.as_deref() {
        artifacts::write_scene_list_artifact(path, &request.scene_list_request, &scene_list)?;
    }

    Ok(scene_list)
}

fn can_reuse_scene_list(request: &ExportHtmlStdoutRequest) -> bool {
    !request.force && request.stats.is_none()
}
