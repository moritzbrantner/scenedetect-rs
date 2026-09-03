use std::cell::Cell;
use std::rc::Rc;

use scenedetect_core::{
    boundary_review_from_content_detection_stats, detect_content_stats,
    scene_list_from_content_detection_stats, BoundaryCandidateStatus, BoundaryReviewOptions,
    ContentDetectionStats, ContentDetectorConfig, DetectionOptions, Frame, FrameIndex, FrameRate,
    FrameSource, FrameTiming, FrameWithTiming, MediaTime, MinSceneLenPolicy, SceneDetectError,
    SceneSpan, TimeBase,
};

struct CountingFrameSource {
    frame_rate: FrameRate,
    frames: std::vec::IntoIter<Frame>,
    reads: Rc<Cell<usize>>,
}

impl CountingFrameSource {
    fn new(frames: Vec<Frame>, reads: Rc<Cell<usize>>) -> Self {
        Self {
            frame_rate: FrameRate(10.0),
            frames: frames.into_iter(),
            reads,
        }
    }
}

impl FrameSource for CountingFrameSource {
    fn frame_rate(&self) -> FrameRate {
        self.frame_rate
    }

    fn next_frame(&mut self) -> scenedetect_core::Result<Option<Frame>> {
        self.reads.set(self.reads.get() + 1);
        Ok(self.frames.next())
    }
}

struct TimingAwareFrameSource {
    frame_rate: FrameRate,
    frames: std::vec::IntoIter<FrameWithTiming>,
    rich_reads: Rc<Cell<usize>>,
    legacy_reads: Rc<Cell<usize>>,
}

impl FrameSource for TimingAwareFrameSource {
    fn frame_rate(&self) -> FrameRate {
        self.frame_rate
    }

    fn next_frame(&mut self) -> scenedetect_core::Result<Option<Frame>> {
        self.legacy_reads.set(self.legacy_reads.get() + 1);
        Err(SceneDetectError::FrameSource(
            "timing-aware source should not fall back to next_frame".to_owned(),
        ))
    }

    fn next_frame_with_timing(&mut self) -> scenedetect_core::Result<Option<FrameWithTiming>> {
        self.rich_reads.set(self.rich_reads.get() + 1);
        Ok(self.frames.next())
    }
}

#[test]
fn source_consumer_detects_once_and_derives_scene_outputs_from_stats() {
    let frames = vec![
        Frame::solid(0, 2, 2, [0, 0, 0]),
        Frame::solid(1, 2, 2, [255, 255, 255]),
        Frame::solid(2, 2, 2, [255, 255, 255]),
    ];
    let reads = Rc::new(Cell::new(0));
    let source = CountingFrameSource::new(frames, Rc::clone(&reads));
    let options = DetectionOptions {
        min_scene_len: 1,
        min_scene_len_policy: MinSceneLenPolicy::Suppress,
    };

    let stats = detect_content_stats(
        source,
        ContentDetectorConfig {
            threshold: 20.0,
            ..Default::default()
        },
        options,
    )
    .unwrap();

    assert_eq!(reads.get(), 4, "three frames plus one EOF read");
    assert_eq!(stats.total_frames, 3);
    assert_eq!(stats.rows.len(), 3);

    let scene_list = scene_list_from_content_detection_stats(&stats);
    assert_eq!(
        scene_list.scenes,
        vec![
            SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(1),
            },
            SceneSpan {
                start: FrameIndex(1),
                end: FrameIndex(3),
            },
        ]
    );

    let review =
        boundary_review_from_content_detection_stats(&stats, BoundaryReviewOptions::default());
    assert_eq!(review.scene_list, scene_list);
    assert!(review.candidates.iter().any(|candidate| {
        candidate.status == BoundaryCandidateStatus::Accepted && candidate.frame == FrameIndex(1)
    }));
}

#[test]
fn legacy_source_default_timing_adapter_wraps_plain_frames() {
    let reads = Rc::new(Cell::new(0));
    let mut source = CountingFrameSource::new(
        vec![Frame::solid(0, 2, 2, [1, 2, 3])],
        Rc::clone(&reads),
    );

    let frame = source
        .next_frame_with_timing()
        .unwrap()
        .expect("legacy frame should be wrapped");
    assert_eq!(frame.frame.index, FrameIndex(0));
    assert_eq!(frame.timing, FrameTiming::default());
    assert_eq!(reads.get(), 1);
}

#[test]
fn timing_aware_source_exposes_richer_frame_path_without_legacy_fallback() {
    let time_base = TimeBase::new(1, 1_000).expect("valid time base");
    let rich_reads = Rc::new(Cell::new(0));
    let legacy_reads = Rc::new(Cell::new(0));
    let mut source = TimingAwareFrameSource {
        frame_rate: FrameRate(10.0),
        frames: vec![
            FrameWithTiming {
                frame: Frame::solid(0, 2, 2, [0, 0, 0]),
                timing: FrameTiming {
                    presentation_time: Some(MediaTime::new(0, time_base)),
                    duration: Some(MediaTime::new(100, time_base)),
                },
            },
            FrameWithTiming {
                frame: Frame::solid(1, 2, 2, [255, 255, 255]),
                timing: FrameTiming {
                    presentation_time: Some(MediaTime::new(100, time_base)),
                    duration: Some(MediaTime::new(250, time_base)),
                },
            },
        ]
        .into_iter(),
        rich_reads: Rc::clone(&rich_reads),
        legacy_reads: Rc::clone(&legacy_reads),
    };

    let first = source
        .next_frame_with_timing()
        .unwrap()
        .expect("first timing-aware frame");
    let second = source
        .next_frame_with_timing()
        .unwrap()
        .expect("second timing-aware frame");
    assert!(source.next_frame_with_timing().unwrap().is_none());

    assert_eq!(first.frame.index, FrameIndex(0));
    assert_eq!(second.frame.index, FrameIndex(1));
    assert!((second.timing.presentation_time.unwrap().seconds() - 0.1).abs() < 1.0e-12);
    assert_eq!(rich_reads.get(), 3, "two frames plus one EOF read");
    assert_eq!(legacy_reads.get(), 0, "explicit rich reads must not use legacy next_frame");
}

#[test]
fn prior_content_detection_stats_shape_remains_deserializable() {
    let fixture = include_str!("fixtures/content-detection-stats-v1.json");
    let stats: ContentDetectionStats = serde_json::from_str(fixture).unwrap();

    assert_eq!(stats.frame_rate, FrameRate(10.0));
    assert_eq!(stats.total_frames, 3);
    assert_eq!(stats.detector_config.threshold, 20.0);
    assert_eq!(stats.options.min_scene_len, 1);

    let scene_list = scene_list_from_content_detection_stats(&stats);
    assert_eq!(
        scene_list.scenes,
        vec![
            SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(1),
            },
            SceneSpan {
                start: FrameIndex(1),
                end: FrameIndex(3),
            },
        ]
    );

    let review =
        boundary_review_from_content_detection_stats(&stats, BoundaryReviewOptions::default());
    assert_eq!(review.candidates.len(), 1);
    assert_eq!(
        review.candidates[0].status,
        BoundaryCandidateStatus::Accepted
    );
    assert_eq!(review.candidates[0].frame, FrameIndex(1));

    let round_trip = serde_json::to_string(&stats).unwrap();
    let restored: ContentDetectionStats = serde_json::from_str(&round_trip).unwrap();
    assert_eq!(restored, stats);
}
