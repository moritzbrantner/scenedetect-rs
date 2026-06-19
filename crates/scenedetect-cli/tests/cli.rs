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
        .success();

    let scenes = std::fs::read_to_string(output_dir.join("scenes.json")).unwrap();
    let scenes: serde_json::Value = serde_json::from_str(&scenes).unwrap();
    assert_eq!(scenes["frame_rate"], 10.0);
    assert_eq!(scenes["scenes"][0]["scene_number"], 1);
    assert_eq!(scenes["scenes"][0]["start_frame"], 1);
    assert_eq!(scenes["scenes"][1]["scene_number"], 2);
    assert_eq!(scenes["scenes"][1]["start_frame"], 4);
    assert!(!output_dir.join("scenes.csv").exists());
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
        .success();

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
