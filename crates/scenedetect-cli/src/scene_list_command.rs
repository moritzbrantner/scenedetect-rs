use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use scenedetect_core::{
    detect_scenes_streaming, write_scene_events_ndjson, write_scene_list_csv,
    write_scene_list_html, write_scene_list_json, CsvStatsSink, DetectionOptions, DetectorConfig,
    FrameRate, NoopStatsSink, SceneList,
};
use scenedetect_ffmpeg::FfmpegFrameSource;

use crate::artifacts;

const SCENE_LIST_HTML_RENDER_KIND: &str = "scene_list_html";

#[derive(Clone, Copy)]
pub(crate) enum SceneListOutputFormat {
    Csv,
    Json,
    Ndjson,
}

pub(crate) struct ListScenesStdoutRequest {
    pub(crate) input: PathBuf,
    pub(crate) detector: DetectorConfig,
    pub(crate) options: DetectionOptions,
    pub(crate) scene_list_request: artifacts::SceneListRequest,
    pub(crate) scene_list_artifact: Option<PathBuf>,
    pub(crate) stats: Option<PathBuf>,
    pub(crate) force: bool,
    pub(crate) quiet: bool,
    pub(crate) frame_rate_override: Option<FrameRate>,
    pub(crate) format: SceneListOutputFormat,
}

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

pub(crate) struct ListScenesFileRequest {
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
    pub(crate) format: SceneListOutputFormat,
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

struct ListScenesFileOutput {
    output_dir: PathBuf,
    output_path: PathBuf,
    render_kind: &'static str,
}

pub(crate) fn run_list_scenes_stdout(request: ListScenesStdoutRequest) -> Result<()> {
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
        write_scene_list(&scene_list, std::io::stdout(), request.format)?;
    }
    Ok(())
}

pub(crate) fn run_list_scenes_file(request: ListScenesFileRequest) -> Result<()> {
    let request_key = artifacts::request_key(&request.scene_list_request)?;
    let output = prepare_list_scenes_file_output(&request)?;
    if reusable_list_scenes_output_exists(&request, &output, &request_key)? {
        if !request.quiet {
            eprintln!(
                "reusing Scene List output: {}",
                output.output_path.display()
            );
            println!("{}", output.output_path.display());
        }
        return Ok(());
    }

    run_list_scenes_file_first_write(request, output, &request_key)
}

fn run_list_scenes_file_first_write(
    request: ListScenesFileRequest,
    output: ListScenesFileOutput,
    request_key: &str,
) -> Result<()> {
    let scene_list_artifact = scene_list_artifact_path(
        request.scene_list_artifact.as_deref(),
        &output.output_dir,
        request_key,
    );
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
            "failed to create scene list {}",
            output.output_path.display()
        )
    })?;
    write_scene_list(&scene_list, file, request.format)?;
    let manifest_path = artifacts::render_manifest_path(
        &output.output_dir,
        &output.output_path,
        output.render_kind,
    )?;
    artifacts::write_render_manifest(
        &manifest_path,
        &output.output_path,
        output.render_kind,
        request_key,
        &request.scene_list_request,
    )?;
    if !request.quiet {
        println!("{}", output.output_path.display());
    }
    Ok(())
}

fn reusable_list_scenes_output_exists(
    request: &ListScenesFileRequest,
    output: &ListScenesFileOutput,
    request_key: &str,
) -> Result<bool> {
    if !can_reuse_list_scenes_output(request)
        || !scene_list_artifact_matches(
            request.scene_list_artifact.as_deref(),
            &request.scene_list_request,
        )?
    {
        return Ok(false);
    }

    let manifest_path = artifacts::render_manifest_path(
        &output.output_dir,
        &output.output_path,
        output.render_kind,
    )?;
    artifacts::reusable_output_exists(
        &manifest_path,
        &output.output_path,
        output.render_kind,
        request_key,
        &request.scene_list_request,
    )
}

fn can_reuse_list_scenes_output(request: &ListScenesFileRequest) -> bool {
    !request.force && request.stats.is_none()
}

fn prepare_list_scenes_file_output(
    request: &ListScenesFileRequest,
) -> Result<ListScenesFileOutput> {
    let output_dir = request.output.clone().unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&output_dir)?;
    let output_path = output_dir.join(list_scenes_filename(request));
    Ok(ListScenesFileOutput {
        output_dir,
        output_path,
        render_kind: list_scenes_render_kind(request.format),
    })
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
    let scene_list_artifact = scene_list_artifact_path(
        request.scene_list_artifact.as_deref(),
        &output.output_dir,
        request_key,
    );
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
    if !can_reuse_export_html_output(request)
        || !scene_list_artifact_matches(
            request.scene_list_artifact.as_deref(),
            &request.scene_list_request,
        )?
    {
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

fn scene_list_artifact_matches(
    scene_list_artifact: Option<&Path>,
    request: &artifacts::SceneListRequest,
) -> Result<bool> {
    let Some(path) = scene_list_artifact else {
        return Ok(true);
    };
    Ok(artifacts::read_scene_list_artifact(path, request)?.is_some())
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

fn write_scene_list<W: std::io::Write>(
    scene_list: &SceneList,
    writer: W,
    format: SceneListOutputFormat,
) -> Result<()> {
    match format {
        SceneListOutputFormat::Csv => write_scene_list_csv(scene_list, writer),
        SceneListOutputFormat::Json => write_scene_list_json(scene_list, writer),
        SceneListOutputFormat::Ndjson => write_scene_events_ndjson(scene_list, writer),
    }?;
    Ok(())
}

fn scene_list_artifact_path(
    scene_list_artifact: Option<&Path>,
    output_dir: &Path,
    request_key: &str,
) -> PathBuf {
    scene_list_artifact
        .map(Path::to_path_buf)
        .unwrap_or_else(|| artifacts::default_scene_list_artifact_path(output_dir, request_key))
}

fn export_html_filename(request: &ExportHtmlFileRequest) -> &str {
    request.filename.as_deref().unwrap_or("scenes.html")
}

fn list_scenes_filename(request: &ListScenesFileRequest) -> &str {
    request.filename.as_deref().unwrap_or(match request.format {
        SceneListOutputFormat::Csv => "scenes.csv",
        SceneListOutputFormat::Json => "scenes.json",
        SceneListOutputFormat::Ndjson => "scenes.ndjson",
    })
}

fn list_scenes_render_kind(format: SceneListOutputFormat) -> &'static str {
    match format {
        SceneListOutputFormat::Csv => "scene_list_csv",
        SceneListOutputFormat::Json => "scene_list_json",
        SceneListOutputFormat::Ndjson => "scene_events_ndjson",
    }
}
