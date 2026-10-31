use super::*;
use std::cell::Cell;
use std::collections::BTreeMap;
use std::rc::Rc;

struct VecFrameSource {
    frame_rate: FrameRate,
    frames: std::vec::IntoIter<Frame>,
}

impl VecFrameSource {
    fn new(frames: Vec<Frame>) -> Self {
        Self {
            frame_rate: FrameRate(10.0),
            frames: frames.into_iter(),
        }
    }
}

impl FrameSource for VecFrameSource {
    fn frame_rate(&self) -> FrameRate {
        self.frame_rate
    }

    fn next_frame(&mut self) -> Result<Option<Frame>> {
        Ok(self.frames.next())
    }
}

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

    fn next_frame(&mut self) -> Result<Option<Frame>> {
        self.reads.set(self.reads.get() + 1);
        Ok(self.frames.next())
    }
}

struct StreamingProbeStatsSink {
    reads: Rc<Cell<usize>>,
    eof_read_count: usize,
    saw_row_before_eof: bool,
}

impl DetectionStatsSink for StreamingProbeStatsSink {
    fn start(&mut self, _metric_names: &[String]) -> Result<()> {
        Ok(())
    }

    fn row(&mut self, _row: StatsRow) -> Result<()> {
        if self.reads.get() < self.eof_read_count {
            self.saw_row_before_eof = true;
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        Ok(())
    }
}

struct MemoryStatsSink {
    stats: DetectionStats,
}

impl Default for MemoryStatsSink {
    fn default() -> Self {
        Self {
            stats: DetectionStats {
                metric_names: Vec::new(),
                rows: Vec::new(),
            },
        }
    }
}

impl DetectionStatsSink for MemoryStatsSink {
    fn start(&mut self, metric_names: &[String]) -> Result<()> {
        self.stats.metric_names = metric_names.to_vec();
        self.stats.rows.clear();
        Ok(())
    }

    fn row(&mut self, row: StatsRow) -> Result<()> {
        self.stats.rows.push(row);
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        Ok(())
    }
}

#[test]
fn streaming_detection_emits_stats_before_frame_source_is_exhausted() {
    let frames = frames(&[[0, 0, 0], [255, 255, 255], [255, 255, 255]]);
    let eof_read_count = frames.len() + 1;
    let reads = Rc::new(Cell::new(0));
    let source = CountingFrameSource::new(frames, Rc::clone(&reads));
    let mut stats_sink = StreamingProbeStatsSink {
        reads,
        eof_read_count,
        saw_row_before_eof: false,
    };

    let scene_list = detect_scenes_streaming(
        DetectorConfig::Content(ContentDetectorConfig {
            threshold: 20.0,
            ..Default::default()
        }),
        source,
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
        &mut stats_sink,
    )
    .unwrap();

    assert_eq!(
        scene_list.scenes,
        vec![
            SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(1)
            },
            SceneSpan {
                start: FrameIndex(1),
                end: FrameIndex(3)
            }
        ]
    );
    assert!(
        stats_sink.saw_row_before_eof,
        "stats rows should be emitted while the Frame Source is still streaming"
    );
}

#[test]
fn streaming_detection_matches_collecting_api_for_supported_detectors() {
    let detector_cases = [
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

    for (detector, frames) in detector_cases {
        let options = DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        };
        let collected = detect_scenes(
            detector.clone(),
            VecFrameSource::new(frames.clone()),
            options.clone(),
        )
        .unwrap();
        let mut stats_sink = MemoryStatsSink::default();
        let scene_list = detect_scenes_streaming(
            detector,
            VecFrameSource::new(frames),
            options,
            &mut stats_sink,
        )
        .unwrap();

        assert_eq!(scene_list, collected.scene_list);
        assert_eq!(stats_sink.stats, collected.stats);
    }
}

#[test]
fn timecode_parses_frames_seconds_and_clock_values() {
    let rate = FrameRate(10.0);

    assert_eq!(Timecode::parse_at_rate("12", rate).unwrap().frames(), 12);
    assert_eq!(Timecode::parse_at_rate("1.5s", rate).unwrap().frames(), 15);
    assert_eq!(
        Timecode::parse_at_rate("00:00:02.500", rate)
            .unwrap()
            .frames(),
        25
    );
}

