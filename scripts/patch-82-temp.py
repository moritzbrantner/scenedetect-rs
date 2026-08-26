#!/usr/bin/env python3
from pathlib import Path


def replace_once(text: str, old: str, new: str, label: str) -> str:
    if old not in text:
        raise SystemExit(f"missing patch anchor: {label}")
    return text.replace(old, new, 1)


def replace_between(text: str, start: str, end: str, replacement: str, label: str) -> str:
    start_index = text.find(start)
    if start_index < 0:
        raise SystemExit(f"missing patch start: {label}")
    end_index = text.find(end, start_index)
    if end_index < 0:
        raise SystemExit(f"missing patch end: {label}")
    return text[:start_index] + replacement.rstrip() + "\n\n" + text[end_index:]


root = Path(__file__).resolve().parents[1]
main_path = root / "crates/scenedetect-cli/src/main.rs"
main = main_path.read_text()

main = replace_once(
    main,
    "    boundary_review_from_content_detection_stats, detect_boundary_review_streaming,\n"
    "    detect_content_stats, detection_stats_from_content_detection_stats,\n",
    "    boundary_review_from_content_detection_stats, detect_boundary_review_streaming,\n"
    "    detect_content_stats, detect_scenes,\n",
    "native imports",
)
main = replace_once(
    main,
    "enum NativeDetectorCommand {\n    Content(NativeContentArgs),\n}\n",
    "enum NativeDetectorCommand {\n    Content(NativeContentArgs),\n    Adaptive(NativeAdaptiveArgs),\n}\n",
    "native detector enum",
)

