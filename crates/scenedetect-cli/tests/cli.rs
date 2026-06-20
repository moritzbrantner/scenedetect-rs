use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use assert_cmd::prelude::*;
use predicates::prelude::*;

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg").arg("-version").output().is_ok()
}

fn write_two_color_video(video: &std::path::Path) {
    assert!(Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=16x16:d=0.3:r=10",
            "-f",
            "lavfi",
            "-i",
            "color=c=white:s=16x16:d=0.3:r=10",
            "-filter_complex",
            "[0:v][1:v]concat=n=2:v=1:a=0",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(video)
        .status()
        .unwrap()
        .success());
}

fn write_three_color_video(video: &std::path::Path) {
    assert!(Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=16x16:d=0.3:r=10",
            "-f",
            "lavfi",
            "-i",
            "color=c=white:s=16x16:d=0.3:r=10",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=16x16:d=0.3:r=10",
            "-filter_complex",
            "[0:v][1:v][2:v]concat=n=3:v=1:a=0",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(video)
        .status()
        .unwrap()
        .success());
}

fn write_review_candidate_video(video: &std::path::Path) {
    assert!(Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=16x16:d=0.3:r=10",
            "-f",
            "lavfi",
            "-i",
            "color=c=white:s=16x16:d=0.3:r=10",
            "-f",
            "lavfi",
            "-i",
            "color=c=gray:s=16x16:d=0.3:r=10",
            "-filter_complex",
            "[0:v][1:v][2:v]concat=n=3:v=1:a=0",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(video)
        .status()
        .unwrap()
        .success());
}

fn write_threshold_final_fade_video(video: &std::path::Path) {
    assert!(Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=white:s=16x16:d=0.2:r=10",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=16x16:d=0.3:r=10",
            "-filter_complex",
            "[0:v][1:v]concat=n=2:v=1:a=0",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(video)
        .status()
        .unwrap()
        .success());
}

fn write_hash_pattern_video(video: &std::path::Path) {
    assert!(Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=16x16:d=0.3:r=10",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=16x16:rate=10:duration=0.3",
            "-filter_complex",
            "[0:v][1:v]concat=n=2:v=1:a=0,format=rgb24",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(video)
        .status()
        .unwrap()
        .success());
}

fn single_render_manifest(output_dir: &Path) -> PathBuf {
    let render_dir = output_dir.join(".scenedetect-rs").join("renders");
    let manifests = std::fs::read_dir(render_dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(manifests.len(), 1);
    manifests[0].clone()
}

fn export_html_render_manifest(output_dir: &Path) -> PathBuf {
    single_render_manifest(output_dir)
}

fn list_scenes_render_manifest(output_dir: &Path) -> PathBuf {
    single_render_manifest(output_dir)
}

fn assert_single_hidden_scene_list_artifact(output_dir: &Path) {
    let artifact_dir = output_dir.join(".scenedetect-rs").join("scene-list");
    let artifacts = std::fs::read_dir(artifact_dir).unwrap().count();
    assert_eq!(artifacts, 1);
}

fn assert_success_without_reusable_output(output: Output) {
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!stderr.contains("reusing Scene List output"));
}

#[test]
fn native_detect_content_writes_visible_detection_stats_json() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    write_two_color_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.args(["detect", "content"])
        .arg("-i")
        .arg(&video)
        .args(["--threshold", "20", "--min-scene-len", "1"])
        .assert()
        .success();

    let stats_path = temp.path().join("scene-change.scenedetect.json");
    let stats: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(stats_path).unwrap()).unwrap();

    assert_eq!(stats["schema_version"], 1);
    assert_eq!(stats["kind"], "detection_stats");
    assert_eq!(stats["detector"]["name"], "content");
    assert_eq!(stats["detector"]["config"]["threshold"], 20.0);
    assert_eq!(stats["options"]["min_scene_len"], 1);
    assert_eq!(stats["metric_names"][0], "content_val");
    assert!(stats["input"]["path"]
        .as_str()
        .unwrap()
        .ends_with("scene-change.mp4"));
    assert!(stats["input"]["byte_len"].as_u64().unwrap() > 0);
    assert_eq!(stats["rows"].as_array().unwrap().len(), 6);
    assert_eq!(stats["rows"][0]["score"], 0.0);
    assert_eq!(stats["rows"][0]["threshold"], 20.0);
    assert_eq!(stats["rows"][0]["decision"], "not_evaluated");
    assert_eq!(stats["rows"][3]["decision"], "accepted");
    assert!(stats["rows"][3]["metrics"]["content_val"].as_f64().unwrap() >= 20.0);
}

