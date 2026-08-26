#!/usr/bin/env python3
from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    if old not in text:
        raise SystemExit(f"missing patch anchor: {label}")
    return text.replace(old, new, 1)


root = Path(__file__).resolve().parents[1]
main_path = root / "crates/scenedetect-cli/src/main.rs"
main = main_path.read_text()

main = replace_once(
    main,
    "enum NativeDetectorCommand {\n    Content(NativeContentArgs),\n    Adaptive(NativeAdaptiveArgs),\n}\n",
    "enum NativeDetectorCommand {\n    Content(NativeContentArgs),\n    Adaptive(NativeAdaptiveArgs),\n    Threshold(NativeThresholdArgs),\n    #[command(name = \"hist\")]\n    Histogram(NativeHistogramArgs),\n    Hash(NativeHashArgs),\n}\n",
    "native detector enum",
)

args = r'''#[derive(Debug, Args)]
struct NativeThresholdArgs {
    #[arg(short = 'i', long = "input")]
    input: PathBuf,
    #[arg(short = 't', long = "threshold", default_value_t = 12.0)]
    threshold: f64,
    #[arg(short = 'f', long = "fade-bias", default_value_t = 0.0)]
    fade_bias: f64,
    #[arg(short = 'l', long = "add-last-scene", default_value_t = true)]
    add_last_scene: bool,
    #[arg(short = 'm', long = "min-scene-len", default_value = "15")]
    min_scene_len: String,
    #[arg(long = "progress", default_value = "auto")]
    progress: ProgressMode,
    #[arg(long = "force")]
    force: bool,
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

#[derive(Debug, Args)]
struct NativeHistogramArgs {
    #[arg(short = 'i', long = "input")]
    input: PathBuf,
    #[arg(short = 't', long = "threshold", default_value_t = 0.05, value_parser = parse_unit_interval)]
    threshold: f64,
    #[arg(short = 'b', long = "bins", default_value_t = 256, value_parser = parse_1_to_256)]
    bins: usize,
    #[arg(short = 'm', long = "min-scene-len", default_value = "15")]
    min_scene_len: String,
    #[arg(long = "progress", default_value = "auto")]
    progress: ProgressMode,
    #[arg(long = "force")]
    force: bool,
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

#[derive(Debug, Args)]
struct NativeHashArgs {
    #[arg(short = 'i', long = "input")]
    input: PathBuf,
    #[arg(short = 't', long = "threshold", default_value_t = 0.395, value_parser = parse_unit_interval)]
    threshold: f64,
    #[arg(short = 's', long = "size", default_value_t = 16, value_parser = parse_1_to_256)]
    size: usize,
    #[arg(short = 'l', long = "lowpass", default_value_t = 2, value_parser = parse_1_to_256)]
    lowpass: usize,
    #[arg(short = 'm', long = "min-scene-len", default_value = "15")]
    min_scene_len: String,
    #[arg(long = "progress", default_value = "auto")]
    progress: ProgressMode,
    #[arg(long = "force")]
    force: bool,
    #[arg(short = 'q', long = "quiet")]
    quiet: bool,
}

'''
main = replace_once(
    main,
    "#[derive(Debug, Args)]\nstruct NativeRenderArgs {",
    args + "#[derive(Debug, Args)]\nstruct NativeRenderArgs {",
    "native detector args",
)

main = replace_once(
    main,
    "    match &args.detector {\n"
    "        NativeDetectorCommand::Content(args) => handle_native_detect_content(cli, args),\n"
    "        NativeDetectorCommand::Adaptive(args) => handle_native_detect_adaptive(cli, args),\n"
    "    }\n",
    "    match &args.detector {\n"
    "        NativeDetectorCommand::Content(args) => handle_native_detect_content(cli, args),\n"
    "        NativeDetectorCommand::Adaptive(args) => handle_native_detect_adaptive(cli, args),\n"
    "        NativeDetectorCommand::Threshold(args) => handle_native_detect_threshold(cli, args),\n"
    "        NativeDetectorCommand::Histogram(args) => handle_native_detect_histogram(cli, args),\n"
    "        NativeDetectorCommand::Hash(args) => handle_native_detect_hash(cli, args),\n"
    "    }\n",
    "native dispatch",
)

