use scenedetect_core::{
    detect_content_stats, ContentDetectorConfig, ContentWeights, DetectionOptions, Frame,
    FrameRate, FrameSource, SceneDetectError,
};

struct Frames {
    frame_rate: FrameRate,
    frames: std::vec::IntoIter<Frame>,
}

impl Frames {
    fn new(frames: Vec<Frame>) -> Self {
        Self {
            frame_rate: FrameRate(10.0),
            frames: frames.into_iter(),
        }
    }
}

impl FrameSource for Frames {
    fn frame_rate(&self) -> FrameRate {
        self.frame_rate
    }

    fn next_frame(&mut self) -> Result<Option<Frame>, SceneDetectError> {
        Ok(self.frames.next())
    }
}

fn metric(row: &scenedetect_core::RichDetectionStatsRow, name: &str) -> f64 {
    *row.metrics.get(name).expect("metric should be present")
}

#[test]
fn content_stats_use_hsv_components_for_custom_weights() {
    let frames = vec![
        Frame::solid(0, 2, 2, [255, 0, 0]),
        Frame::solid(1, 2, 2, [255, 255, 0]),
        Frame::solid(2, 2, 2, [255, 255, 255]),
    ];
    let stats = detect_content_stats(
        Frames::new(frames),
        ContentDetectorConfig {
            threshold: 10.0,
            weights: ContentWeights {
                hue: 1.0,
                saturation: 0.0,
                luminance: 0.0,
                edges: 0.0,
            },
            luma_only: false,
        },
        DetectionOptions {
            min_scene_len: 1,
            ..DetectionOptions::default()
        },
    )
    .unwrap();

    let red_to_yellow = &stats.rows[1];
    assert_eq!(metric(red_to_yellow, "delta_hue"), 30.0);
    assert_eq!(metric(red_to_yellow, "delta_saturation"), 0.0);
    assert_eq!(metric(red_to_yellow, "delta_luminance"), 0.0);
    assert_eq!(metric(red_to_yellow, "content_val"), 30.0);

    let yellow_to_white = &stats.rows[2];
    assert_eq!(metric(yellow_to_white, "delta_hue"), 30.0);
    assert_eq!(metric(yellow_to_white, "delta_saturation"), 255.0);
    assert_eq!(metric(yellow_to_white, "delta_luminance"), 0.0);
    assert_eq!(metric(yellow_to_white, "content_val"), 30.0);
}

#[test]
fn luma_only_uses_hsv_value_component() {
    let stats = detect_content_stats(
        Frames::new(vec![
            Frame::solid(0, 2, 2, [255, 0, 0]),
            Frame::solid(1, 2, 2, [255, 255, 0]),
        ]),
        ContentDetectorConfig {
            threshold: 1.0,
            weights: ContentWeights::default(),
            luma_only: true,
        },
        DetectionOptions {
            min_scene_len: 1,
            ..DetectionOptions::default()
        },
    )
    .unwrap();

    let red_to_yellow = &stats.rows[1];
    assert_eq!(metric(red_to_yellow, "delta_luminance"), 0.0);
    assert_eq!(metric(red_to_yellow, "content_val"), 0.0);
}