adaptive_args = r'''#[derive(Debug, Args)]
struct NativeAdaptiveArgs {
    #[arg(short = 'i', long = "input")]
    input: PathBuf,
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
    adaptive_args + "#[derive(Debug, Args)]\nstruct NativeRenderArgs {",
    "adaptive args",
)
main = replace_once(
    main,
    "    match &args.detector {\n"
    "        NativeDetectorCommand::Content(args) => handle_native_detect_content(cli, args),\n"
    "    }\n",
    "    match &args.detector {\n"
    "        NativeDetectorCommand::Content(args) => handle_native_detect_content(cli, args),\n"
    "        NativeDetectorCommand::Adaptive(args) => handle_native_detect_adaptive(cli, args),\n"
    "    }\n",
    "native dispatch",
)

adaptive_handler = r'''fn handle_native_detect_adaptive(cli: &Cli, args: &NativeAdaptiveArgs) -> Result<()> {
    let quiet = cli.quiet || args.quiet;
    let metadata = probe_video(&args.input)
        .with_context(|| format!("failed to open input video {}", args.input.display()))?;
    let min_scene_len = Timecode::parse_at_rate(&args.min_scene_len, metadata.frame_rate)?.frames();
    let options = DetectionOptions {
        min_scene_len,
        min_scene_len_policy: MinSceneLenPolicy::Suppress,
    };
    let detector = DetectorConfig::Adaptive(AdaptiveDetectorConfig {
        threshold: args.threshold,
        min_content_val: args.min_content_val,
        frame_window: args.frame_window,
        weights: parse_weights(args.weights.as_deref()),
        luma_only: args.luma_only,
    });

    let progress_enabled = progress_enabled(args.progress) && !quiet;
    if progress_enabled {
        eprintln!("detecting adaptive  0 frames  00:00:00.000  boundaries: 0");
    }

    let source = FfmpegFrameSource::open(&args.input, None)
        .with_context(|| format!("failed to open input video {}", args.input.display()))?;
    let result = detect_scenes(detector.clone(), source, options.clone())?;
    let boundary_count = result.scene_list.scenes.len().saturating_sub(1);
    let total_frames = result
        .scene_list
        .scenes
        .last()
        .map(|scene| scene.end.0)
        .unwrap_or(0);
    let stats_path = native_stats::detection_stats_path_for_input(&args.input)?;
    let document = native_stats::DetectionStatsDocument::from_detection_result(
        &args.input,
        &metadata,
        detector,
        options,
        result,
    )?;
    native_stats::write_detection_stats(&stats_path, &document)?;

    if progress_enabled {
        let timecode = Timecode::from_frames(total_frames).display_at_rate(metadata.frame_rate);
        eprintln!(
            "detecting adaptive  {total_frames} frames  {timecode}  100%  boundaries: {boundary_count}"
        );
        eprintln!("wrote Detection Stats: {}", stats_path.display());
    }

    Ok(())
}
'''
main = replace_once(
    main,
    "fn handle_native_render(args: &NativeRenderArgs) -> Result<()> {",
    adaptive_handler + "\nfn handle_native_render(args: &NativeRenderArgs) -> Result<()> {",
    "adaptive handler",
)

main = replace_between(
    main,
    "fn handle_native_render_scenes(args: &NativeRenderScenesArgs) -> Result<()> {",
    "fn handle_native_render_stats(args: &NativeRenderStatsArgs) -> Result<()> {",
    r'''fn handle_native_render_scenes(args: &NativeRenderScenesArgs) -> Result<()> {
    let document = native_stats::read_detection_stats_document_for_input(&args.input)?;
    let scene_list = document.scene_list()?;
    let output_path = args
        .output
        .clone()
        .unwrap_or(native_stats::render_output_path_for_input(
            &args.input,
            match args.format {
                SceneListFormat::Csv => "scenes.csv",
                SceneListFormat::Json => "scenes.json",
                SceneListFormat::Ndjson => "scenes.ndjson",
            },
        )?);
    let file = File::create(&output_path)
        .with_context(|| format!("failed to create Scene List {}", output_path.display()))?;
    write_native_scene_list(&scene_list, file, &args.format)?;
    println!("{}", output_path.display());
    Ok(())
}''',
    "render scenes",
)
main = replace_between(
    main,
    "fn handle_native_render_stats(args: &NativeRenderStatsArgs) -> Result<()> {",
    "fn handle_native_render_boundaries(args: &NativeRenderBoundariesArgs) -> Result<()> {",
    r'''fn handle_native_render_stats(args: &NativeRenderStatsArgs) -> Result<()> {
    if !args.csv {
        return Err(anyhow!("native stats rendering currently requires --csv"));
    }
    let document = native_stats::read_detection_stats_document_for_input(&args.input)?;
    let stats = document.detection_stats()?;
    let output_path = args
        .output
        .clone()
        .unwrap_or(native_stats::render_output_path_for_input(
            &args.input,
            "stats.csv",
        )?);
    let file = File::create(&output_path).with_context(|| {
        format!(
            "failed to create Detection Stats CSV {}",
            output_path.display()
        )
    })?;
    write_stats_csv(&stats, file)?;
    println!("{}", output_path.display());
    Ok(())
}''',
    "render stats",
)
main = replace_between(
    main,
    "fn handle_native_render_boundaries(args: &NativeRenderBoundariesArgs) -> Result<()> {",
    "fn handle_native_render_html(args: &NativeRenderHtmlArgs) -> Result<()> {",
    r'''fn handle_native_render_boundaries(args: &NativeRenderBoundariesArgs) -> Result<()> {
    let document = native_stats::read_detection_stats_document_for_input(&args.input)?;
    if !matches!(
        document.detector,
        native_stats::DetectionStatsDetector::Content(_)
    ) {
        return Err(anyhow!(
            "native Boundary Candidate review is not available for {} Detection Stats",
            document.detector.name()
        ));
    }
    let stats = document.into_content_stats()?;
    let output_path = args
        .output
        .clone()
        .unwrap_or(native_stats::render_output_path_for_input(
            &args.input,
            match args.format {
                BoundaryReviewFormat::Csv => "boundaries.csv",
                BoundaryReviewFormat::Json => "boundaries.json",
            },
        )?);
    let review = boundary_review_from_content_detection_stats(
        &stats,
        BoundaryReviewOptions {
            review_threshold: args.review_threshold,
        },
    );
    let file = File::create(&output_path).with_context(|| {
        format!(
            "failed to create Boundary Candidate review {}",
            output_path.display()
        )
    })?;
    write_boundary_review(&review, file, &args.format)?;
    println!("{}", output_path.display());
    Ok(())
}''',
    "render boundaries",
)
main = replace_between(
    main,
    "fn handle_native_render_html(args: &NativeRenderHtmlArgs) -> Result<()> {",
    "fn write_native_scene_list<W: std::io::Write>(",
    r'''fn handle_native_render_html(args: &NativeRenderHtmlArgs) -> Result<()> {
    let document = native_stats::read_detection_stats_document_for_input(&args.input)?;
    let scene_list = document.scene_list()?;
    let output_path = args
        .output
        .clone()
        .unwrap_or(native_stats::render_output_path_for_input(
            &args.input,
            "scenes.html",
        )?);
    let file = File::create(&output_path)
        .with_context(|| format!("failed to create HTML Scene List {}", output_path.display()))?;
    write_scene_list_html(&scene_list, file)?;
    println!("{}", output_path.display());
    Ok(())
}''',
    "render html",
)
main_path.write_text(main)

parity_path = root / "tests/parity/run.py"
parity = parity_path.read_text()
parity = replace_once(
    parity,
    "    reason = case.get(\"reason\")\n",
    "    native_detector = case.get(\"native_detector\")\n"
    "    if native_detector is not None and native_detector not in {\n"
    "        \"content\", \"adaptive\", \"threshold\", \"hist\", \"hash\"\n"
    "    }:\n"
    "        raise ConfigError(f\"{label} has invalid native_detector {native_detector!r}\")\n\n"
    "    reason = case.get(\"reason\")\n",
    "native detector validation",
)

native_helper = r'''
def load_native_scene_json(path: Path, case_id: str) -> list[dict[str, int]]:
    try:
        with path.open() as file:
            payload = json.load(file)
    except json.JSONDecodeError as error:
        raise AssertionError(f"{case_id}: native Scene List JSON is invalid: {error}") from error
    scenes = payload.get("scenes") if isinstance(payload, dict) else None
    if not isinstance(scenes, list):
        raise AssertionError(f"{case_id}: native Scene List scenes must be an array")
    normalized = []
    for index, scene in enumerate(scenes, start=1):
        if not isinstance(scene, dict):
            raise AssertionError(f"{case_id}: native scene {index} must be an object")
        start = scene.get("start_frame")
        end = scene.get("end_frame")
        if not isinstance(start, int) or not isinstance(end, int):
            raise AssertionError(f"{case_id}: native scene {index} has invalid frame columns")
        normalized.append({"start": start - 1, "end": end})
    return normalized


