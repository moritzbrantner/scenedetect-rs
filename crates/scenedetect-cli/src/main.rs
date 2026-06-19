use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use scenedetect_core::{
    detect_scenes_streaming, write_scene_events_ndjson, write_scene_list_csv,
    write_scene_list_json, AdaptiveDetectorConfig, ContentDetectorConfig, ContentWeights,
    CsvStatsSink, DetectionOptions, DetectorConfig, FrameRate, FrameSource, HashDetectorConfig,
    HistogramDetectorConfig, MinSceneLenPolicy, NoopStatsSink, ThresholdDetectorConfig, Timecode,
};
use scenedetect_ffmpeg::FfmpegFrameSource;

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

#[derive(Debug, Clone, ValueEnum)]
enum SceneListFormat {
    Csv,
    Json,
    Ndjson,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(&cli.verbosity);

    let frame_rate_override = cli.framerate.map(FrameRate);
    let source = FfmpegFrameSource::open(&cli.input, frame_rate_override)
        .with_context(|| format!("failed to open input video {}", cli.input.display()))?;
    let frame_rate = source.frame_rate();

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
    let list_scenes = list_scenes_args(&cli.detector);
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

    if !list_scenes.no_output_file {
        let output_dir = list_scenes
            .output
            .as_ref()
            .or(cli.output.as_ref())
            .cloned()
            .unwrap_or_else(|| PathBuf::from("."));
        fs::create_dir_all(&output_dir)?;
        let output_path = output_dir.join(scene_list_filename(list_scenes));
        let file = File::create(&output_path)
            .with_context(|| format!("failed to create scene list {}", output_path.display()))?;
        write_scene_list(&scene_list, file, &list_scenes.format)?;
        if !cli.quiet && !list_scenes.quiet {
            println!("{}", output_path.display());
        }
    } else if !cli.quiet && !list_scenes.quiet {
        write_scene_list(&scene_list, std::io::stdout(), &list_scenes.format)?;
    }

    Ok(())
}

fn scene_list_filename(args: &ListScenesArgs) -> &str {
    args.filename.as_deref().unwrap_or(match &args.format {
        SceneListFormat::Csv => "scenes.csv",
        SceneListFormat::Json => "scenes.json",
        SceneListFormat::Ndjson => "scenes.ndjson",
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

fn list_scenes_args(command: &DetectorCommand) -> &ListScenesArgs {
    match command {
        DetectorCommand::Content(args) => match &args.output {
            OutputCommand::ListScenes(args) => args,
        },
        DetectorCommand::Adaptive(args) => match &args.output {
            OutputCommand::ListScenes(args) => args,
        },
        DetectorCommand::Threshold(args) => match &args.output {
            OutputCommand::ListScenes(args) => args,
        },
        DetectorCommand::Histogram(args) => match &args.output {
            OutputCommand::ListScenes(args) => args,
        },
        DetectorCommand::Hash(args) => match &args.output {
            OutputCommand::ListScenes(args) => args,
        },
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
