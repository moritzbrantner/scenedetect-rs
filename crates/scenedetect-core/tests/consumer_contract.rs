use std::cell::Cell;
use std::rc::Rc;

use scenedetect_core::{
    boundary_review_from_content_detection_stats, detect_content_stats,
    scene_list_from_content_detection_stats, BoundaryCandidateStatus, BoundaryReviewOptions,
    ContentDetectionStats, ContentDetectorConfig, DetectionOptions, Frame, FrameIndex, FrameRate,
    FrameSource, MinSceneLenPolicy, SceneSpan,
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
