use std::path::{Path, PathBuf};
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
        assert!(std::fs::read_to_string(html)
            .unwrap()
            .contains("Scene List"));

        Command::cargo_bin("scenedetect-rs")
            .unwrap()
            .args(["render", "boundaries"])
            .arg("-i")
            .arg(video)
            .assert()
            .failure();
    }
}
