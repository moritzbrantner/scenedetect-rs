mod artifacts;

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use scenedetect_core::{
    detect_boundary_review_streaming, detect_scenes_streaming, write_boundary_review_csv,
    write_boundary_review_json, write_scene_events_ndjson, write_scene_list_csv,
    write_scene_list_html, write_scene_list_json, AdaptiveDetectorConfig, BoundaryReviewOptions,
    ContentDetectorConfig, ContentWeights, CsvStatsSink, DetectionOptions, DetectorConfig,
    FrameRate, HashDetectorConfig, HistogramDetectorConfig, MinSceneLenPolicy, NoopStatsSink,
    SceneList, ThresholdDetectorConfig, Timecode,
};
use scenedetect_ffmpeg::{probe_video, FfmpegFrameSource};

#[derive(Debug, Parser)]
#[command(name = "scenedetect-rs")]
#[command(about = "Rust scene detection CLI with PySceneDetect parity goals.")]
struct Cli {
    #[arg(short = 'i', long = "input")]
    input: PathBuf,
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
    #[arg(short = 's', long = "stats")]
    stats: Option<PathBuf>,
    #[arg(long = "scene-list-artifact")]
    scene_list_artifact: Option<PathBuf>,
    #[arg(long = "force")]
    force: bool,
    #[arg(short = 'f', long = "framerate")]
    framerate: Option<f64>,
    #[arg(short = 'm', long = "min-scene-len", default_value = "15")]
    min_scene_len: String,
    #[arg(long = "drop-short-scenes")]
    drop_short_scenes: bool,
    #[arg(long = "merge-last-scene")]
    merge_last_scene: bool,
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
    #[arg(short = 'v', long = "verbosity", default_value = "warning")]
    verbosity: Verbosity,
    #[command(subcommand)]
    detector: DetectorCommand,
}

#[derive(Debug, Clone, ValueEnum)]
enum Verbosity {
    Debug,
    Info,
    Warning,
    Error,
    None,
}

#[derive(Debug, Subcommand)]
enum DetectorCommand {
    #[command(name = "detect-content")]
    Content(ContentArgs),
    #[command(name = "detect-adaptive")]
    Adaptive(AdaptiveArgs),
    #[command(name = "detect-threshold")]
    Threshold(ThresholdArgs),
    #[command(name = "detect-hist")]
    Histogram(HistogramArgs),
    #[command(name = "detect-hash")]
    Hash(HashArgs),
}

#[derive(Debug, Args)]
struct ContentArgs {
    #[arg(short = 't', long = "threshold", default_value_t = 27.0)]
    threshold: f64,
    #[arg(short = 'w', long = "weights", num_args = 4)]
    weights: Option<Vec<f64>>,
    #[arg(short = 'l', long = "luma-only")]
    luma_only: bool,
    #[arg(short = 'm', long = "min-scene-len")]
    min_scene_len: Option<String>,
    #[arg(short = 'f', long = "filter-mode")]
    filter_mode: Option<String>,
    #[command(subcommand)]
    output: OutputCommand,
}

#[derive(Debug, Args)]
struct AdaptiveArgs {
    #[arg(short = 't', long = "threshold", default_value_t = 3.0)]
    threshold: f64,
    #[arg(short = 'c', long = "min-content-val", default_value_t = 15.0)]
    min_content_val: f64,
    #[arg(short = 'f', long = "frame-window", default_value_t = 2)]
    frame_window: usize,
    #[arg(short = 'w', long = "weights", num_args = 4)]
    weights: Option<Vec<f64>>,
    #[arg(short = 'l', long = "luma-only")]
    luma_only: bool,
    #[arg(short = 'm', long = "min-scene-len")]
    min_scene_len: Option<String>,
    #[command(subcommand)]
    output: OutputCommand,
}

#[derive(Debug, Args)]
struct ThresholdArgs {
    #[arg(short = 't', long = "threshold", default_value_t = 12.0)]
    threshold: f64,
    #[arg(short = 'f', long = "fade-bias", default_value_t = 0.0)]
    fade_bias: f64,
    #[arg(short = 'l', long = "add-last-scene", default_value_t = true)]
    add_last_scene: bool,
    #[arg(short = 'm', long = "min-scene-len")]
    min_scene_len: Option<String>,
    #[command(subcommand)]
    output: OutputCommand,
}

#[derive(Debug, Args)]
struct HistogramArgs {
    #[arg(short = 't', long = "threshold", default_value_t = 0.05, value_parser = parse_unit_interval)]
    threshold: f64,
    #[arg(short = 'b', long = "bins", default_value_t = 256, value_parser = parse_1_to_256)]
    bins: usize,
    #[arg(short = 'm', long = "min-scene-len")]
    min_scene_len: Option<String>,
    #[command(subcommand)]
    output: OutputCommand,
}

