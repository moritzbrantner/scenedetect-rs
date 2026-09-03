use std::path::PathBuf;
use std::process::{Command, Stdio};

use scenedetect_core::{FrameRate, FrameSource, SceneDetectError};
use scenedetect_ffmpeg::{FfmpegBinaries, FfmpegFrameSource};

fn missing_binary(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("scenedetect-rs-missing-{name}"))
}

fn ffmpeg_available() -> bool {
    Command::new("ffmpeg").arg("-version").output().is_ok()
        && Command::new("ffprobe").arg("-version").output().is_ok()
}

fn generated_fixture(name: &str) -> Option<PathBuf> {
    if !ffmpeg_available() {
        eprintln!("skipping ffmpeg integration test because ffmpeg or ffprobe is unavailable");
        return None;
    }

    let repo_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .unwrap()
        .to_path_buf();
    let status = Command::new(repo_root.join("scripts/generate-fixtures.sh"))
        .current_dir(&repo_root)
        .stdout(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());

    Some(repo_root.join("tests/fixtures/generated").join(name))
}

fn generated_vfr_fixture() -> Option<(tempfile::TempDir, PathBuf)> {
    if !ffmpeg_available() {
        eprintln!("skipping VFR timing test because ffmpeg or ffprobe is unavailable");
        return None;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("vfr.mkv");
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "testsrc=size=16x16:rate=10:duration=0.4",
            "-vf",
            "setpts=N*N/(10*TB)",
            "-fps_mode",
            "vfr",
            "-c:v",
            "ffv1",
        ])
        .arg(&video)
        .status()
        .unwrap();
    assert!(status.success());
    Some((temp, video))
}

#[test]
fn ffmpeg_frame_source_reports_missing_ffprobe_as_actionable_error() {
    let result = FfmpegFrameSource::open_with_binaries(
        "video.mp4",
        None,
        FfmpegBinaries {
            ffprobe: missing_binary("ffprobe"),
            ..FfmpegBinaries::default()
        },
    );

    let Err(SceneDetectError::FrameSource(message)) = result else {
        panic!("expected frame source error");
    };
    assert!(message.contains("ffprobe"));
    assert!(message.contains("not found"));
    assert!(message.contains("Install FFmpeg"));
}

#[test]
fn ffmpeg_frame_source_reports_missing_ffmpeg_as_actionable_error() {
    let Some(video) = generated_fixture("content-hard-cut.mkv") else {
        return;
    };

    let result = FfmpegFrameSource::open_with_binaries(
        &video,
        None,
        FfmpegBinaries {
            ffmpeg: missing_binary("ffmpeg"),
            ..FfmpegBinaries::default()
        },
    );

    let Err(SceneDetectError::FrameSource(message)) = result else {
        panic!("expected frame source error");
    };
    assert!(message.contains("ffmpeg"));
    assert!(message.contains("not found"));
    assert!(message.contains("Install FFmpeg"));
}

#[test]
fn ffmpeg_frame_source_rejects_non_video_input_before_decoding() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("not-video.txt");
    std::fs::write(&input, "not a video").unwrap();

    let result = FfmpegFrameSource::open(&input, None);

    let Err(SceneDetectError::FrameSource(message)) = result else {
        panic!("expected frame source error");
    };
    assert!(message.contains("not-video.txt"));
    assert!(message.to_lowercase().contains("invalid"));
}

#[test]
fn ffmpeg_frame_source_uses_frame_rate_override_for_generated_fixture() {
    let Some(video) = generated_fixture("content-hard-cut.mkv") else {
        return;
    };

    let mut source = FfmpegFrameSource::open(&video, Some(FrameRate(24.0))).unwrap();
    assert_eq!(source.metadata().width, 64);
    assert_eq!(source.metadata().height, 64);
    assert_eq!(source.frame_rate(), FrameRate(24.0));
    assert!(source.next_frame().unwrap().is_some());
}

#[test]
fn ffmpeg_frame_source_decodes_generated_hard_cut_and_fade_fixtures() {
    for fixture in ["content-hard-cut.mkv", "threshold-fade-return.mkv"] {
        let Some(video) = generated_fixture(fixture) else {
            return;
        };

        let mut source = FfmpegFrameSource::open(&video, None).unwrap();
        assert_eq!(source.metadata().width, 64);
        assert_eq!(source.metadata().height, 64);
        assert_eq!(source.frame_rate(), FrameRate(10.0));

        let mut frames = 0;
        while source.next_frame().unwrap().is_some() {
            frames += 1;
        }
        assert_eq!(frames, 10, "{fixture} should decode all generated frames");
    }
}

#[test]
fn ffmpeg_frame_source_streams_non_uniform_vfr_presentation_times() {
    let Some((_temp, video)) = generated_vfr_fixture() else {
        return;
    };

    let mut source = FfmpegFrameSource::open(&video, None).unwrap();
    let mut presentation_seconds = Vec::new();
    while let Some(frame) = source.next_frame_with_timing().unwrap() {
        let presentation = frame
            .timing
            .presentation_time
            .expect("ffprobe should provide a presentation timestamp");
        presentation_seconds.push(presentation.seconds());
    }

    assert_eq!(presentation_seconds.len(), 4);
    let expected = [0.0, 0.1, 0.4, 0.9];
    for (actual, expected) in presentation_seconds.iter().zip(expected) {
        assert!(
            (actual - expected).abs() < 0.001,
            "expected VFR presentation time {expected}, got {actual}"
        );
    }

    let deltas: Vec<_> = presentation_seconds
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect();
    assert!(deltas[2] > deltas[0] * 4.0, "timing must not collapse to CFR");
}