handlers = r'''fn handle_native_detect_threshold(cli: &Cli, args: &NativeThresholdArgs) -> Result<()> {
    handle_native_detect_generic(
        cli,
        &args.input,
        &args.min_scene_len,
        DetectorConfig::Threshold(ThresholdDetectorConfig {
            threshold: args.threshold,
            fade_bias: args.fade_bias,
            add_last_scene: args.add_last_scene,
        }),
        args.progress,
        args.quiet,
        "threshold",
    )
}

fn handle_native_detect_histogram(cli: &Cli, args: &NativeHistogramArgs) -> Result<()> {
    handle_native_detect_generic(
        cli,
        &args.input,
        &args.min_scene_len,
        DetectorConfig::Histogram(HistogramDetectorConfig {
            threshold: args.threshold,
            bins: args.bins,
        }),
        args.progress,
        args.quiet,
        "hist",
    )
}

fn handle_native_detect_hash(cli: &Cli, args: &NativeHashArgs) -> Result<()> {
    if args.lowpass > args.size {
        return Err(anyhow!("--lowpass must be less than or equal to --size"));
    }
    handle_native_detect_generic(
        cli,
        &args.input,
        &args.min_scene_len,
        DetectorConfig::Hash(HashDetectorConfig {
            threshold: args.threshold,
            size: args.size,
            lowpass: args.lowpass,
        }),
        args.progress,
        args.quiet,
        "hash",
    )
}

fn handle_native_detect_generic(
    cli: &Cli,
    input: &std::path::Path,
    min_scene_len: &str,
    detector: DetectorConfig,
    progress: ProgressMode,
    detector_quiet: bool,
    detector_name: &str,
) -> Result<()> {
    let quiet = cli.quiet || detector_quiet;
    let metadata = probe_video(input)
        .with_context(|| format!("failed to open input video {}", input.display()))?;
    let min_scene_len = Timecode::parse_at_rate(min_scene_len, metadata.frame_rate)?.frames();
    let options = DetectionOptions {
        min_scene_len,
        min_scene_len_policy: MinSceneLenPolicy::Suppress,
    };
    let progress_enabled = progress_enabled(progress) && !quiet;
    if progress_enabled {
        eprintln!("detecting {detector_name}  0 frames  00:00:00.000  boundaries: 0");
    }

    let source = FfmpegFrameSource::open(input, None)
        .with_context(|| format!("failed to open input video {}", input.display()))?;
    let result = detect_scenes(detector.clone(), source, options.clone())?;
    let boundary_count = result.scene_list.scenes.len().saturating_sub(1);
    let total_frames = result
        .scene_list
        .scenes
        .last()
        .map(|scene| scene.end.0)
        .unwrap_or(0);
    let stats_path = native_stats::detection_stats_path_for_input(input)?;
    let document = native_stats::DetectionStatsDocument::from_detection_result(
        input,
        &metadata,
        detector,
        options,
        result,
    )?;
    native_stats::write_detection_stats(&stats_path, &document)?;

    if progress_enabled {
        let timecode = Timecode::from_frames(total_frames).display_at_rate(metadata.frame_rate);
        eprintln!(
            "detecting {detector_name}  {total_frames} frames  {timecode}  100%  boundaries: {boundary_count}"
        );
        eprintln!("wrote Detection Stats: {}", stats_path.display());
    }
    Ok(())
}

'''
main = replace_once(
    main,
    "fn handle_native_render(args: &NativeRenderArgs) -> Result<()> {",
    handlers + "fn handle_native_render(args: &NativeRenderArgs) -> Result<()> {",
    "native handlers",
)
main_path.write_text(main)

cases_path = root / "tests/parity/cases.toml"
cases = cases_path.read_text()
for case_id, detector in [
    ("threshold-fade-return", "threshold"),
    ("hist-hard-cut", "hist"),
    ("hash-pattern-cut", "hash"),
]:
    anchor = f'id = "{case_id}"\nstatus = "required"\n'
    cases = replace_once(
        cases,
        anchor,
        anchor + f'native_detector = "{detector}"\n',
        case_id,
    )
cases_path.write_text(cases)

