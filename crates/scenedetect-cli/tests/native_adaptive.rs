use std::path::PathBuf;
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