#[test]
fn native_detect_content_overwrites_stale_detection_stats_json() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    write_two_color_video(&video);
    let stats_path = temp.path().join("scene-change.scenedetect.json");
    std::fs::write(&stats_path, r#"{"kind":"stale"}"#).unwrap();

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.args(["detect", "content"])
        .arg("-i")
        .arg(&video)
        .args(["--threshold", "20", "--min-scene-len", "1"])
        .assert()
        .success();

    let stats: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(stats_path).unwrap()).unwrap();
    assert_eq!(stats["schema_version"], 1);
    assert_eq!(stats["kind"], "detection_stats");
}

#[test]
fn native_render_commands_derive_outputs_from_detection_stats() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    write_two_color_video(&video);

    let mut detect = Command::cargo_bin("scenedetect-rs").unwrap();
    detect
        .args(["detect", "content"])
        .arg("-i")
        .arg(&video)
        .args(["--threshold", "20", "--min-scene-len", "1"])
        .assert()
        .success();

    std::fs::remove_file(&video).unwrap();

    for render_args in [
        vec!["render", "scenes"],
        vec!["render", "stats", "--csv"],
        vec!["render", "boundaries"],
        vec!["render", "html"],
    ] {
        let mut render = Command::cargo_bin("scenedetect-rs").unwrap();
        render
            .args(render_args)
            .arg("-i")
            .arg(&video)
            .assert()
            .success();
    }

    let scenes = std::fs::read_to_string(temp.path().join("scene-change.scenes.csv")).unwrap();
    assert!(scenes.contains("Scene Number,Start Frame,Start Timecode"));
    assert!(scenes.lines().count() >= 3);

    let stats = std::fs::read_to_string(temp.path().join("scene-change.stats.csv")).unwrap();
    assert!(stats.contains("Frame Number,content_val"));
    assert!(stats.contains("delta_edges"));

    let boundaries =
        std::fs::read_to_string(temp.path().join("scene-change.boundaries.csv")).unwrap();
    assert!(boundaries.contains("Rank,Status,Boundary Candidate Number"));
    assert!(boundaries.contains("accepted"));

    let html = std::fs::read_to_string(temp.path().join("scene-change.scenes.html")).unwrap();
    assert!(html.contains("<title>Scene List</title>"));
}

#[test]
fn native_detect_content_reports_progress_to_stderr_when_forced() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    write_two_color_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.args(["detect", "content"])
        .arg("-i")
        .arg(&video)
        .args([
            "--threshold",
            "20",
            "--min-scene-len",
            "1",
            "--progress",
            "always",
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("detecting content"))
        .stderr(predicate::str::contains("frames"))
        .stderr(predicate::str::contains("100%"))
        .stderr(predicate::str::contains("boundaries:"))
        .stderr(predicate::str::contains("wrote Detection Stats:"));
}

#[test]
fn native_detect_content_suppresses_progress_for_auto_noninteractive_and_never() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    for progress in ["auto", "never"] {
        let temp = tempfile::tempdir().unwrap();
        let video = temp.path().join("scene-change.mp4");
        write_two_color_video(&video);

        let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
        cmd.args(["detect", "content"])
            .arg("-i")
            .arg(&video)
            .args(["--threshold", "20", "--min-scene-len", "1"])
            .args(["--progress", progress])
            .assert()
            .success()
            .stdout(predicate::str::is_empty())
            .stderr(predicate::str::is_empty());
    }
}

#[test]
fn native_render_fails_when_detection_stats_are_missing() {
    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("missing.mp4");

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.args(["render", "scenes"])
        .arg("-i")
        .arg(&video)
        .assert()
        .failure()
        .stderr(predicate::str::contains("Detection Stats are missing"))
        .stderr(predicate::str::contains("scenedetect-rs detect content"));
}

#[test]
fn native_render_fails_when_detection_stats_are_invalid() {
    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("broken.mp4");
    std::fs::write(temp.path().join("broken.scenedetect.json"), "not json").unwrap();

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.args(["render", "scenes"])
        .arg("-i")
        .arg(&video)
        .assert()
        .failure()
        .stderr(predicate::str::contains("failed to parse Detection Stats"));
}