def run_native_case(
    case: dict[str, Any],
    candidate_bin: Path,
    video: Path,
    reference: list[dict[str, int]],
    case_dir: Path,
) -> None:
    native_detector = case.get("native_detector")
    if native_detector is None:
        return
    native_args = detector_args(case, "candidate_args")
    run(
        [
            str(candidate_bin),
            "detect",
            native_detector,
            "-i",
            str(video),
            *native_args,
            "--min-scene-len",
            case["min_scene_len"],
            "--progress",
            "never",
            "--quiet",
        ]
    )
    native_json = case_dir / "native-scenes.json"
    run(
        [
            str(candidate_bin),
            "render",
            "scenes",
            "-i",
            str(video),
            "--format",
            "json",
            "--output",
            str(native_json),
        ]
    )
    native_scenes = load_native_scene_json(native_json, case["id"])
    compare_scenes(case, reference, native_scenes)
    print(f"native parity ok: {case['id']}: {len(reference)} scenes")

'''
parity = replace_once(
    parity,
    "def run_required_case(\n",
    native_helper + "def run_required_case(\n",
    "native parity helper",
)
parity = replace_once(
    parity,
    "    compare_scenes(case, reference, candidate)\n"
    "    print(f\"parity ok: {case_id}: {len(reference)} scenes\")\n",
    "    compare_scenes(case, reference, candidate)\n"
    "    print(f\"parity ok: {case_id}: {len(reference)} scenes\")\n"
    "    run_native_case(case, candidate_bin, video, reference, case_dir)\n",
    "native parity invocation",
)
parity_path.write_text(parity)

cases_path = root / "tests/parity/cases.toml"
cases = cases_path.read_text()
cases = replace_once(
    cases,
    'id = "adaptive-fast-motion"\nstatus = "required"\nvideo = "tests/fixtures/generated/adaptive-fast-motion.mkv"\ndetector = "detect-adaptive"\n',
    'id = "adaptive-fast-motion"\nstatus = "required"\nvideo = "tests/fixtures/generated/adaptive-fast-motion.mkv"\ndetector = "detect-adaptive"\nnative_detector = "adaptive"\n',
    "adaptive native parity case",
)
cases_path.write_text(cases)

test_path = root / "crates/scenedetect-cli/tests/native_adaptive.rs"
test_path.write_text(r'''use std::path::PathBuf;
use std::process::Command;

