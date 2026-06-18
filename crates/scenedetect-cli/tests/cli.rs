use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

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
fn cli_writes_scene_list_and_stats_for_generated_video() {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("skipping CLI integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("scene-change.mp4");
    let output_dir = temp.path().join("out");
    let stats = temp.path().join("stats.csv");

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
        .arg(&video)
        .status()
        .unwrap()
        .success());

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