#[test]
fn content_detector_writes_score_ranked_boundary_candidates_to_csv() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("boundary-review.mp4");
    let output_dir = temp.path().join("out");
    write_review_candidate_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args(["-m", "1"])
        .args([
            "detect-content",
            "--threshold",
            "200",
            "list-boundaries",
            "--review-threshold",
            "100",
        ])
        .assert()
        .success();

    let boundaries = std::fs::read_to_string(output_dir.join("boundaries.csv")).unwrap();
    assert!(boundaries.contains("Rank,Status,Boundary Candidate Number"));
    assert!(boundaries.contains("accepted"));
    assert!(boundaries.contains("near_miss"));
    assert!(!output_dir.join("scenes.csv").exists());
}

#[test]
fn content_detector_writes_boundary_candidates_as_json_to_stdout() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("boundary-review.mp4");
    write_review_candidate_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    let output = cmd
        .current_dir(temp.path())
        .arg("-i")
        .arg(&video)
        .args(["-m", "1"])
        .args([
            "detect-content",
            "--threshold",
            "200",
            "list-boundaries",
            "--review-threshold",
            "100",
            "--format",
            "json",
            "--no-output-file",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let review: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(review["detector"], "content");
    assert_eq!(review["sort"], "threshold_distance");
    assert_eq!(review["score_metric"], "content_val");
    assert!(review["candidate_count"].as_u64().unwrap() >= 2);
    assert_eq!(
        review["boundary_candidates"][0]["boundary_frame_index"]
            .as_u64()
            .unwrap()
            + 1,
        review["boundary_candidates"][0]["boundary_frame"]
            .as_u64()
            .unwrap()
    );
    assert!(!temp.path().join("boundaries.json").exists());
}

#[test]
fn content_boundary_review_threshold_controls_near_miss_inclusion() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("boundary-review.mp4");
    write_review_candidate_video(&video);

    let mut default_cutoff = Command::cargo_bin("scenedetect-rs").unwrap();
    let default_output = default_cutoff
        .arg("-i")
        .arg(&video)
        .args(["-m", "1"])
        .args([
            "detect-content",
            "--threshold",
            "200",
            "list-boundaries",
            "--format",
            "json",
            "--no-output-file",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let mut relaxed_cutoff = Command::cargo_bin("scenedetect-rs").unwrap();
    let relaxed_output = relaxed_cutoff
        .arg("-i")
        .arg(&video)
        .args(["-m", "1"])
        .args([
            "detect-content",
            "--threshold",
            "200",
            "list-boundaries",
            "--review-threshold",
            "100",
            "--format",
            "json",
            "--no-output-file",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let default_review: serde_json::Value = serde_json::from_slice(&default_output).unwrap();
    let relaxed_review: serde_json::Value = serde_json::from_slice(&relaxed_output).unwrap();
    assert!(
        relaxed_review["candidate_count"].as_u64().unwrap()
            > default_review["candidate_count"].as_u64().unwrap(),
        "lower review threshold should include additional near misses"
    );
}

#[test]
fn adaptive_detector_writes_boundary_candidates_as_json_to_stdout() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("boundary-review.mp4");
    write_review_candidate_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    let output = cmd
        .arg("-i")
        .arg(&video)
        .args(["-m", "1"])
        .args([
            "detect-adaptive",
            "--threshold",
            "3",
            "--min-content-val",
            "20",
            "--frame-window",
            "1",
            "list-boundaries",
            "--format",
            "json",
            "--no-output-file",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let review: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(review["detector"], "adaptive");
    assert_eq!(review["score_metric"], "adaptive_ratio");
    assert!(review["candidate_count"].as_u64().unwrap() >= 1);
}

#[test]
fn threshold_detector_rejects_boundary_review_output() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("threshold-final-fade.mp4");
    write_threshold_final_fade_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.arg("-i")
        .arg(&video)
        .args([
            "detect-threshold",
            "--threshold",
            "12",
            "list-boundaries",
            "--no-output-file",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "boundary review is not supported for detect-threshold",
        ));
}

#[test]
fn cli_reports_missing_input_video() {
    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();

    cmd.args([
        "-i",
        "missing.mp4",
        "detect-content",
        "--threshold",
        "20",
        "list-scenes",
        "--no-output-file",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("failed to open input video"));
}

#[test]
fn content_detector_writes_scene_list_to_global_output_directory() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let output_dir = temp.path().join("out");
    write_two_color_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args(["detect-content", "--threshold", "20", "list-scenes"])
        .assert()
        .success()
        .stdout(predicate::str::contains("scenes.csv"));

    let scenes = std::fs::read_to_string(output_dir.join("scenes.csv")).unwrap();
    assert!(scenes.contains("Scene Number,Start Frame,Start Timecode"));
    assert!(scenes.lines().count() >= 3);
    assert_single_hidden_scene_list_artifact(&output_dir);
}

#[test]
fn export_html_first_write_uses_default_filename_and_writes_hidden_artifact() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let output_dir = temp.path().join("nested").join("out");
    write_two_color_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "export-html",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("scenes.html"));

    let html = std::fs::read_to_string(output_dir.join("scenes.html")).unwrap();
    assert!(html.contains("<title>Scene List</title>"));
    assert!(html.contains("<td>00:00:00.000</td>"));

    assert_single_hidden_scene_list_artifact(&output_dir);
}

#[test]
fn list_scenes_reuses_valid_scene_list_output() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let output_dir = temp.path().join("out");
    write_two_color_video(&video);

    let mut first = Command::cargo_bin("scenedetect-rs").unwrap();
    first
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
        ])
        .assert()
        .success();

    let mut second = Command::cargo_bin("scenedetect-rs").unwrap();
    second
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("scenes.csv"))
        .stderr(predicate::str::contains("reusing Scene List output"));
}

