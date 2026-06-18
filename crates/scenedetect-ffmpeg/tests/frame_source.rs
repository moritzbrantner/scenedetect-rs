use std::process::Command;

use scenedetect_core::FrameSource;
use scenedetect_ffmpeg::FfmpegFrameSource;

#[test]
fn ffmpeg_frame_source_decodes_generated_video_frames() {
    if Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("skipping ffmpeg integration test because ffmpeg is unavailable");
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("solid.mp4");
    let status = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=16x16:d=0.2:r=10",
            "-pix_fmt",
            "yuv420p",
        ])
        .arg(&video)
        .status()
        .unwrap();
    assert!(status.success());

    let mut source = FfmpegFrameSource::open(&video, None).unwrap();
    assert_eq!(source.metadata().width, 16);
    assert_eq!(source.metadata().height, 16);
    assert!(source.next_frame().unwrap().is_some());
}