#[derive(Debug, Args)]
struct HashArgs {
    #[arg(short = 't', long = "threshold", default_value_t = 0.395, value_parser = parse_unit_interval)]
    threshold: f64,
    #[arg(short = 's', long = "size", default_value_t = 16, value_parser = parse_1_to_256)]
    size: usize,
    #[arg(short = 'l', long = "lowpass", default_value_t = 2, value_parser = parse_1_to_256)]
    lowpass: usize,
    #[arg(short = 'm', long = "min-scene-len")]
    min_scene_len: Option<String>,
    #[command(subcommand)]
    output: OutputCommand,
}

#[derive(Debug, Subcommand)]
enum OutputCommand {
    ListScenes(ListScenesArgs),
    ListBoundaries(ListBoundariesArgs),
    ExportHtml(ExportHtmlArgs),
}

#[derive(Debug, Args)]
struct ListScenesArgs {
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
    #[arg(short = 'f', long = "filename")]
    filename: Option<String>,
    #[arg(long = "format", default_value = "csv")]
    format: SceneListFormat,
    #[arg(short = 'n', long = "no-output-file")]
    no_output_file: bool,
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
    #[arg(short = 's', long = "skip-cuts")]
    skip_cuts: bool,
}

#[derive(Debug, Args)]
struct ListBoundariesArgs {
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
    #[arg(short = 'f', long = "filename")]
    filename: Option<String>,
    #[arg(long = "format", default_value = "csv")]
    format: BoundaryReviewFormat,
    #[arg(long = "review-threshold")]
    review_threshold: Option<f64>,
    #[arg(short = 'n', long = "no-output-file")]
    no_output_file: bool,
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

#[derive(Debug, Args)]
struct ExportHtmlArgs {
    #[arg(short = 'o', long = "output")]
    output: Option<PathBuf>,
    #[arg(short = 'f', long = "filename")]
    filename: Option<String>,
    #[arg(short = 'n', long = "no-output-file")]
    no_output_file: bool,
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

#[derive(Debug, Clone, ValueEnum)]
enum SceneListFormat {
    Csv,
    Json,
    Ndjson,
}

#[derive(Debug, Clone, ValueEnum)]
enum BoundaryReviewFormat {
    Csv,
    Json,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli.verbosity);

    let frame_rate_override = cli.framerate.map(FrameRate);
    let mut video_metadata = probe_video(&cli.input)
        .with_context(|| format!("failed to open input video {}", cli.input.display()))?;
    if let Some(frame_rate) = frame_rate_override {
        video_metadata.frame_rate = frame_rate;
    }
    let frame_rate = video_metadata.frame_rate;

    let detector_min_scene_len = detector_min_scene_len(&cli.detector);
    let min_scene_len = detector_min_scene_len
        .as_ref()
        .unwrap_or(&cli.min_scene_len);
    let min_scene_len = Timecode::parse_at_rate(min_scene_len, frame_rate)?.frames();
    let options = DetectionOptions {
        min_scene_len,
        min_scene_len_policy: if cli.merge_last_scene {
            MinSceneLenPolicy::MergeLast
        } else {
            MinSceneLenPolicy::Suppress
        },
    };

    let detector = detector_config(&cli.detector);
    let request = artifacts::scene_list_request(
        &cli.input,
        frame_rate,
        frame_rate_override,
        &detector,
        &options,
    )?;
    match output_command(&cli.detector) {
        OutputCommand::ListScenes(args) => {
            handle_list_scenes(&cli, args, detector, options, &request, frame_rate_override)?;
        }
        OutputCommand::ListBoundaries(args) => {
            let source = FfmpegFrameSource::open(&cli.input, frame_rate_override)
                .with_context(|| format!("failed to open input video {}", cli.input.display()))?;
            let review_options = BoundaryReviewOptions {
                review_threshold: args.review_threshold,
            };
            let review = if let Some(stats_path) = cli.stats.as_ref() {
                if let Some(parent) = stats_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                let file = File::create(stats_path).with_context(|| {
                    format!("failed to create stats file {}", stats_path.display())
                })?;
                let mut stats_sink = CsvStatsSink::new(file);
                detect_boundary_review_streaming(
                    detector,
                    source,
                    options,
                    review_options,
                    &mut stats_sink,
                )?
            } else {
                let mut stats_sink = NoopStatsSink;
                detect_boundary_review_streaming(
                    detector,
                    source,
                    options,
                    review_options,
                    &mut stats_sink,
                )?
            };

            if !args.no_output_file {
                let output_dir = args
                    .output
                    .as_ref()
                    .or(cli.output.as_ref())
                    .cloned()
                    .unwrap_or_else(|| PathBuf::from("."));
                fs::create_dir_all(&output_dir)?;
                let output_path = output_dir.join(boundary_review_filename(args));
                let file = File::create(&output_path).with_context(|| {
                    format!("failed to create boundary review {}", output_path.display())
                })?;
                write_boundary_review(&review, file, &args.format)?;
                if !cli.quiet && !args.quiet {
                    println!("{}", output_path.display());
                }
            } else if !cli.quiet && !args.quiet {
                write_boundary_review(&review, std::io::stdout(), &args.format)?;
            }
        }
        OutputCommand::ExportHtml(args) => {
            handle_export_html(&cli, args, detector, options, &request, frame_rate_override)?;
        }
    }