#[test]
fn list_scenes_requires_explicit_scene_list_artifact_match_before_reusing_output() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let output_dir = temp.path().join("out");
    let explicit_artifact = temp.path().join("explicit-scene-list.json");
    write_two_color_video(&video);

    let mut first = Command::cargo_bin("scenedetect-rs").unwrap();
    first
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
        ])
        .assert()
        .success();

    let mut second = Command::cargo_bin("scenedetect-rs").unwrap();
    let output = second
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .arg("--scene-list-artifact")
        .arg(&explicit_artifact)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
        ])
        .output()
        .unwrap();
    assert_success_without_reusable_output(output);
    assert!(explicit_artifact.exists());
}

#[test]
fn list_scenes_quiet_suppresses_reusable_output_messages() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let output_dir = temp.path().join("out");
    write_two_color_video(&video);

    let mut first = Command::cargo_bin("scenedetect-rs").unwrap();
    first
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
        ])
        .assert()
        .success();

    let mut second = Command::cargo_bin("scenedetect-rs").unwrap();
    second
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
            "--quiet",
        ])
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty());
}

#[test]
fn export_html_reuses_scene_list_artifact_from_prior_list_scenes() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let output_dir = temp.path().join("out");
    write_two_color_video(&video);

    let mut list_scenes = Command::cargo_bin("scenedetect-rs").unwrap();
    list_scenes
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
        ])
        .assert()
        .success();

    let mut export_html = Command::cargo_bin("scenedetect-rs").unwrap();
    export_html
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "export-html",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("scenes.html"))
        .stderr(predicate::str::contains("reusing Scene List Artifact"));

    let html = std::fs::read_to_string(output_dir.join("scenes.html")).unwrap();
    assert!(html.contains("<title>Scene List</title>"));
    assert!(html.contains("<td>1</td>"));
    assert!(html.contains("<td>00:00:00.000</td>"));
    assert!(html.contains("<td>00:00:00.300</td>"));
}

#[test]
fn export_html_reuses_valid_scene_list_output() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let output_dir = temp.path().join("out");
    write_two_color_video(&video);

    let mut first = Command::cargo_bin("scenedetect-rs").unwrap();
    first
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "export-html",
        ])
        .assert()
        .success();

    let mut second = Command::cargo_bin("scenedetect-rs").unwrap();
    second
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "export-html",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("scenes.html"))
        .stderr(predicate::str::contains("reusing Scene List output"));
}

#[test]
fn export_html_force_bypasses_reusable_output() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let output_dir = temp.path().join("out");
    write_two_color_video(&video);

    let mut first = Command::cargo_bin("scenedetect-rs").unwrap();
    first
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "export-html",
        ])
        .assert()
        .success();

    let mut second = Command::cargo_bin("scenedetect-rs").unwrap();
    let output = second
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .arg("--force")
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "export-html",
        ])
        .output()
        .unwrap();
    assert_success_without_reusable_output(output);
}

