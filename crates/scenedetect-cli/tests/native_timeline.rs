use std::fs;
use std::process::Command as StdCommand;

use assert_cmd::Command;
use serde_json::Value;
use tempfile::tempdir;

fn require_ffmpeg() -> bool {
    StdCommand::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn generate_vfr_hard_cuts(path: &std::path::Path) {
    let status = StdCommand::new("ffmpeg")
        .args(["-v", "error", "-y"])
        .args(["-f", "lavfi", "-i", "color=c=black:s=16x16:r=10:d=0.2"])
        .args(["-f", "lavfi", "-i", "color=c=white:s=16x16:r=10:d=0.2"])
        .args([
            "-filter_complex",
            "[0:v][1:v]concat=n=2:v=1:a=0,setpts=N*N/(10*TB)[v]",
            "-map",
            "[v]",
            "-fps_mode",
            "vfr",
            "-c:v",
            "ffv1",
        ])
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

fn media_time_seconds(value: &Value) -> f64 {
    let ticks = value["ticks"].as_i64().unwrap() as f64;
    let numerator = value["time_base"]["numerator"].as_i64().unwrap() as f64;
    let denominator = value["time_base"]["denominator"].as_i64().unwrap() as f64;
    ticks * numerator / denominator
}

#[test]
fn native_timeline_preserves_exact_vfr_endpoints_without_mutating_detection_stats() {
    if !require_ffmpeg() {
        return;
    }

    let temp = tempdir().unwrap();
    let video = temp.path().join("vfr-hard-cuts.mkv");
    generate_vfr_hard_cuts(&video);

    Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .args(["detect", "content", "-i"])
        .arg(&video)
        .args([
            "--threshold",
            "20",
            "--min-scene-len",
            "1",
            "--progress",
            "never",
            "--quiet",
        ])
        .assert()
        .success();

    let stats_path = temp.path().join("vfr-hard-cuts.scenedetect.json");
    let stats_before = fs::read(&stats_path).unwrap();

    Command::cargo_bin("scenedetect-rs")
        .unwrap()
        .args(["render", "timeline", "-i"])
        .arg(&video)
        .assert()
        .success();

    let timeline_path = temp.path().join("vfr-hard-cuts.timeline.json");
    let timeline: Value = serde_json::from_slice(&fs::read(&timeline_path).unwrap()).unwrap();

    assert_eq!(timeline["schema_version"], 1);
    assert_eq!(timeline["kind"], "scene_timeline");
    assert_eq!(timeline["source_detection_stats_schema_version"], 1);
    assert_eq!(timeline["detector"]["name"], "content");
    assert_eq!(timeline["scenes"].as_array().unwrap().len(), 2);

    let first = &timeline["scenes"][0];
    let second = &timeline["scenes"][1];
    assert_eq!(first["scene_number"], 1);
    assert_eq!(first["start_frame"], 0);
    assert_eq!(first["end_frame"], 2);
    assert_eq!(second["scene_number"], 2);
    assert_eq!(second["start_frame"], 2);
    assert_eq!(second["end_frame"], 4);

    assert!((media_time_seconds(&first["start_time"]) - 0.0).abs() < 0.01);
    assert!((media_time_seconds(&first["end_time"]) - 0.4).abs() < 0.01);
    assert!((media_time_seconds(&second["start_time"]) - 0.4).abs() < 0.01);
    assert!((media_time_seconds(&second["end_time"]) - 1.0).abs() < 0.01);

    assert_eq!(
        fs::read(&stats_path).unwrap(),
        stats_before,
        "rendering a Scene Timeline must not rewrite canonical Detection Stats"
    );
}
