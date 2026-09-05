use scenedetect_core::{
    detect_frames, AdaptiveDetectorConfig, ContentDetectorConfig, DetectionOptions,
    DetectionSession, DetectorConfig, Frame, FrameIndex, FrameRate, HashDetectorConfig,
    HistogramDetectorConfig, ThresholdDetectorConfig,
};

fn frames(colors: &[[u8; 3]]) -> Vec<Frame> {
    colors
        .iter()
        .enumerate()
        .map(|(index, color)| Frame::solid(index as u64, 2, 2, *color))
        .collect()
}

fn structural_pattern_frame(index: u64) -> Frame {
    let width = 64;
    let height = 64;
    let mut rgb = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let value = ((x * 37 + y * 17 + x * y * 3) % 256) as u8;
            rgb.extend_from_slice(&[value, value, value]);
        }
    }
    Frame {
        index: FrameIndex(index),
        width,
        height,
        rgb,
    }
}

#[test]
fn incremental_session_matches_batch_detection_for_every_detector() {
    let cases = [
        (
            DetectorConfig::Content(ContentDetectorConfig {
                threshold: 20.0,
                ..Default::default()
            }),
            frames(&[[0, 0, 0], [0, 0, 0], [255, 255, 255], [255, 255, 255]]),
        ),
        (
            DetectorConfig::Adaptive(AdaptiveDetectorConfig {
                threshold: 3.0,
                min_content_val: 20.0,
                frame_window: 1,
                ..Default::default()
            }),
            frames(&[
                [0, 0, 0],
                [3, 3, 3],
                [6, 6, 6],
                [255, 255, 255],
                [252, 252, 252],
                [249, 249, 249],
            ]),
        ),
        (
            DetectorConfig::Threshold(ThresholdDetectorConfig {
                threshold: 12.0,
                ..Default::default()
            }),
            frames(&[
                [0, 0, 0],
                [0, 0, 0],
                [128, 128, 128],
                [255, 255, 255],
                [255, 255, 255],
            ]),
        ),
        (
            DetectorConfig::Histogram(HistogramDetectorConfig {
                threshold: 0.05,
                bins: 256,
            }),
            frames(&[[0, 0, 0], [0, 0, 0], [255, 255, 255], [255, 255, 255]]),
        ),
        (
            DetectorConfig::Hash(HashDetectorConfig {
                threshold: 0.395,
                size: 16,
                lowpass: 2,
            }),
            vec![
                Frame::solid(0, 64, 64, [0, 0, 0]),
                Frame::solid(1, 64, 64, [0, 0, 0]),
                structural_pattern_frame(2),
                structural_pattern_frame(3),
            ],
        ),
    ];

    for (detector, case_frames) in cases {
        let options = DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        };
        let batch = detect_frames(
            detector.clone(),
            FrameRate(10.0),
            &case_frames,
            options.clone(),
        )
        .expect("batch detection should succeed");

        let mut session = DetectionSession::new(detector, FrameRate(10.0), options);
        for frame in case_frames {
            session
                .push_frame(frame)
                .expect("incremental frame should be accepted");
        }
        let incremental = session.finish().expect("session should finish");

        assert_eq!(incremental, batch);
    }
}