    Ok(())
}

enum SceneListSource {
    Artifact,
    Detection,
}

fn handle_list_scenes(
    cli: &Cli,
    args: &ListScenesArgs,
    detector: DetectorConfig,
    options: DetectionOptions,
    request: &artifacts::SceneListRequest,
    frame_rate_override: Option<FrameRate>,
) -> Result<()> {
    let request_key = artifacts::request_key(request)?;
    let report_reuse = !cli.quiet && !args.quiet;

    if !args.no_output_file {
        let output_dir = scene_list_output_dir(cli, args);
        fs::create_dir_all(&output_dir)?;
        let output_path = output_dir.join(scene_list_filename(args));
        let render_kind = scene_list_render_kind(&args.format);
        let manifest_path =
            artifacts::render_manifest_path(&output_dir, &output_path, render_kind)?;
        if can_reuse_scene_list(cli)
            && explicit_artifact_matches(cli, request)?
            && artifacts::reusable_output_exists(
                &manifest_path,
                &output_path,
                render_kind,
                &request_key,
                request,
            )?
        {
            if report_reuse {
                eprintln!("reusing Scene List output: {}", output_path.display());
                println!("{}", output_path.display());
            }
            return Ok(());
        }

        let artifact_path = scene_list_artifact_path(cli, Some(&output_dir), &request_key);
        let (scene_list, _) = get_or_create_scene_list(
            cli,
            detector,
            options,
            request,
            artifact_path.as_deref(),
            report_reuse,
            frame_rate_override,
        )?;
        let file = File::create(&output_path)
            .with_context(|| format!("failed to create scene list {}", output_path.display()))?;
        write_scene_list(&scene_list, file, &args.format)?;
        artifacts::write_render_manifest(
            &manifest_path,
            &output_path,
            render_kind,
            &request_key,
            request,
        )?;
        if !cli.quiet && !args.quiet {
            println!("{}", output_path.display());
        }
    } else {
        let artifact_path = cli.scene_list_artifact.as_deref();
        let (scene_list, _) = get_or_create_scene_list(
            cli,
            detector,
            options,
            request,
            artifact_path,
            report_reuse,
            frame_rate_override,
        )?;
        if !cli.quiet && !args.quiet {
            write_scene_list(&scene_list, std::io::stdout(), &args.format)?;
        }
    }

    Ok(())
}

fn handle_export_html(
    cli: &Cli,
    args: &ExportHtmlArgs,
    detector: DetectorConfig,
    options: DetectionOptions,
    request: &artifacts::SceneListRequest,
    frame_rate_override: Option<FrameRate>,
) -> Result<()> {
    let request_key = artifacts::request_key(request)?;
    let report_reuse = !cli.quiet && !args.quiet;

    if !args.no_output_file {
        let output_dir = export_html_output_dir(cli, args);
        fs::create_dir_all(&output_dir)?;
        let output_path = output_dir.join(export_html_filename(args));
        let render_kind = "scene_list_html";
        let manifest_path =
            artifacts::render_manifest_path(&output_dir, &output_path, render_kind)?;
        if can_reuse_scene_list(cli)
            && explicit_artifact_matches(cli, request)?
            && artifacts::reusable_output_exists(
                &manifest_path,
                &output_path,
                render_kind,
                &request_key,
                request,
            )?
        {
            if report_reuse {
                eprintln!("reusing Scene List output: {}", output_path.display());
                println!("{}", output_path.display());
            }
            return Ok(());
        }

        let artifact_path = scene_list_artifact_path(cli, Some(&output_dir), &request_key);
        let (scene_list, _) = get_or_create_scene_list(
            cli,
            detector,
            options,
            request,
            artifact_path.as_deref(),
            report_reuse,
            frame_rate_override,
        )?;
        let file = File::create(&output_path).with_context(|| {
            format!("failed to create HTML scene list {}", output_path.display())
        })?;
        write_scene_list_html(&scene_list, file)?;
        artifacts::write_render_manifest(
            &manifest_path,
            &output_path,
            render_kind,
            &request_key,
            request,
        )?;
        if !cli.quiet && !args.quiet {
            println!("{}", output_path.display());
        }
    } else {
        let artifact_path = cli.scene_list_artifact.as_deref();
        let (scene_list, _) = get_or_create_scene_list(
            cli,
            detector,
            options,
            request,
            artifact_path,
            report_reuse,
            frame_rate_override,
        )?;
        if !cli.quiet && !args.quiet {
            write_scene_list_html(&scene_list, std::io::stdout())?;
        }
    }

    Ok(())
}

fn get_or_create_scene_list(
    cli: &Cli,
    detector: DetectorConfig,
    options: DetectionOptions,
    request: &artifacts::SceneListRequest,
    artifact_path: Option<&Path>,
    report_reuse: bool,
    frame_rate_override: Option<FrameRate>,
) -> Result<(SceneList, SceneListSource)> {
    if can_reuse_scene_list(cli) {
        if let Some(path) = artifact_path {
            if let Some(scene_list) = artifacts::read_scene_list_artifact(path, request)? {
                if report_reuse {
                    eprintln!("reusing Scene List Artifact: {}", path.display());
                }
                return Ok((scene_list, SceneListSource::Artifact));
            }
        }
    }

    let source = FfmpegFrameSource::open(&cli.input, frame_rate_override)
        .with_context(|| format!("failed to open input video {}", cli.input.display()))?;
    let scene_list = if let Some(stats_path) = cli.stats.as_ref() {
        if let Some(parent) = stats_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = File::create(stats_path)
            .with_context(|| format!("failed to create stats file {}", stats_path.display()))?;
        let mut stats_sink = CsvStatsSink::new(file);
        detect_scenes_streaming(detector, source, options, &mut stats_sink)?
    } else {
        let mut stats_sink = NoopStatsSink;
        detect_scenes_streaming(detector, source, options, &mut stats_sink)?
    };

    if let Some(path) = artifact_path {
        artifacts::write_scene_list_artifact(path, request, &scene_list)?;
    }

    Ok((scene_list, SceneListSource::Detection))
}

fn can_reuse_scene_list(cli: &Cli) -> bool {
    !cli.force && cli.stats.is_none()
}

fn explicit_artifact_matches(cli: &Cli, request: &artifacts::SceneListRequest) -> Result<bool> {
    let Some(path) = cli.scene_list_artifact.as_deref() else {
        return Ok(true);
    };
    Ok(artifacts::read_scene_list_artifact(path, request)?.is_some())
}

fn scene_list_artifact_path(
    cli: &Cli,
    output_dir: Option<&Path>,
    request_key: &str,
) -> Option<PathBuf> {
    if let Some(path) = cli.scene_list_artifact.as_ref() {
        return Some(path.clone());
    }
    output_dir
        .map(|output_dir| artifacts::default_scene_list_artifact_path(output_dir, request_key))
}

fn scene_list_output_dir(cli: &Cli, args: &ListScenesArgs) -> PathBuf {
    args.output
        .as_ref()
        .or(cli.output.as_ref())
        .cloned()
        .unwrap_or_else(|| PathBuf::from("."))
}

fn export_html_output_dir(cli: &Cli, args: &ExportHtmlArgs) -> PathBuf {
    args.output
        .as_ref()
        .or(cli.output.as_ref())
        .cloned()
        .unwrap_or_else(|| PathBuf::from("."))
}

fn scene_list_filename(args: &ListScenesArgs) -> &str {
    args.filename.as_deref().unwrap_or(match &args.format {
        SceneListFormat::Csv => "scenes.csv",
        SceneListFormat::Json => "scenes.json",
        SceneListFormat::Ndjson => "scenes.ndjson",
    })
}

fn export_html_filename(args: &ExportHtmlArgs) -> &str {
    args.filename.as_deref().unwrap_or("scenes.html")
}

fn scene_list_render_kind(format: &SceneListFormat) -> &'static str {
    match format {
        SceneListFormat::Csv => "scene_list_csv",
        SceneListFormat::Json => "scene_list_json",
        SceneListFormat::Ndjson => "scene_events_ndjson",
    }
}

