use std::cell::Cell;
use std::rc::Rc;

use scenedetect_core::{
    detect_scenes, AdaptiveDetectorConfig, ContentDetectorConfig, DetectionOptions, DetectorConfig,
    Frame, FrameRate, FrameSource, FrameWithTiming, HashDetectorConfig, HistogramDetectorConfig,
    SceneDetectError, ThresholdDetectorConfig,
};

struct PlainOnlySource {
    frames: std::vec::IntoIter<Frame>,
    plain_reads: Rc<Cell<usize>>,
    rich_reads: Rc<Cell<usize>>,
}

impl PlainOnlySource {
    fn new(plain_reads: Rc<Cell<usize>>, rich_reads: Rc<Cell<usize>>) -> Self {
        Self {
            frames: vec![
                Frame::solid(0, 32, 32, [0, 0, 0]),
                Frame::solid(1, 32, 32, [255, 255, 255]),
                Frame::solid(2, 32, 32, [255, 255, 255]),
                Frame::solid(3, 32, 32, [0, 0, 0]),
            ]
            .into_iter(),
            plain_reads,
            rich_reads,
        }
    }
}

impl FrameSource for PlainOnlySource {
    fn frame_rate(&self) -> FrameRate {
        FrameRate(10.0)
    }

    fn next_frame(&mut self) -> scenedetect_core::Result<Option<Frame>> {
        self.plain_reads.set(self.plain_reads.get() + 1);
        Ok(self.frames.next())
    }

    fn next_frame_with_timing(&mut self) -> scenedetect_core::Result<Option<FrameWithTiming>> {
        self.rich_reads.set(self.rich_reads.get() + 1);
        Err(SceneDetectError::FrameSource(
            "frame-index detector unexpectedly requested rich timing".to_owned(),
        ))
    }
}

#[test]
fn canonical_detectors_consume_only_plain_frames() {
    let detectors = [
        DetectorConfig::Content(ContentDetectorConfig::default()),
        DetectorConfig::Adaptive(AdaptiveDetectorConfig::default()),
        DetectorConfig::Threshold(ThresholdDetectorConfig::default()),
        DetectorConfig::Histogram(HistogramDetectorConfig::default()),
        DetectorConfig::Hash(HashDetectorConfig::default()),
    ];

    for detector in detectors {
        let plain_reads = Rc::new(Cell::new(0));
        let rich_reads = Rc::new(Cell::new(0));
        let source = PlainOnlySource::new(Rc::clone(&plain_reads), Rc::clone(&rich_reads));

        detect_scenes(
            detector,
            source,
            DetectionOptions {
                min_scene_len: 1,
                ..DetectionOptions::default()
            },
        )
        .expect("frame-index detector should not require timing metadata");

        assert!(plain_reads.get() > 0, "detector should consume plain frames");
        assert_eq!(
            rich_reads.get(),
            0,
            "detector must not activate the timing-aware source path"
        );
    }
}