#[test]
fn export_html_stats_bypasses_reusable_output_and_regenerates_detection_stats() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let output_dir = temp.path().join("out");
    let stats = temp.path().join("stats.csv");
    write_two_color_video(&video);

    let mut first = Command::cargo_bin("scenedetect-rs").unwrap();
    first
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "export-html",
        ])
        .assert()
        .success();

    let mut second = Command::cargo_bin("scenedetect-rs").unwrap();
    let output = second
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .arg("--stats")
        .arg(&stats)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "export-html",
        ])
        .output()
        .unwrap();
    assert_success_without_reusable_output(output);

    let stats = std::fs::read_to_string(stats).unwrap();
    assert!(stats.contains("Frame Number,content_val"));
}

#[test]
fn export_html_invalid_render_manifests_do_not_reuse_output() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let cases = [
        "missing manifest",
        "malformed manifest",
        "stale output fingerprint",
        "mismatched request key",
    ];

    for case in cases {
        let temp = tempfile::tempdir().unwrap();
        let video = temp.path().join("scene-change.mp4");
        let output_dir = temp.path().join("out");
        let output_path = output_dir.join("scenes.html");
        write_two_color_video(&video);

        let mut first = Command::cargo_bin("scenedetect-rs").unwrap();
        first
            .arg("-i")
            .arg(&video)
            .arg("--output")
            .arg(&output_dir)
            .args([
                "-m",
                "1",
                "detect-content",
                "--threshold",
                "20",
                "export-html",
            ])
            .assert()
            .success();

        let manifest = export_html_render_manifest(&output_dir);
        match case {
            "missing manifest" => {
                std::fs::remove_file(&manifest).unwrap();
                std::fs::write(&output_path, "stale html").unwrap();
            }
            "malformed manifest" => {
                std::fs::write(&manifest, "{not-json").unwrap();
            }
            "stale output fingerprint" => {
                std::fs::write(&output_path, "stale html").unwrap();
            }
            "mismatched request key" => {
                let mut manifest_json: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
                manifest_json["scene_list_request_key"] =
                    serde_json::Value::String("mismatched-scene-list-request".to_owned());
                std::fs::write(
                    &manifest,
                    serde_json::to_string_pretty(&manifest_json).unwrap(),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }

        let mut second = Command::cargo_bin("scenedetect-rs").unwrap();
        let output = second
            .arg("-i")
            .arg(&video)
            .arg("--output")
            .arg(&output_dir)
            .args([
                "-m",
                "1",
                "detect-content",
                "--threshold",
                "20",
                "export-html",
            ])
            .output()
            .unwrap();
        assert_success_without_reusable_output(output);

        let html = std::fs::read_to_string(output_path).unwrap();
        assert!(html.contains("<title>Scene List</title>"), "{case}");
        assert!(!html.contains("stale html"), "{case}");
    }
}

#[test]
fn force_recomputes_even_when_reusable_output_exists() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let output_dir = temp.path().join("out");
    write_two_color_video(&video);

    let mut first = Command::cargo_bin("scenedetect-rs").unwrap();
    first
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
        ])
        .assert()
        .success();

    let mut second = Command::cargo_bin("scenedetect-rs").unwrap();
    second
        .arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .arg("--force")
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
        ])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}

#[test]
fn stale_output_without_manifest_is_overwritten() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let output_dir = temp.path().join("out");
    std::fs::create_dir_all(&output_dir).unwrap();
    std::fs::write(output_dir.join("scenes.csv"), "stale scene list").unwrap();
    write_two_color_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args(["detect-content", "--threshold", "20", "list-scenes"])
        .assert()
        .success();

    let scenes = std::fs::read_to_string(output_dir.join("scenes.csv")).unwrap();
    assert!(scenes.contains("Scene Number,Start Frame,Start Timecode"));
    assert!(!scenes.contains("stale scene list"));
}

