use std::process::Command;

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
        .success();

    let scenes = std::fs::read_to_string(output_dir.join("scenes.csv")).unwrap();
    assert!(scenes.contains("Scene Number,Start Frame,Start Timecode"));
    assert!(scenes.lines().count() >= 3);
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
fn invalid_pyscenedetect_style_global_option_order_fails_with_clap_error() {
    let mut cmd = Command::cargo_bin("scenedetect-rs").unwrap();

    cmd.args(["detect-content", "-i", "video.mp4", "list-scenes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unexpected argument '-i' found"))
        .stderr(predicate::str::contains(
            "Usage: scenedetect-rs --input <INPUT> detect-content",
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