test_path = root / "crates/scenedetect-cli/tests/native_detectors.rs"
test_path.write_text(r'''use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg").arg("-version").output().is_ok()
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn copy_fixture(root: &Path, temp: &Path, name: &str) -> PathBuf {
    let video = temp.join(name);
    std::fs::copy(root.join("tests/fixtures/generated").join(name), &video).unwrap();
    video
}

fn run_detect(video: &Path, detector: &str, args: &[&str]) -> serde_json::Value {
    let mut command = Command::cargo_bin("scenedetect-rs").unwrap();
    command.args(["detect", detector]).arg("-i").arg(video);
    command.args(args).args(["--progress", "never", "--quiet"]);
    command.assert().success();
    let stats_path = video.with_extension("scenedetect.json");
    serde_json::from_str(&std::fs::read_to_string(stats_path).unwrap()).unwrap()
}

#[test]
fn remaining_native_detectors_persist_reusable_stats() {
    if !ffmpeg_available() {
        eprintln!("skipping native detector test because ffmpeg is unavailable");
        return;
    }
    let root = repository_root();
    assert!(Command::new("bash")
        .arg(root.join("scripts/generate-fixtures.sh"))
        .status()
        .unwrap()
        .success());
    let temp = tempfile::tempdir().unwrap();

    let threshold_video = copy_fixture(&root, temp.path(), "threshold-fade-return.mkv");
    let threshold = run_detect(
        &threshold_video,
        "threshold",
        &["--threshold", "12", "--min-scene-len", "1"],
    );
    assert_eq!(threshold["schema_version"], 2);
    assert_eq!(threshold["detector"]["name"], "threshold");
    assert!(threshold["metric_names"]
        .as_array()
        .unwrap()
        .iter()
        .any(|name| name == "average_rgb"));

    let hist_video = copy_fixture(&root, temp.path(), "content-hard-cut.mkv");
    let hist = run_detect(
        &hist_video,
        "hist",
        &[
            "--threshold",
            "0.05",
            "--bins",
            "256",
            "--min-scene-len",
            "1",
        ],
    );
    assert_eq!(hist["detector"]["name"], "hist");
    assert!(hist["metric_names"]
        .as_array()
        .unwrap()
        .iter()
        .any(|name| name.as_str().unwrap().starts_with("hist_diff")));

    let hash_video = copy_fixture(&root, temp.path(), "hash-pattern-cut.mkv");
    let hash = run_detect(
        &hash_video,
        "hash",
        &[
            "--threshold",
            "0.395",
            "--size",
            "16",
            "--lowpass",
            "2",
            "--min-scene-len",
            "1",
        ],
    );
    assert_eq!(hash["detector"]["name"], "hash");
    assert!(hash["metric_names"]
        .as_array()
        .unwrap()
        .iter()
        .any(|name| name.as_str().unwrap().starts_with("hash_dist")));

    for video in [&threshold_video, &hist_video, &hash_video] {
        let scenes = video.with_extension("scenes.json");
        Command::cargo_bin("scenedetect-rs")
            .unwrap()
            .args(["render", "scenes"])
            .arg("-i")
            .arg(video)
            .args(["--format", "json", "--output"])
            .arg(&scenes)
            .assert()
            .success();
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(scenes).unwrap()).unwrap();
        assert!(value["scene_count"].as_u64().unwrap() >= 1);

        let csv = video.with_extension("stats.csv");
        Command::cargo_bin("scenedetect-rs")
            .unwrap()
            .args(["render", "stats"])
            .arg("-i")
            .arg(video)
            .args(["--csv", "--output"])
            .arg(&csv)
            .assert()
            .success();
        assert!(std::fs::metadata(csv).unwrap().len() > 0);

        let html = video.with_extension("scenes.html");
        Command::cargo_bin("scenedetect-rs")
            .unwrap()
            .args(["render", "html"])
            .arg("-i")
            .arg(video)
            .arg("--output")
            .arg(&html)
            .assert()
            .success();
        assert!(std::fs::read_to_string(html).unwrap().contains("Scene List"));

        Command::cargo_bin("scenedetect-rs")
            .unwrap()
            .args(["render", "boundaries"])
            .arg("-i")
            .arg(video)
            .assert()
            .failure();
    }
}
''')