#[test]
fn list_scenes_invalid_render_manifests_do_not_reuse_output() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let cases = [
        "missing manifest",
        "malformed manifest",
        "stale output fingerprint",
        "mismatched request key",
    ];

    for case in cases {
        let temp = tempfile::tempdir().unwrap();
        let video = temp.path().join("scene-change.mp4");
        let output_dir = temp.path().join("out");
        let output_path = output_dir.join("scenes.csv");
        write_two_color_video(&video);

        let mut first = Command::cargo_bin("scenedetect-rs").unwrap();
        first
            .arg("-i")
            .arg(&video)
            .arg("--output")
            .arg(&output_dir)
            .args([
                "-m",
                "1",
                "detect-content",
                "--threshold",
                "20",
                "list-scenes",
            ])
            .assert()
            .success();

        let manifest = list_scenes_render_manifest(&output_dir);
        match case {
            "missing manifest" => {
                std::fs::remove_file(&manifest).unwrap();
                std::fs::write(&output_path, "stale scene list").unwrap();
            }
            "malformed manifest" => {
                std::fs::write(&manifest, "{not-json").unwrap();
            }
            "stale output fingerprint" => {
                std::fs::write(&output_path, "stale scene list").unwrap();
            }
            "mismatched request key" => {
                let mut manifest_json: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(&manifest).unwrap()).unwrap();
                manifest_json["scene_list_request_key"] =
                    serde_json::Value::String("mismatched-scene-list-request".to_owned());
                std::fs::write(
                    &manifest,
                    serde_json::to_string_pretty(&manifest_json).unwrap(),
                )
                .unwrap();
            }
            _ => unreachable!(),
        }

        let mut second = Command::cargo_bin("scenedetect-rs").unwrap();
        let output = second
            .arg("-i")
            .arg(&video)
            .arg("--output")
            .arg(&output_dir)
            .args([
                "-m",
                "1",
                "detect-content",
                "--threshold",
                "20",
                "list-scenes",
            ])
            .output()
            .unwrap();
        assert_success_without_reusable_output(output);

        let scenes = std::fs::read_to_string(output_path).unwrap();
        assert!(
            scenes.contains("Scene Number,Start Frame,Start Timecode"),
            "{case}"
        );
        assert!(!scenes.contains("stale scene list"), "{case}");
    }
}

#[test]
fn no_output_file_does_not_create_hidden_artifact_by_default() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    write_two_color_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.current_dir(temp.path())
        .arg("-i")
        .arg(&video)
        .args([
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
            "--no-output-file",
        ])
        .assert()
        .success();

    assert!(!temp.path().join(".scenedetect-rs").exists());
}

#[test]
fn explicit_scene_list_artifact_is_used_with_no_output_file() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let artifact = temp.path().join("scene-list-artifact.json");
    write_two_color_video(&video);

    let mut first = Command::cargo_bin("scenedetect-rs").unwrap();
    first
        .current_dir(temp.path())
        .arg("-i")
        .arg(&video)
        .arg("--scene-list-artifact")
        .arg(&artifact)
        .args([
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
            "--no-output-file",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Scene Number,Start Frame,Start Timecode",
        ));

    assert!(artifact.exists());

    let mut second = Command::cargo_bin("scenedetect-rs").unwrap();
    second
        .current_dir(temp.path())
        .arg("-i")
        .arg(&video)
        .arg("--scene-list-artifact")
        .arg(&artifact)
        .args([
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
            "--no-output-file",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Scene Number,Start Frame,Start Timecode",
        ))
        .stderr(predicate::str::contains("reusing Scene List Artifact"));

    assert!(!temp.path().join(".scenedetect-rs").exists());
}

#[test]
fn export_html_no_output_file_reuses_explicit_scene_list_artifact() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let artifact = temp.path().join("scene-list-artifact.json");
    write_two_color_video(&video);

    let mut create_artifact = Command::cargo_bin("scenedetect-rs").unwrap();
    create_artifact
        .current_dir(temp.path())
        .arg("-i")
        .arg(&video)
        .arg("--scene-list-artifact")
        .arg(&artifact)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
            "--no-output-file",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Scene Number,Start Frame,Start Timecode",
        ));

    assert!(artifact.exists());

    let mut export_html = Command::cargo_bin("scenedetect-rs").unwrap();
    export_html
        .current_dir(temp.path())
        .arg("-i")
        .arg(&video)
        .arg("--scene-list-artifact")
        .arg(&artifact)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "export-html",
            "--no-output-file",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("<title>Scene List</title>"))
        .stdout(predicate::str::contains("<td>00:00:00.000</td>"))
        .stderr(predicate::str::contains("reusing Scene List Artifact"));

    assert!(!temp.path().join(".scenedetect-rs").exists());
}