use assert_cmd::prelude::*;

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg").arg("-version").output().is_ok()
}

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn native_adaptive_detection_reuses_one_stats_artifact_for_renders() {
    if !ffmpeg_available() {
        eprintln!("skipping adaptive native test because ffmpeg is unavailable");
        return;
    }

    let root = repository_root();
    assert!(Command::new("bash")
        .arg(root.join("scripts/generate-fixtures.sh"))
        .status()
        .unwrap()
        .success());
    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("adaptive-fast-motion.mkv");
    std::fs::copy(
        root.join("tests/fixtures/generated/adaptive-fast-motion.mkv"),
        &video,
    )
    .unwrap();

    Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .args(["detect", "adaptive"])
        .arg("-i")
        .arg(&video)
        .args([
            "--threshold",
            "3",
            "--min-content-val",
            "15",
            "--frame-window",
            "2",
            "--min-scene-len",
            "1",
            "--progress",
            "never",
            "--quiet",
        ])
        .assert()
        .success();

    let stats_path = temp.path().join("adaptive-fast-motion.scenedetect.json");
    let stats: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&stats_path).unwrap()).unwrap();
    assert_eq!(stats["schema_version"], 2);
    assert_eq!(stats["detector"]["name"], "adaptive");
    assert!(stats["metric_names"]
        .as_array()
        .unwrap()
        .iter()
        .any(|name| name == "adaptive_ratio"));
    assert!(stats["rows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|row| row["decision"] == "accepted"));

    let native_json = temp.path().join("native-scenes.json");
    Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .args(["render", "scenes"])
        .arg("-i")
        .arg(&video)
        .args(["--format", "json", "--output"])
        .arg(&native_json)
        .assert()
        .success();

    let legacy = Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .arg("-i")
        .arg(&video)
        .args([
            "-m",
            "1",
            "detect-adaptive",
            "--threshold",
            "3",
            "--min-content-val",
            "15",
            "--frame-window",
            "2",
            "list-scenes",
            "--format",
            "json",
            "--no-output-file",
            "--quiet",
        ])
        .output()
        .unwrap();
    assert!(legacy.status.success());
    let legacy_json: serde_json::Value = serde_json::from_slice(&legacy.stdout).unwrap();
    let native: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(native_json).unwrap()).unwrap();
    assert_eq!(native["scenes"], legacy_json["scenes"]);

    let stats_csv = temp.path().join("adaptive-stats.csv");
    Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .args(["render", "stats"])
        .arg("-i")
        .arg(&video)
        .arg("--csv")
        .arg("--output")
        .arg(&stats_csv)
        .assert()
        .success();
    assert!(std::fs::read_to_string(stats_csv)
        .unwrap()
        .contains("adaptive_ratio"));

    let html = temp.path().join("adaptive-scenes.html");
    Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .args(["render", "html"])
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&html)
        .assert()
        .success();
    assert!(std::fs::read_to_string(html).unwrap().contains("Scene List"));

    let boundary_output = Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .args(["render", "boundaries"])
        .arg("-i")
        .arg(&video)
        .output()
        .unwrap();
    assert!(!boundary_output.status.success());
    assert!(String::from_utf8(boundary_output.stderr)
        .unwrap()
        .contains("not available for adaptive Detection Stats"));
}
''')