fn boundary_review_filename(args: &ListBoundariesArgs) -> &str {
    args.filename.as_deref().unwrap_or(match &args.format {
        BoundaryReviewFormat::Csv => "boundaries.csv",
        BoundaryReviewFormat::Json => "boundaries.json",
    })
}

fn write_scene_list<W: std::io::Write>(
    scene_list: &scenedetect_core::SceneList,
    writer: W,
    format: &SceneListFormat,
) -> Result<()> {
    match format {
        SceneListFormat::Csv => write_scene_list_csv(scene_list, writer),
        SceneListFormat::Json => write_scene_list_json(scene_list, writer),
        SceneListFormat::Ndjson => write_scene_events_ndjson(scene_list, writer),
    }?;
    Ok(())
}

fn write_boundary_review<W: std::io::Write>(
    review: &scenedetect_core::BoundaryReview,
    writer: W,
    format: &BoundaryReviewFormat,
) -> Result<()> {
    match format {
        BoundaryReviewFormat::Csv => write_boundary_review_csv(review, writer),
        BoundaryReviewFormat::Json => write_boundary_review_json(review, writer),
    }?;
    Ok(())
}

fn init_tracing(verbosity: &Verbosity) {
    let level = match verbosity {
        Verbosity::Debug => Some("debug"),
        Verbosity::Info => Some("info"),
        Verbosity::Warning => Some("warn"),
        Verbosity::Error => Some("error"),
        Verbosity::None => None,
    };

    if let Some(level) = level {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(level)
            .without_time()
            .try_init();
    }
}