#[test]
fn detector_min_scene_len_overrides_global_min_scene_len() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("close-scene-changes.mp4");
    write_three_color_video(&video);

    let mut global_only = Command::cargo_bin("scenedetect-rs").unwrap();
    let global_only_output = global_only
        .arg("-i")
        .arg(&video)
        .args([
            "--min-scene-len",
            "100",
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
            "--no-output-file",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    let detector_override_output = cmd
        .arg("-i")
        .arg(&video)
        .args([
            "--min-scene-len",
            "100",
            "detect-content",
            "--threshold",
            "20",
            "--min-scene-len",
            "1",
            "list-scenes",
            "--no-output-file",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let global_only_scenes = String::from_utf8(global_only_output).unwrap();
    let detector_override_scenes = String::from_utf8(detector_override_output).unwrap();
    assert!(
        detector_override_scenes.lines().count() > global_only_scenes.lines().count(),
        "detector-level min scene length should allow more scene boundaries than the global value"
    );
}

#[test]
fn threshold_detector_adds_final_fade_out_scene_by_default() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("threshold-final-fade.mp4");
    write_threshold_final_fade_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    let output = cmd
        .arg("-i")
        .arg(&video)
        .args([
            "detect-threshold",
            "--threshold",
            "12",
            "--min-scene-len",
            "1",
            "list-scenes",
            "--no-output-file",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let scenes = String::from_utf8(output).unwrap();
    assert!(
        scenes.contains("1,1,00:00:00.000,0.000000,2,00:00:00.200"),
        "expected a final fade-out scene boundary at frame 2, got:\n{scenes}"
    );
}

#[test]
fn list_scenes_no_output_file_writes_scene_list_to_stdout() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    write_two_color_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    let output = cmd
        .current_dir(temp.path())
        .arg("-i")
        .arg(&video)
        .args([
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
            "--no-output-file",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Scene Number,Start Frame,Start Timecode",
        ))
        .get_output()
        .stdout
        .clone();

    let scenes = String::from_utf8(output).unwrap();
    assert!(scenes.lines().count() >= 3);
    assert!(!temp.path().join("scenes.csv").exists());
}

#[test]
fn json_scene_list_output_writes_ordered_scene_spans_to_file() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let output_dir = temp.path().join("out");
    write_two_color_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args(["-m", "1"])
        .args([
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
            "--format",
            "json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("scenes.json"));

    let scenes = std::fs::read_to_string(output_dir.join("scenes.json")).unwrap();
    let scenes: serde_json::Value = serde_json::from_str(&scenes).unwrap();
    assert_eq!(scenes["frame_rate"], 10.0);
    assert_eq!(scenes["scenes"][0]["scene_number"], 1);
    assert_eq!(scenes["scenes"][0]["start_frame"], 1);
    assert_eq!(scenes["scenes"][1]["scene_number"], 2);
    assert_eq!(scenes["scenes"][1]["start_frame"], 4);
    assert!(!output_dir.join("scenes.csv").exists());
    assert_single_hidden_scene_list_artifact(&output_dir);
}

#[test]
fn json_scene_list_no_output_file_writes_scene_list_to_stdout() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    write_two_color_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    let output = cmd
        .current_dir(temp.path())
        .arg("-i")
        .arg(&video)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
            "--format",
            "json",
            "--no-output-file",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let scenes: serde_json::Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(scenes["frame_rate"], 10.0);
    assert_eq!(scenes["scene_count"], 2);
    assert_eq!(scenes["scenes"][0]["scene_number"], 1);
    assert_eq!(scenes["scenes"][1]["scene_number"], 2);
    assert!(!temp.path().join("scenes.json").exists());
    assert!(!temp.path().join(".scenedetect-rs").exists());
}

#[test]
fn ndjson_scene_events_no_output_file_writes_one_scene_span_per_stdout_line() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    write_two_color_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    let output = cmd
        .current_dir(temp.path())
        .arg("-i")
        .arg(&video)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
            "--format",
            "ndjson",
            "--no-output-file",
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let events: Vec<serde_json::Value> = String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["event"], "scene");
    assert_eq!(events[0]["scene_number"], 1);
    assert_eq!(events[0]["start_frame"], 1);
    assert_eq!(events[1]["event"], "scene");
    assert_eq!(events[1]["scene_number"], 2);
    assert_eq!(events[1]["start_frame"], 4);
    assert!(!temp.path().join("scenes.ndjson").exists());
    assert!(!temp.path().join(".scenedetect-rs").exists());
}

#[test]
fn ndjson_scene_events_output_writes_one_scene_span_per_file_line() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let output_dir = temp.path().join("out");
    write_two_color_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.arg("-i")
        .arg(&video)
        .arg("--output")
        .arg(&output_dir)
        .args(["-m", "1"])
        .args([
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
            "--format",
            "ndjson",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("scenes.ndjson"));

    let events = std::fs::read_to_string(output_dir.join("scenes.ndjson")).unwrap();
    let events: Vec<serde_json::Value> = events
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["event"], "scene");
    assert_eq!(events[0]["scene_number"], 1);
    assert_eq!(events[1]["event"], "scene");
    assert_eq!(events[1]["scene_number"], 2);
    assert!(!output_dir.join("scenes.csv").exists());
    assert!(!output_dir.join("scenes.json").exists());
    assert_single_hidden_scene_list_artifact(&output_dir);
}

