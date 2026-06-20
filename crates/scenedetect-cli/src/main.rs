mod artifacts;
mod scene_list_command;

use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use scenedetect_core::{
    detect_boundary_review_streaming, write_boundary_review_csv, write_boundary_review_json,
    AdaptiveDetectorConfig, BoundaryReviewOptions, ContentDetectorConfig, ContentWeights,
    CsvStatsSink, DetectionOptions, DetectorConfig, FrameRate, HashDetectorConfig,
    HistogramDetectorConfig, MinSceneLenPolicy, NoopStatsSink, ThresholdDetectorConfig, Timecode,
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

impl From<&SceneListFormat> for scene_list_command::SceneListOutputFormat {
    fn from(format: &SceneListFormat) -> Self {
        match format {
            SceneListFormat::Csv => Self::Csv,
            SceneListFormat::Json => Self::Json,
            SceneListFormat::Ndjson => Self::Ndjson,
        }
    }
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

fn handle_list_scenes(
    cli: &Cli,
    args: &ListScenesArgs,
    detector: DetectorConfig,
    options: DetectionOptions,
    request: &artifacts::SceneListRequest,
    frame_rate_override: Option<FrameRate>,
) -> Result<()> {
    let quiet = cli.quiet || args.quiet;
    if args.no_output_file {
        return scene_list_command::run_list_scenes_stdout(
            scene_list_command::ListScenesStdoutRequest {
                input: cli.input.clone(),
                detector,
                options,
                scene_list_request: request.clone(),
                scene_list_artifact: cli.scene_list_artifact.clone(),
                stats: cli.stats.clone(),
                force: cli.force,
                quiet,
                frame_rate_override,
                format: scene_list_command::SceneListOutputFormat::from(&args.format),
            },
        );
    }

    let output_dir = scene_list_output_dir(cli, args);
    fs::create_dir_all(&output_dir)?;
    let output_path = output_dir.join(scene_list_filename(args));
    let render_kind = scene_list_render_kind(&args.format);

    scene_list_command::run_list_scenes_file(scene_list_command::ListScenesFileFirstWriteRequest {
        input: cli.input.clone(),
        detector,
        options,
        scene_list_request: request.clone(),
        scene_list_artifact: cli.scene_list_artifact.clone(),
        output_dir,
        output_path,
        stats: cli.stats.clone(),
        force: cli.force,
        quiet,
        frame_rate_override,
        format: scene_list_command::SceneListOutputFormat::from(&args.format),
        render_kind,
    })
}

fn handle_export_html(
    cli: &Cli,
    args: &ExportHtmlArgs,
    detector: DetectorConfig,
    options: DetectionOptions,
    request: &artifacts::SceneListRequest,
    frame_rate_override: Option<FrameRate>,
) -> Result<()> {
    let quiet = cli.quiet || args.quiet;

    if args.no_output_file {
        return scene_list_command::run_export_html_stdout(
            scene_list_command::ExportHtmlStdoutRequest {
                input: cli.input.clone(),
                detector,
                options,
                scene_list_request: request.clone(),
                scene_list_artifact: cli.scene_list_artifact.clone(),
                stats: cli.stats.clone(),
                force: cli.force,
                quiet,
                frame_rate_override,
            },
        );
    }

    let file_request = scene_list_command::ExportHtmlFileRequest {
        input: cli.input.clone(),
        detector,
        options,
        scene_list_request: request.clone(),
        scene_list_artifact: cli.scene_list_artifact.clone(),
        output: args.output.as_ref().or(cli.output.as_ref()).cloned(),
        filename: args.filename.clone(),
        stats: cli.stats.clone(),
        force: cli.force,
        quiet,
        frame_rate_override,
    };
    scene_list_command::run_export_html_file(file_request)
}

fn scene_list_output_dir(cli: &Cli, args: &ListScenesArgs) -> PathBuf {
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