fn detector_config(command: &DetectorCommand) -> DetectorConfig {
    match command {
        DetectorCommand::Content(args) => DetectorConfig::Content(ContentDetectorConfig {
            threshold: args.threshold,
            weights: parse_weights(args.weights.as_deref()),
            luma_only: args.luma_only,
        }),
        DetectorCommand::Adaptive(args) => DetectorConfig::Adaptive(AdaptiveDetectorConfig {
            threshold: args.threshold,
            min_content_val: args.min_content_val,
            frame_window: args.frame_window,
            weights: parse_weights(args.weights.as_deref()),
            luma_only: args.luma_only,
        }),
        DetectorCommand::Threshold(args) => DetectorConfig::Threshold(ThresholdDetectorConfig {
            threshold: args.threshold,
            fade_bias: args.fade_bias,
            add_last_scene: args.add_last_scene,
        }),
        DetectorCommand::Histogram(args) => DetectorConfig::Histogram(HistogramDetectorConfig {
            threshold: args.threshold,
            bins: args.bins,
        }),
        DetectorCommand::Hash(args) => DetectorConfig::Hash(HashDetectorConfig {
            threshold: args.threshold,
            size: args.size,
            lowpass: args.lowpass,
        }),
    }
}

fn detector_min_scene_len(command: &DetectorCommand) -> Option<String> {
    match command {
        DetectorCommand::Content(args) => args.min_scene_len.clone(),
        DetectorCommand::Adaptive(args) => args.min_scene_len.clone(),
        DetectorCommand::Threshold(args) => args.min_scene_len.clone(),
        DetectorCommand::Histogram(args) => args.min_scene_len.clone(),
        DetectorCommand::Hash(args) => args.min_scene_len.clone(),
    }
}

fn output_command(command: &DetectorCommand) -> &OutputCommand {
    match command {
        DetectorCommand::Content(args) => &args.output,
        DetectorCommand::Adaptive(args) => &args.output,
        DetectorCommand::Threshold(args) => &args.output,
        DetectorCommand::Histogram(args) => &args.output,
        DetectorCommand::Hash(args) => &args.output,
    }
}

fn parse_weights(weights: Option<&[f64]>) -> ContentWeights {
    let Some(weights) = weights else {
        return ContentWeights::default();
    };
    ContentWeights {
        hue: weights[0],
        saturation: weights[1],
        luminance: weights[2],
        edges: weights[3],
    }
}

fn parse_unit_interval(value: &str) -> std::result::Result<f64, String> {
    let parsed = value
        .parse::<f64>()
        .map_err(|_| format!("{value:?} is not a number"))?;
    if (0.0..=1.0).contains(&parsed) {
        Ok(parsed)
    } else {
        Err(format!("{parsed} must be between 0.0 and 1.0"))
    }
}

fn parse_1_to_256(value: &str) -> std::result::Result<usize, String> {
    let parsed = value
        .parse::<usize>()
        .map_err(|_| format!("{value:?} is not a positive integer"))?;
    if (1..=256).contains(&parsed) {
        Ok(parsed)
    } else {
        Err(format!("{parsed} must be between 1 and 256"))
    }
}

#[allow(dead_code)]
fn default_output_dir(input: &Path) -> PathBuf {
    input
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf()
}