#[test]
fn invalid_pyscenedetect_style_global_option_order_fails_with_clap_error() {
    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();

    cmd.args(["detect-content", "-i", "video.mp4", "list-scenes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument '-i' found"))
        .stderr(predicate::str::contains(
            "Usage: scenedetect-rs detect-content",
        ));
}

#[test]
fn cli_writes_scene_list_and_stats_for_generated_video() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let output_dir = temp.path().join("out");
    let stats = temp.path().join("stats.csv");
    write_two_color_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.arg("-i")
        .arg(&video)
        .arg("--stats")
        .arg(&stats)
        .args([
            "-m",
            "1",
            "detect-content",
            "--threshold",
            "20",
            "list-scenes",
        ])
        .arg("--output")
        .arg(&output_dir)
        .assert()
        .success();

    let scenes = std::fs::read_to_string(output_dir.join("scenes.csv")).unwrap();
    let stats = std::fs::read_to_string(stats).unwrap();
    assert!(scenes.contains("Scene Number,Start Frame,Start Timecode"));
    assert!(scenes.lines().count() >= 3);
    assert!(stats.contains("Frame Number,content_val"));
}

#[test]
fn hist_detector_writes_scene_list_and_stats_for_generated_video() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("hist-scene-change.mp4");
    let output_dir = temp.path().join("out");
    let stats = temp.path().join("stats.csv");
    write_two_color_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.arg("-i")
        .arg(&video)
        .arg("--stats")
        .arg(&stats)
        .args([
            "-m",
            "1",
            "detect-hist",
            "--threshold",
            "0.05",
            "--bins",
            "256",
            "list-scenes",
        ])
        .arg("--output")
        .arg(&output_dir)
        .assert()
        .success();

    let scenes = std::fs::read_to_string(output_dir.join("scenes.csv")).unwrap();
    let stats = std::fs::read_to_string(stats).unwrap();
    assert!(scenes.contains("Scene Number,Start Frame,Start Timecode"));
    assert!(scenes.lines().count() >= 3);
    assert!(stats.contains("Frame Number,hist_diff [bins=256]"));
}

#[test]
fn hash_detector_writes_scene_list_and_stats_for_generated_video() {
    if !ffmpeg_available() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("hash-scene-change.mp4");
    let output_dir = temp.path().join("out");
    let stats = temp.path().join("stats.csv");
    write_hash_pattern_video(&video);

    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();
    cmd.arg("-i")
        .arg(&video)
        .arg("--stats")
        .arg(&stats)
        .args([
            "-m",
            "1",
            "detect-hash",
            "--threshold",
            "0.395",
            "--size",
            "16",
            "--lowpass",
            "2",
            "list-scenes",
        ])
        .arg("--output")
        .arg(&output_dir)
        .assert()
        .success();

    let scenes = std::fs::read_to_string(output_dir.join("scenes.csv")).unwrap();
    let stats = std::fs::read_to_string(stats).unwrap();
    assert!(scenes.contains("Scene Number,Start Frame,Start Timecode"));
    assert!(scenes.lines().count() >= 3);
    assert!(stats.contains("Frame Number,hash_dist [size=16 lowpass=2]"));
}

#[test]
fn hist_and_hash_detector_options_validate_pyscenedetect_ranges() {
    let mut hist = Command::cargo_bin("scenedetect-rs").unwrap();
    hist.args([
        "-i",
        "video.mp4",
        "detect-hist",
        "--threshold",
        "1.5",
        "list-scenes",
        "--no-output-file",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("invalid value"));

    let mut hash = Command::cargo_bin("scenedetect-rs").unwrap();
    hash.args([
        "-i",
        "video.mp4",
        "detect-hash",
        "--size",
        "0",
        "list-scenes",
        "--no-output-file",
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("invalid value"));
}
