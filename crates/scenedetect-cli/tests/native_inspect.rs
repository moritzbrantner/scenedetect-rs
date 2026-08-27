use std::path::Path;
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg").arg("-version").output().is_ok()
}

fn write_two_color_video(video: &Path) {
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

#[test]
fn native_help_exposes_inspect_and_all_detectors() {
    let help = Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .arg("--help")
        .output()
        .unwrap();
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("inspect"));

    let detect_help = Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .args(["detect", "--help"])
        .output()
        .unwrap();
    assert!(detect_help.status.success());
    let detect_help = String::from_utf8(detect_help.stdout).unwrap();
    for detector in ["content", "adaptive", "threshold", "hist", "hash"] {
        assert!(detect_help.contains(detector), "missing {detector} in detect help");
    }
}

#[test]
fn inspect_reads_video_or_stats_without_decoding() {
    if !ffmpeg_available() {
        eprintln!("skipping inspect integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("inspect.mp4");
    write_two_color_video(&video);

    Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .args(["detect", "content"])
        .arg("-i")
        .arg(&video)
        .args(["--threshold", "20", "--min-scene-len", "1", "--progress", "never"])
        .assert()
        .success();

    let json = Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .arg("inspect")
        .arg("-i")
        .arg(&video)
        .arg("--json")
        .output()
        .unwrap();
    assert!(json.status.success());
    let json: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(json["detector"]["name"], "content");
    assert_eq!(json["input"]["frame_rate"], 10.0);
    assert_eq!(json["input"]["total_frames"], 6);
    assert_eq!(json["scene_boundary_count"], 1);
    assert!(json["detection_stats_path"]
        .as_str()
        .unwrap()
        .ends_with("inspect.scenedetect.json"));

    let stats = temp.path().join("inspect.scenedetect.json");
    std::fs::remove_file(&video).unwrap();
    Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .arg("inspect")
        .arg("-i")
        .arg(&stats)
        .assert()
        .success()
        .stdout(predicate::str::contains("Detector: content"))
        .stdout(predicate::str::contains("Frames: 6"))
        .stdout(predicate::str::contains("Scene boundaries: 1"));
}

#[test]
fn native_artifact_errors_name_artifact_and_recovery_command() {
    let temp = tempfile::tempdir().unwrap();
    let missing_video = temp.path().join("missing.mp4");
    let missing_stats = temp.path().join("missing.scenedetect.json");

    Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .arg("inspect")
        .arg("-i")
        .arg(&missing_video)
        .assert()
        .failure()
        .stderr(predicate::str::contains(missing_stats.display().to_string()))
        .stderr(predicate::str::contains("Recovery:"))
        .stderr(predicate::str::contains("scenedetect-rs detect content"));

    let broken_video = temp.path().join("broken.mp4");
    let broken_stats = temp.path().join("broken.scenedetect.json");
    std::fs::write(&broken_stats, "not json").unwrap();
    Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .arg("inspect")
        .arg("-i")
        .arg(&broken_video)
        .assert()
        .failure()
        .stderr(predicate::str::contains(broken_stats.display().to_string()))
        .stderr(predicate::str::contains("malformed"))
        .stderr(predicate::str::contains("Recovery:"));
}

#[test]
fn changed_input_marks_detection_stats_stale_before_rendering() {
    if !ffmpeg_available() {
        eprintln!("skipping stale-input integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("stale.mp4");
    write_two_color_video(&video);

    Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .args(["detect", "content"])
        .arg("-i")
        .arg(&video)
        .args(["--threshold", "20", "--min-scene-len", "1", "--progress", "never"])
        .assert()
        .success();

    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(&video)
        .unwrap()
        .write_all(b"changed")
        .unwrap();

    let stats = temp.path().join("stale.scenedetect.json");
    Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .args(["render", "scenes"])
        .arg("-i")
        .arg(&video)
        .assert()
        .failure()
        .stderr(predicate::str::contains(stats.display().to_string()))
        .stderr(predicate::str::contains("stale"))
        .stderr(predicate::str::contains("Recovery:"))
        .stderr(predicate::str::contains("scenedetect-rs detect content"));
}
