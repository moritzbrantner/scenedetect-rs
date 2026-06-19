use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use scenedetect_core::{
    detect_scenes_streaming, write_scene_list_html, CsvStatsSink, DetectionOptions, DetectorConfig,
    FrameRate, NoopStatsSink, SceneList,
};
use scenedetect_ffmpeg::FfmpegFrameSource;

use crate::artifacts;

const SCENE_LIST_HTML_RENDER_KIND: &str = "scene_list_html";

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

pub(crate) struct ExportHtmlFileRequest {
    pub(crate) input: PathBuf,
    pub(crate) detector: DetectorConfig,
    pub(crate) options: DetectionOptions,
    pub(crate) scene_list_request: artifacts::SceneListRequest,
    pub(crate) scene_list_artifact: Option<PathBuf>,
    pub(crate) output: Option<PathBuf>,
    pub(crate) filename: Option<String>,
    pub(crate) stats: Option<PathBuf>,
    pub(crate) force: bool,
    pub(crate) quiet: bool,
    pub(crate) frame_rate_override: Option<FrameRate>,
}

struct ExportHtmlFileOutput {
    output_dir: PathBuf,
    output_path: PathBuf,
}

pub(crate) fn run_export_html_stdout(request: ExportHtmlStdoutRequest) -> Result<()> {
    let scene_list = get_or_create_scene_list(SceneListAcquisitionRequest {
        input: &request.input,
        detector: request.detector,
        options: request.options,
        scene_list_request: &request.scene_list_request,
        scene_list_artifact: request.scene_list_artifact.as_deref(),
        stats: request.stats.as_deref(),
        force: request.force,
        quiet: request.quiet,
        frame_rate_override: request.frame_rate_override,
    })?;
    if !request.quiet {
        write_scene_list_html(&scene_list, std::io::stdout())?;
    }
    Ok(())
}

fn prepare_export_html_file_output(
    request: &ExportHtmlFileRequest,
) -> Result<ExportHtmlFileOutput> {
    let output_dir = request.output.clone().unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&output_dir)?;
    let output_path = output_dir.join(export_html_filename(request));
    Ok(ExportHtmlFileOutput {
        output_dir,
        output_path,
    })
}

pub(crate) fn run_export_html_file(request: ExportHtmlFileRequest) -> Result<()> {
    let request_key = artifacts::request_key(&request.scene_list_request)?;
    let output = prepare_export_html_file_output(&request)?;
    if reusable_export_html_output_exists(&request, &output, &request_key)? {
        if !request.quiet {
            eprintln!(
                "reusing Scene List output: {}",
                output.output_path.display()
            );
            println!("{}", output.output_path.display());
        }
        return Ok(());
    }

    run_export_html_file_first_write(request, output, &request_key)
}

fn run_export_html_file_first_write(
    request: ExportHtmlFileRequest,
    output: ExportHtmlFileOutput,
    request_key: &str,
) -> Result<()> {
    let scene_list_artifact = scene_list_artifact_path(&request, &output.output_dir, request_key);
    let scene_list = get_or_create_scene_list(SceneListAcquisitionRequest {
        input: &request.input,
        detector: request.detector,
        options: request.options,
        scene_list_request: &request.scene_list_request,
        scene_list_artifact: Some(scene_list_artifact.as_ref()),
        stats: request.stats.as_deref(),
        force: request.force,
        quiet: request.quiet,
        frame_rate_override: request.frame_rate_override,
    })?;
    let file = File::create(&output.output_path).with_context(|| {
        format!(
            "failed to create HTML scene list {}",
            output.output_path.display()
        )
    })?;
    write_scene_list_html(&scene_list, file)?;

    let manifest_path = artifacts::render_manifest_path(
        &output.output_dir,
        &output.output_path,
        SCENE_LIST_HTML_RENDER_KIND,
    )?;
    artifacts::write_render_manifest(
        &manifest_path,
        &output.output_path,
        SCENE_LIST_HTML_RENDER_KIND,
        request_key,
        &request.scene_list_request,
    )?;
    if !request.quiet {
        println!("{}", output.output_path.display());
    }
    Ok(())
}

fn reusable_export_html_output_exists(
    request: &ExportHtmlFileRequest,
    output: &ExportHtmlFileOutput,
    request_key: &str,
) -> Result<bool> {
    if !can_reuse_export_html_output(request) || !explicit_artifact_matches(request)? {
        return Ok(false);
    }

    let manifest_path = artifacts::render_manifest_path(
        &output.output_dir,
        &output.output_path,
        SCENE_LIST_HTML_RENDER_KIND,
    )?;
    artifacts::reusable_output_exists(
        &manifest_path,
        &output.output_path,
        SCENE_LIST_HTML_RENDER_KIND,
        request_key,
        &request.scene_list_request,
    )
}

fn can_reuse_export_html_output(request: &ExportHtmlFileRequest) -> bool {
    !request.force && request.stats.is_none()
}

fn explicit_artifact_matches(request: &ExportHtmlFileRequest) -> Result<bool> {
    let Some(path) = request.scene_list_artifact.as_deref() else {
        return Ok(true);
    };
    Ok(artifacts::read_scene_list_artifact(path, &request.scene_list_request)?.is_some())
}

struct SceneListAcquisitionRequest<'a> {
    input: &'a Path,
    detector: DetectorConfig,
    options: DetectionOptions,
    scene_list_request: &'a artifacts::SceneListRequest,
    scene_list_artifact: Option<&'a Path>,
    stats: Option<&'a Path>,
    force: bool,
    quiet: bool,
    frame_rate_override: Option<FrameRate>,
}

fn get_or_create_scene_list(request: SceneListAcquisitionRequest<'_>) -> Result<SceneList> {
    if can_reuse_scene_list(&request) {
        if let Some(path) = request.scene_list_artifact {
            if let Some(scene_list) =
                artifacts::read_scene_list_artifact(path, request.scene_list_request)?
            {
                if !request.quiet {
                    eprintln!("reusing Scene List Artifact: {}", path.display());
                }
                return Ok(scene_list);
            }
        }
    }

    let source = FfmpegFrameSource::open(request.input, request.frame_rate_override)
        .with_context(|| format!("failed to open input video {}", request.input.display()))?;
    let scene_list = if let Some(stats_path) = request.stats {
        if let Some(parent) = stats_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::create(stats_path)
            .with_context(|| format!("failed to create stats file {}", stats_path.display()))?;
        let mut stats_sink = CsvStatsSink::new(file);
        detect_scenes_streaming(request.detector, source, request.options, &mut stats_sink)?
    } else {
        let mut stats_sink = NoopStatsSink;
        detect_scenes_streaming(request.detector, source, request.options, &mut stats_sink)?
    };

    if let Some(path) = request.scene_list_artifact {
        artifacts::write_scene_list_artifact(path, request.scene_list_request, &scene_list)?;
    }

    Ok(scene_list)
}

fn can_reuse_scene_list(request: &SceneListAcquisitionRequest<'_>) -> bool {
    !request.force && request.stats.is_none()
}

fn scene_list_artifact_path(
    request: &ExportHtmlFileRequest,
    output_dir: &Path,
    request_key: &str,
) -> PathBuf {
    request
        .scene_list_artifact
        .clone()
        .unwrap_or_else(|| artifacts::default_scene_list_artifact_path(output_dir, request_key))
}

fn export_html_filename(request: &ExportHtmlFileRequest) -> &str {
    request.filename.as_deref().unwrap_or("scenes.html")
}