#[test]
fn empty_video_emits_no_scenes_and_no_stats_rows() {
    let result = detect_frames(
        DetectorConfig::Content(ContentDetectorConfig::default()),
        FrameRate(10.0),
        &[],
        DetectionOptions::default(),
    )
    .unwrap();

    assert!(result.scene_list.scenes.is_empty());
    assert_eq!(result.stats.metric_names, vec!["content_val"]);
    assert!(result.stats.rows.is_empty());
}

#[test]
fn single_scene_video_spans_all_decoded_frames() {
    let frames = frames(&[[0, 0, 0], [0, 0, 0], [0, 0, 0]]);
    let result = detect_frames(
        DetectorConfig::Content(ContentDetectorConfig {
            threshold: 20.0,
            ..Default::default()
        }),
        FrameRate(10.0),
        &frames,
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(
        result.scene_list.scenes,
        vec![SceneSpan {
            start: FrameIndex(0),
            end: FrameIndex(3)
        }]
    );
    assert_eq!(result.stats.rows.len(), frames.len());
}

#[test]
fn content_detector_emits_scene_list_for_hard_color_change() {
    let frames = frames(&[
        [0, 0, 0],
        [0, 0, 0],
        [0, 0, 0],
        [255, 255, 255],
        [255, 255, 255],
    ]);
    let result = detect_scenes(
        DetectorConfig::Content(ContentDetectorConfig {
            threshold: 20.0,
            ..Default::default()
        }),
        VecFrameSource::new(frames),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(
        result.scene_list.scenes,
        vec![
            SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(3)
            },
            SceneSpan {
                start: FrameIndex(3),
                end: FrameIndex(5)
            }
        ]
    );
    assert_eq!(result.stats.metric_names, vec!["content_val"]);
}

#[test]
fn content_detector_min_scene_len_suppresses_close_scene_boundaries() {
    let frames = frames(&[
        [0, 0, 0],
        [0, 0, 0],
        [255, 255, 255],
        [255, 255, 255],
        [255, 255, 255],
        [255, 255, 255],
        [0, 0, 0],
        [0, 0, 0],
        [0, 0, 0],
        [0, 0, 0],
        [0, 0, 0],
        [0, 0, 0],
    ]);
    let result = detect_frames(
        DetectorConfig::Content(ContentDetectorConfig {
            threshold: 20.0,
            ..Default::default()
        }),
        FrameRate(10.0),
        &frames,
        DetectionOptions {
            min_scene_len: 5,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(
        result.scene_list.scenes,
        vec![SceneSpan {
            start: FrameIndex(0),
            end: FrameIndex(12)
        }]
    );
}

#[test]
fn content_detector_threshold_controls_scene_boundary_sensitivity() {
    let frames = frames(&[[0, 0, 0], [0, 0, 0], [50, 50, 50], [50, 50, 50]]);

    let lower_threshold = detect_frames(
        DetectorConfig::Content(ContentDetectorConfig {
            threshold: 10.0,
            ..Default::default()
        }),
        FrameRate(10.0),
        &frames,
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let higher_threshold = detect_frames(
        DetectorConfig::Content(ContentDetectorConfig {
            threshold: 20.0,
            ..Default::default()
        }),
        FrameRate(10.0),
        &frames,
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(lower_threshold.scene_list.scenes[0].end, FrameIndex(2));
    assert_eq!(
        higher_threshold.scene_list.scenes,
        vec![SceneSpan {
            start: FrameIndex(0),
            end: FrameIndex(4)
        }]
    );
}

#[test]
fn content_detection_stats_derive_scene_list_and_boundary_review() {
    let frames = frames(&[[0, 0, 0], [0, 0, 0], [255, 255, 255], [255, 255, 255]]);
    let stats = detect_content_stats(
        VecFrameSource::new(frames),
        ContentDetectorConfig {
            threshold: 20.0,
            ..Default::default()
        },
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(stats.total_frames, 4);
    assert_eq!(stats.metric_names[0], "content_val");
    assert_eq!(stats.rows[0].decision, DetectionStatsDecision::NotEvaluated);
    assert_eq!(
        stats.rows[1].decision,
        DetectionStatsDecision::BelowThreshold
    );
    assert_eq!(stats.rows[2].decision, DetectionStatsDecision::Accepted);
    assert_eq!(stats.rows[2].threshold, 20.0);
    assert!(stats.rows[2].score >= 20.0);
    assert!(stats.rows[2].metrics.contains_key("delta_luminance"));

    let scene_list = scene_list_from_content_detection_stats(&stats);
    assert_eq!(
        scene_list.scenes,
        vec![
            SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(2)
            },
            SceneSpan {
                start: FrameIndex(2),
                end: FrameIndex(4)
            }
        ]
    );

    let review = boundary_review_from_content_detection_stats(
        &stats,
        BoundaryReviewOptions {
            review_threshold: Some(10.0),
        },
    );
    assert_eq!(review.candidates.len(), 1);
    assert_eq!(
        review.candidates[0].status,
        BoundaryCandidateStatus::Accepted
    );
    assert_eq!(review.candidates[0].frame, FrameIndex(2));
}

#[test]
fn content_boundary_review_classifies_candidates_without_changing_scene_list() {
    let frames = frames(&[[0, 0, 0], [50, 50, 50], [200, 200, 200], [50, 50, 50]]);
    let mut stats_sink = NoopStatsSink;
    let review = detect_boundary_review_streaming(
        DetectorConfig::Content(ContentDetectorConfig {
            threshold: 33.0,
            ..Default::default()
        }),
        VecFrameSource::new(frames),
        DetectionOptions {
            min_scene_len: 2,
            ..Default::default()
        },
        BoundaryReviewOptions {
            review_threshold: Some(16.0),
        },
        &mut stats_sink,
    )
    .unwrap();

    assert_eq!(
        review
            .candidates
            .iter()
            .map(|candidate| candidate.status)
            .collect::<Vec<_>>(),
        vec![
            BoundaryCandidateStatus::NearMiss,
            BoundaryCandidateStatus::Accepted,
            BoundaryCandidateStatus::SuppressedMinSceneLen,
        ]
    );
    assert_eq!(
        review.scene_list.scenes,
        vec![
            SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(2)
            },
            SceneSpan {
                start: FrameIndex(2),
                end: FrameIndex(4)
            }
        ]
    );
}

#[test]
fn content_boundary_review_defaults_to_eighty_percent_of_detector_threshold() {
    let frames = frames(&[[0, 0, 0], [70, 70, 70], [200, 200, 200]]);
    let mut stats_sink = NoopStatsSink;
    let review = detect_boundary_review_streaming(
        DetectorConfig::Content(ContentDetectorConfig {
            threshold: 40.0,
            ..Default::default()
        }),
        VecFrameSource::new(frames),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
        BoundaryReviewOptions::default(),
        &mut stats_sink,
    )
    .unwrap();

    assert_eq!(review.review_threshold, 32.0);
    assert_eq!(review.candidates.len(), 1);
    assert_eq!(review.candidates[0].frame, FrameIndex(2));
}

#[test]
fn boundary_review_sorts_candidates_by_distance_to_detector_threshold() {
    let frames = frames(&[[0, 0, 0], [80, 80, 80], [190, 190, 190], [90, 90, 90]]);
    let mut stats_sink = NoopStatsSink;
    let review = detect_boundary_review_streaming(
        DetectorConfig::Content(ContentDetectorConfig {
            threshold: 33.0,
            ..Default::default()
        }),
        VecFrameSource::new(frames),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
        BoundaryReviewOptions {
            review_threshold: Some(0.0),
        },
        &mut stats_sink,
    )
    .unwrap();

    assert_eq!(
        review
            .candidates
            .iter()
            .map(|candidate| candidate.frame)
            .collect::<Vec<_>>(),
        vec![FrameIndex(3), FrameIndex(2), FrameIndex(1), FrameIndex(0)]
    );
}

#[test]
fn adaptive_boundary_review_keeps_min_content_value_as_noise_floor() {
    let frames = frames(&[
        [0, 0, 0],
        [1, 1, 1],
        [100, 100, 100],
        [101, 101, 101],
        [102, 102, 102],
    ]);

    let mut strict_stats_sink = NoopStatsSink;
    let strict_review = detect_boundary_review_streaming(
        DetectorConfig::Adaptive(AdaptiveDetectorConfig {
            threshold: 100.0,
            min_content_val: 34.0,
            frame_window: 1,
            ..Default::default()
        }),
        VecFrameSource::new(frames.clone()),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
        BoundaryReviewOptions {
            review_threshold: Some(50.0),
        },
        &mut strict_stats_sink,
    )
    .unwrap();

    let mut relaxed_stats_sink = NoopStatsSink;
    let relaxed_review = detect_boundary_review_streaming(
        DetectorConfig::Adaptive(AdaptiveDetectorConfig {
            threshold: 100.0,
            min_content_val: 30.0,
            frame_window: 1,
            ..Default::default()
        }),
        VecFrameSource::new(frames),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
        BoundaryReviewOptions {
            review_threshold: Some(50.0),
        },
        &mut relaxed_stats_sink,
    )
    .unwrap();

    assert!(strict_review.candidates.is_empty());
    assert_eq!(relaxed_review.candidates.len(), 1);
    assert_eq!(
        relaxed_review.candidates[0].status,
        BoundaryCandidateStatus::NearMiss
    );
}

#[test]
fn content_detector_luma_only_ignores_chroma_only_scene_boundary() {
    let frames = frames(&[[255, 0, 0], [255, 0, 0], [255, 255, 0], [255, 255, 0]]);
    let result = detect_frames(
        DetectorConfig::Content(ContentDetectorConfig {
            threshold: 20.0,
            luma_only: true,
            ..Default::default()
        }),
        FrameRate(10.0),
        &frames,
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(
        result.scene_list.scenes,
        vec![SceneSpan {
            start: FrameIndex(0),
            end: FrameIndex(4)
        }]
    );
    assert!(result.stats.rows[2].metrics["content_val"] < 1.0);
}

#[test]
fn adaptive_detector_uses_local_ratio_to_ignore_background_motion() {
    let frames = frames(&[
        [0, 0, 0],
        [3, 3, 3],
        [6, 6, 6],
        [255, 255, 255],
        [252, 252, 252],
        [249, 249, 249],
    ]);
    let result = detect_frames(
        DetectorConfig::Adaptive(AdaptiveDetectorConfig {
            threshold: 3.0,
            min_content_val: 20.0,
            frame_window: 1,
            ..Default::default()
        }),
        FrameRate(10.0),
        &frames,
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(result.scene_list.scenes[0].end, FrameIndex(3));
    assert_eq!(
        result.stats.metric_names,
        vec!["adaptive_ratio", "content_val"]
    );
}

#[test]
fn adaptive_detector_options_control_scene_boundary_sensitivity() {
    let frames = frames(&[
        [0, 0, 0],
        [80, 80, 80],
        [100, 100, 100],
        [200, 200, 200],
        [220, 220, 220],
        [140, 140, 140],
        [140, 140, 140],
    ]);

    let sensitive = detect_scenes(
        DetectorConfig::Adaptive(AdaptiveDetectorConfig {
            threshold: 3.0,
            min_content_val: 30.0,
            frame_window: 1,
            ..Default::default()
        }),
        VecFrameSource::new(frames.clone()),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let higher_ratio_threshold = detect_scenes(
        DetectorConfig::Adaptive(AdaptiveDetectorConfig {
            threshold: 6.0,
            min_content_val: 30.0,
            frame_window: 1,
            ..Default::default()
        }),
        VecFrameSource::new(frames.clone()),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let higher_min_content_val = detect_scenes(
        DetectorConfig::Adaptive(AdaptiveDetectorConfig {
            threshold: 3.0,
            min_content_val: 34.0,
            frame_window: 1,
            ..Default::default()
        }),
        VecFrameSource::new(frames.clone()),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let wider_frame_window = detect_scenes(
        DetectorConfig::Adaptive(AdaptiveDetectorConfig {
            threshold: 3.0,
            min_content_val: 30.0,
            frame_window: 2,
            ..Default::default()
        }),
        VecFrameSource::new(frames),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(
        sensitive.scene_list.scenes,
        vec![
            SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(3)
            },
            SceneSpan {
                start: FrameIndex(3),
                end: FrameIndex(7)
            }
        ]
    );
    assert_eq!(
        higher_ratio_threshold.scene_list.scenes,
        vec![SceneSpan {
            start: FrameIndex(0),
            end: FrameIndex(7)
        }]
    );
    assert_eq!(
        higher_min_content_val.scene_list.scenes,
        higher_ratio_threshold.scene_list.scenes
    );
    assert_eq!(
        wider_frame_window.scene_list.scenes,
        higher_ratio_threshold.scene_list.scenes
    );
}

#[test]
fn threshold_detector_places_fade_return_scene_boundary_between_fade_events() {
    let frames = frames(&[
        [0, 0, 0],
        [0, 0, 0],
        [0, 0, 0],
        [0, 0, 0],
        [128, 128, 128],
        [128, 128, 128],
        [255, 255, 255],
        [255, 255, 255],
        [255, 255, 255],
        [255, 255, 255],
    ]);
    let result = detect_scenes(
        DetectorConfig::Threshold(ThresholdDetectorConfig {
            threshold: 12.0,
            ..Default::default()
        }),
        VecFrameSource::new(frames),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(
        result.scene_list.scenes,
        vec![
            SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(2)
            },
            SceneSpan {
                start: FrameIndex(2),
                end: FrameIndex(10)
            }
        ]
    );
    assert_eq!(result.stats.metric_names, vec!["average_rgb"]);
}

#[test]
fn threshold_detector_options_control_fade_scene_boundaries() {
    let fade_out_then_in = frames(&[
        [20, 20, 20],
        [20, 20, 20],
        [0, 0, 0],
        [0, 0, 0],
        [80, 80, 80],
        [80, 80, 80],
    ]);

    let default_bias = detect_scenes(
        DetectorConfig::Threshold(ThresholdDetectorConfig {
            threshold: 12.0,
            fade_bias: 0.0,
            add_last_scene: true,
        }),
        VecFrameSource::new(fade_out_then_in.clone()),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let no_dark_frames_at_threshold = detect_scenes(
        DetectorConfig::Threshold(ThresholdDetectorConfig {
            threshold: 0.0,
            fade_bias: 0.0,
            add_last_scene: true,
        }),
        VecFrameSource::new(fade_out_then_in.clone()),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let fade_out_bias = detect_scenes(
        DetectorConfig::Threshold(ThresholdDetectorConfig {
            threshold: 12.0,
            fade_bias: -1.0,
            add_last_scene: true,
        }),
        VecFrameSource::new(fade_out_then_in.clone()),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let fade_in_bias = detect_scenes(
        DetectorConfig::Threshold(ThresholdDetectorConfig {
            threshold: 12.0,
            fade_bias: 1.0,
            add_last_scene: true,
        }),
        VecFrameSource::new(fade_out_then_in),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(default_bias.scene_list.scenes[0].end, FrameIndex(3));
    assert_eq!(
        no_dark_frames_at_threshold.scene_list.scenes,
        vec![SceneSpan {
            start: FrameIndex(0),
            end: FrameIndex(6)
        }]
    );
    assert_eq!(fade_out_bias.scene_list.scenes[0].end, FrameIndex(2));
    assert_eq!(fade_in_bias.scene_list.scenes[0].end, FrameIndex(4));

    let ends_on_fade_out = frames(&[[80, 80, 80], [80, 80, 80], [0, 0, 0], [0, 0, 0], [0, 0, 0]]);
    let with_final_scene = detect_scenes(
        DetectorConfig::Threshold(ThresholdDetectorConfig {
            threshold: 12.0,
            fade_bias: 0.0,
            add_last_scene: true,
        }),
        VecFrameSource::new(ends_on_fade_out.clone()),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let without_final_scene = detect_scenes(
        DetectorConfig::Threshold(ThresholdDetectorConfig {
            threshold: 12.0,
            fade_bias: 0.0,
            add_last_scene: false,
        }),
        VecFrameSource::new(ends_on_fade_out),
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(with_final_scene.scene_list.scenes[0].end, FrameIndex(2));
    assert_eq!(
        without_final_scene.scene_list.scenes,
        vec![SceneSpan {
            start: FrameIndex(0),
            end: FrameIndex(5)
        }]
    );
}

#[test]
fn histogram_detector_emits_scene_list_for_hard_luma_change() {
    let frames = frames(&[[0, 0, 0], [0, 0, 0], [255, 255, 255], [255, 255, 255]]);
    let result = detect_frames(
        DetectorConfig::Histogram(HistogramDetectorConfig {
            threshold: 0.05,
            bins: 256,
        }),
        FrameRate(10.0),
        &frames,
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(
        result.scene_list.scenes,
        vec![
            SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(2)
            },
            SceneSpan {
                start: FrameIndex(2),
                end: FrameIndex(4)
            }
        ]
    );
    assert_eq!(result.stats.metric_names, vec!["hist_diff [bins=256]"]);
    assert_eq!(result.stats.rows.len(), frames.len());
    assert_eq!(result.stats.rows[0].metrics["hist_diff [bins=256]"], 0.0);
}

#[test]
fn hash_detector_emits_scene_list_for_structural_pattern_change() {
    let frames = vec![
        Frame::solid(0, 64, 64, [0, 0, 0]),
        Frame::solid(1, 64, 64, [0, 0, 0]),
        structural_pattern_frame(2),
        structural_pattern_frame(3),
    ];
    let result = detect_frames(
        DetectorConfig::Hash(HashDetectorConfig {
            threshold: 0.395,
            size: 16,
            lowpass: 2,
        }),
        FrameRate(10.0),
        &frames,
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(
        result.scene_list.scenes,
        vec![
            SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(2)
            },
            SceneSpan {
                start: FrameIndex(2),
                end: FrameIndex(4)
            }
        ]
    );
    assert_eq!(
        result.stats.metric_names,
        vec!["hash_dist [size=16 lowpass=2]"]
    );
    assert_eq!(result.stats.rows.len(), frames.len());
    assert_eq!(
        result.stats.rows[0].metrics["hash_dist [size=16 lowpass=2]"],
        0.0
    );
}

#[test]
fn hash_detector_does_not_require_uniform_luma_change_as_scene_boundary() {
    let frames = frames(&[[0, 0, 0], [0, 0, 0], [255, 255, 255], [255, 255, 255]]);
    let result = detect_frames(
        DetectorConfig::Hash(HashDetectorConfig {
            threshold: 0.395,
            size: 4,
            lowpass: 2,
        }),
        FrameRate(10.0),
        &frames,
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();

    assert_eq!(
        result.scene_list.scenes,
        vec![SceneSpan {
            start: FrameIndex(0),
            end: FrameIndex(4)
        }]
    );
}

#[test]
fn scene_list_csv_uses_pyscenedetect_frame_and_timecode_columns() {
    let scene_list = SceneList {
        frame_rate: FrameRate(10.0),
        scenes: vec![
            SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(3),
            },
            SceneSpan {
                start: FrameIndex(3),
                end: FrameIndex(5),
            },
        ],
    };
    let mut output = Vec::new();

    write_scene_list_csv(&scene_list, &mut output).unwrap();
    let output = String::from_utf8(output).unwrap();

    assert!(output.starts_with("Timecode List:,00:00:00.300\n"));
    assert!(output.contains(
        "Scene Number,Start Frame,Start Timecode,Start Time (seconds),End Frame,End Timecode,End Time (seconds),Length (frames),Length (timecode),Length (seconds)"
    ));
    assert!(output
        .contains("1,1,00:00:00.000,0.000000,3,00:00:00.300,0.300000,3,00:00:00.300,0.300000"));
    assert!(output
        .contains("2,4,00:00:00.300,0.300000,5,00:00:00.500,0.500000,2,00:00:00.200,0.200000"));
}

#[test]
fn stats_csv_contains_detector_metric_columns() {
    let frames = frames(&[[0, 0, 0], [3, 3, 3], [255, 255, 255], [252, 252, 252]]);
    let result = detect_frames(
        DetectorConfig::Adaptive(AdaptiveDetectorConfig {
            threshold: 3.0,
            min_content_val: 20.0,
            frame_window: 1,
            ..Default::default()
        }),
        FrameRate(10.0),
        &frames,
        DetectionOptions {
            min_scene_len: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let mut output = Vec::new();

    write_stats_csv(&result.stats, &mut output).unwrap();
    let output = String::from_utf8(output).unwrap();
    let lines: Vec<_> = output.lines().collect();

    assert_eq!(lines[0], "Frame Number,adaptive_ratio,content_val");
    assert_eq!(lines.len(), frames.len() + 1);
    assert!(lines[1].starts_with("0,0.000000,0.000000"));
}

#[test]
fn json_scene_list_and_ndjson_events_use_export_fields() {
    let scene_list = SceneList {
        frame_rate: FrameRate(10.0),
        scenes: vec![
            SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(3),
            },
            SceneSpan {
                start: FrameIndex(3),
                end: FrameIndex(5),
            },
        ],
    };
    let mut json = Vec::new();
    let mut ndjson = Vec::new();

    write_scene_list_json(&scene_list, &mut json).unwrap();
    write_scene_events_ndjson(&scene_list, &mut ndjson).unwrap();

    let json: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(json["frame_rate"], 10.0);
    assert_eq!(json["scene_count"], 2);
    assert_eq!(json["scenes"][0]["scene_number"], 1);
    assert_eq!(json["scenes"][0]["start_frame"], 1);
    assert_eq!(json["scenes"][0]["start_timecode"], "00:00:00.000");
    assert_eq!(json["scenes"][0]["end_frame"], 3);
    assert_eq!(json["scenes"][0]["end_timecode"], "00:00:00.300");
    assert_eq!(json["scenes"][0]["length_frames"], 3);

    let ndjson = String::from_utf8(ndjson).unwrap();
    let events: Vec<serde_json::Value> = ndjson
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(events.len(), 2);
    assert_eq!(events[0]["event"], "scene");
    assert_eq!(events[0]["scene_number"], 1);
    assert_eq!(events[1]["start_frame"], 4);
    assert_eq!(events[1]["length_seconds"], 0.2);
}

#[test]
fn html_scene_list_output_contains_scene_spans_and_timecodes() {
    let scene_list = SceneList {
        frame_rate: FrameRate(10.0),
        scenes: vec![
            SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(3),
            },
            SceneSpan {
                start: FrameIndex(3),
                end: FrameIndex(5),
            },
        ],
    };
    let mut html = Vec::new();

    write_scene_list_html(&scene_list, &mut html).unwrap();
    let html = String::from_utf8(html).unwrap();

    assert!(html.contains("<title>Scene List</title>"));
    assert!(html.contains("<h1>Scene List</h1>"));
    assert!(html.contains("<p>Frame rate: 10.000000</p>"));
    assert!(html.contains("<p>Scene count: 2</p>"));
    assert!(html.contains("<th>Start Timecode</th>"));
    assert!(html.contains("<td>1</td>"));
    assert!(html.contains("<td>00:00:00.000</td>"));
    assert!(html.contains("<td>00:00:00.300</td>"));
    assert!(html.contains("<td>00:00:00.500</td>"));
}

#[test]
fn boundary_review_writers_use_ranked_candidate_export_fields() {
    let review = BoundaryReview {
        frame_rate: FrameRate(10.0),
        detector: "content".to_owned(),
        score_metric: "content_val".to_owned(),
        detector_threshold: 20.0,
        review_threshold: 16.0,
        scene_list: SceneList {
            frame_rate: FrameRate(10.0),
            scenes: vec![
                SceneSpan {
                    start: FrameIndex(0),
                    end: FrameIndex(3),
                },
                SceneSpan {
                    start: FrameIndex(3),
                    end: FrameIndex(5),
                },
            ],
        },
        candidates: vec![BoundaryCandidateReview {
            candidate_number: 2,
            status: BoundaryCandidateStatus::NearMiss,
            frame: FrameIndex(3),
            score_metric: "content_val".to_owned(),
            score: 18.5,
            detector_threshold: 20.0,
            review_threshold: 16.0,
            threshold_distance: 1.5,
            metrics: BTreeMap::from([("content_val".to_owned(), 18.5)]),
            before: ReviewSceneContext {
                start: FrameIndex(0),
                end: FrameIndex(3),
            },
            after: ReviewSceneContext {
                start: FrameIndex(3),
                end: FrameIndex(5),
            },
        }],
    };
    let mut csv = Vec::new();
    let mut json = Vec::new();

    write_boundary_review_csv(&review, &mut csv).unwrap();
    write_boundary_review_json(&review, &mut json).unwrap();

    let csv = String::from_utf8(csv).unwrap();
    assert!(csv.contains("Rank,Status,Boundary Candidate Number"));
    assert!(csv.contains(
        "1,near_miss,2,4,3,00:00:00.300,0.300000,content_val,18.500000,20.000000,16.000000,1.500000,1,3,4,5,18.500000"
    ));

    let json: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert_eq!(json["frame_rate"], 10.0);
    assert_eq!(json["detector"], "content");
    assert_eq!(json["sort"], "threshold_distance");
    assert_eq!(json["candidate_count"], 1);
    assert_eq!(json["boundary_candidates"][0]["rank"], 1);
    assert_eq!(json["boundary_candidates"][0]["status"], "near_miss");
    assert_eq!(
        json["boundary_candidates"][0]["boundary_timecode"],
        "00:00:00.300"
    );
    assert_eq!(
        json["boundary_candidates"][0]["metrics"]["content_val"],
        18.5
    );
}
