use std::collections::VecDeque;

use super::{
    build_scene_list, content_score, emit_ready_adaptive_rows, hash_distance,
    histogram_correlation, luma_histogram, perceptual_hash, round_half_to_even,
    AdaptiveDetectorConfig, AdaptiveSample, ContentDetectorConfig, DetectionOptions,
    DetectionResult, DetectionStats, DetectionStatsSink, DetectorConfig, FadeType, Frame,
    FrameIndex, FrameRate, HashDetectorConfig, HistogramDetectorConfig, Result, SceneBoundary,
    StatsRow, ThresholdDetectorConfig,
};

/// Incremental scene detection for consumers which already own frame decoding.
///
/// This is the browser/WASM seam: callers push decoded RGB frames one at a time,
/// while detector scoring, minimum-scene-length behavior, adaptive windows, fade
/// state, and final scene construction remain owned by `scenedetect-core`.
pub struct DetectionSession {
    frame_rate: FrameRate,
    options: DetectionOptions,
    state: SessionState,
    stats: DetectionStats,
}

impl DetectionSession {
    pub fn new(detector: DetectorConfig, frame_rate: FrameRate, options: DetectionOptions) -> Self {
        let metric_names = metric_names(&detector);
        Self {
            frame_rate,
            options,
            state: SessionState::new(detector),
            stats: DetectionStats {
                metric_names,
                rows: Vec::new(),
            },
        }
    }

    pub fn push_frame(&mut self, frame: Frame) -> Result<()> {
        self.state
            .push_frame(frame, self.options.min_scene_len, &mut self.stats)
    }

    pub fn finish(mut self) -> Result<DetectionResult> {
        let (boundaries, total_frames) = self
            .state
            .finish(self.options.min_scene_len, &mut self.stats)?;
        let scene_list = build_scene_list(self.frame_rate, total_frames, boundaries, self.options);
        Ok(DetectionResult {
            scene_list,
            stats: self.stats,
        })
    }
}

fn metric_names(detector: &DetectorConfig) -> Vec<String> {
    match detector {
        DetectorConfig::Content(_) => vec!["content_val".to_owned()],
        DetectorConfig::Adaptive(_) => {
            vec!["adaptive_ratio".to_owned(), "content_val".to_owned()]
        }
        DetectorConfig::Threshold(_) => vec!["average_rgb".to_owned()],
        DetectorConfig::Histogram(config) => {
            vec![format!("hist_diff [bins={}]", config.bins.max(1))]
        }
        DetectorConfig::Hash(config) => vec![format!(
            "hash_dist [size={} lowpass={}]",
            config.size.max(1),
            config.lowpass.max(1)
        )],
    }
}

enum SessionState {
    Content {
        config: ContentDetectorConfig,
        previous: Option<Frame>,
        last_candidate_boundary: u64,
        boundaries: Vec<SceneBoundary>,
        total_frames: u64,
    },
    Adaptive {
        config: AdaptiveDetectorConfig,
        previous: Option<Frame>,
        samples: VecDeque<AdaptiveSample>,
        next_emit: usize,
        last_boundary: u64,
        boundaries: Vec<SceneBoundary>,
        total_frames: usize,
    },
    Threshold {
        config: ThresholdDetectorConfig,
        last_scene_cut: u64,
        last_fade_frame: Option<u64>,
        last_fade_type: Option<FadeType>,
        boundaries: Vec<SceneBoundary>,
        total_frames: u64,
    },
    Histogram {
        config: HistogramDetectorConfig,
        metric_name: String,
        previous_histogram: Option<Vec<f64>>,
        last_cut: u64,
        boundaries: Vec<SceneBoundary>,
        total_frames: u64,
    },
    Hash {
        config: HashDetectorConfig,
        metric_name: String,
        previous_hash: Option<Vec<bool>>,
        last_cut: u64,
        boundaries: Vec<SceneBoundary>,
        total_frames: u64,
    },
}

impl SessionState {
    fn new(detector: DetectorConfig) -> Self {
        match detector {
            DetectorConfig::Content(config) => Self::Content {
                config,
                previous: None,
                last_candidate_boundary: 0,
                boundaries: Vec::new(),
                total_frames: 0,
            },
            DetectorConfig::Adaptive(config) => Self::Adaptive {
                config,
                previous: None,
                samples: VecDeque::new(),
                next_emit: 0,
                last_boundary: 0,
                boundaries: Vec::new(),
                total_frames: 0,
            },
            DetectorConfig::Threshold(config) => Self::Threshold {
                config,
                last_scene_cut: 0,
                last_fade_frame: None,
                last_fade_type: None,
                boundaries: Vec::new(),
                total_frames: 0,
            },
            DetectorConfig::Histogram(config) => Self::Histogram {
                metric_name: format!("hist_diff [bins={}]", config.bins.max(1)),
                config,
                previous_histogram: None,
                last_cut: 0,
                boundaries: Vec::new(),
                total_frames: 0,
            },
            DetectorConfig::Hash(config) => Self::Hash {
                metric_name: format!(
                    "hash_dist [size={} lowpass={}]",
                    config.size.max(1),
                    config.lowpass.max(1)
                ),
                config,
                previous_hash: None,
                last_cut: 0,
                boundaries: Vec::new(),
                total_frames: 0,
            },
        }
    }

    fn push_frame(
        &mut self,
        frame: Frame,
        min_scene_len: u64,
        stats: &mut DetectionStats,
    ) -> Result<()> {
        match self {
            Self::Content {
                config,
                previous,
                last_candidate_boundary,
                boundaries,
                total_frames,
            } => {
                let content_val = previous.as_ref().map_or(0.0, |previous| {
                    content_score(previous, &frame, &config.weights, config.luma_only)
                });
                stats.rows.push(StatsRow {
                    frame: frame.index,
                    metrics: std::collections::BTreeMap::from([(
                        "content_val".to_owned(),
                        content_val,
                    )]),
                });

                let frame_number = frame.index.0;
                if content_val >= config.threshold {
                    if frame_number.saturating_sub(*last_candidate_boundary) >= min_scene_len {
                        boundaries.push(SceneBoundary { frame: frame.index });
                    }
                    *last_candidate_boundary = frame_number;
                }
                *previous = Some(frame);
                *total_frames += 1;
            }
            Self::Adaptive {
                config,
                previous,
                samples,
                next_emit,
                last_boundary,
                boundaries,
                total_frames,
            } => {
                let content_val = previous.as_ref().map_or(0.0, |previous| {
                    content_score(previous, &frame, &config.weights, config.luma_only)
                });
                samples.push_back(AdaptiveSample {
                    position: *total_frames,
                    frame: frame.index,
                    content_val,
                });
                *previous = Some(frame);
                *total_frames += 1;

                let mut sink = SessionStatsSink(stats);
                emit_ready_adaptive_rows(
                    samples,
                    next_emit,
                    *total_frames,
                    None,
                    config,
                    min_scene_len,
                    last_boundary,
                    boundaries,
                    &mut sink,
                )?;
            }
            Self::Threshold {
                config,
                last_scene_cut,
                last_fade_frame,
                last_fade_type,
                boundaries,
                total_frames,
            } => {
                if *total_frames == 0 {
                    *last_scene_cut = frame.index.0;
                }
                let luma = frame.mean_luma();
                stats.rows.push(StatsRow {
                    frame: frame.index,
                    metrics: std::collections::BTreeMap::from([("average_rgb".to_owned(), luma)]),
                });

                let frame_number = frame.index.0;
                match *last_fade_type {
                    None => {
                        *last_fade_frame = Some(frame_number);
                        *last_fade_type = Some(if luma < config.threshold {
                            FadeType::Out
                        } else {
                            FadeType::In
                        });
                    }
                    Some(FadeType::In) if luma < config.threshold => {
                        *last_fade_frame = Some(frame_number);
                        *last_fade_type = Some(FadeType::Out);
                    }
                    Some(FadeType::Out) if luma >= config.threshold => {
                        if frame_number.saturating_sub(*last_scene_cut) >= min_scene_len {
                            let fade_out_frame = last_fade_frame.unwrap_or(frame_number);
                            let duration = frame_number.saturating_sub(fade_out_frame);
                            let split = fade_out_frame
                                + round_half_to_even(
                                    duration as f64 * (1.0 + config.fade_bias) / 2.0,
                                );
                            boundaries.push(SceneBoundary {
                                frame: FrameIndex(split),
                            });
                            *last_scene_cut = frame_number;
                        }
                        *last_fade_frame = Some(frame_number);
                        *last_fade_type = Some(FadeType::In);
                    }
                    _ => {}
                }
                *total_frames += 1;
            }
            Self::Histogram {
                config,
                metric_name,
                previous_histogram,
                last_cut,
                boundaries,
                total_frames,
            } => {
                let bins = config.bins.max(1);
                let histogram = luma_histogram(&frame, bins);
                let hist_diff = previous_histogram
                    .as_ref()
                    .map_or(0.0, |previous| histogram_correlation(previous, &histogram));
                stats.rows.push(StatsRow {
                    frame: frame.index,
                    metrics: std::collections::BTreeMap::from([(metric_name.clone(), hist_diff)]),
                });

                let frame_number = frame.index.0;
                if previous_histogram.is_some()
                    && hist_diff <= 1.0 - config.threshold
                    && frame_number.saturating_sub(*last_cut) >= min_scene_len
                {
                    boundaries.push(SceneBoundary { frame: frame.index });
                    *last_cut = frame_number;
                }
                *previous_histogram = Some(histogram);
                *total_frames += 1;
            }
            Self::Hash {
                config,
                metric_name,
                previous_hash,
                last_cut,
                boundaries,
                total_frames,
            } => {
                let size = config.size.max(1);
                let lowpass = config.lowpass.max(1);
                let frame_hash = perceptual_hash(&frame, size, lowpass);
                let hash_dist = previous_hash
                    .as_ref()
                    .map_or(0.0, |previous| hash_distance(previous, &frame_hash));
                stats.rows.push(StatsRow {
                    frame: frame.index,
                    metrics: std::collections::BTreeMap::from([(metric_name.clone(), hash_dist)]),
                });

                let frame_number = frame.index.0;
                if previous_hash.is_some()
                    && hash_dist >= config.threshold
                    && frame_number.saturating_sub(*last_cut) >= min_scene_len
                {
                    boundaries.push(SceneBoundary { frame: frame.index });
                    *last_cut = frame_number;
                }
                *previous_hash = Some(frame_hash);
                *total_frames += 1;
            }
        }
        Ok(())
    }

    fn finish(
        self,
        min_scene_len: u64,
        stats: &mut DetectionStats,
    ) -> Result<(Vec<SceneBoundary>, u64)> {
        match self {
            Self::Content {
                boundaries,
                total_frames,
                ..
            } => Ok((boundaries, total_frames)),
            Self::Adaptive {
                config,
                mut samples,
                mut next_emit,
                mut last_boundary,
                mut boundaries,
                total_frames,
                ..
            } => {
                let mut sink = SessionStatsSink(stats);
                emit_ready_adaptive_rows(
                    &mut samples,
                    &mut next_emit,
                    total_frames,
                    Some(total_frames),
                    &config,
                    min_scene_len,
                    &mut last_boundary,
                    &mut boundaries,
                    &mut sink,
                )?;
                Ok((boundaries, total_frames as u64))
            }
            Self::Threshold {
                config,
                last_scene_cut,
                last_fade_frame,
                last_fade_type,
                mut boundaries,
                total_frames,
            } => {
                if matches!(last_fade_type, Some(FadeType::Out))
                    && config.add_last_scene
                    && total_frames.saturating_sub(last_scene_cut) >= min_scene_len
                {
                    if let Some(frame) = last_fade_frame {
                        boundaries.push(SceneBoundary {
                            frame: FrameIndex(frame),
                        });
                    }
                }
                Ok((boundaries, total_frames))
            }
            Self::Histogram {
                boundaries,
                total_frames,
                ..
            }
            | Self::Hash {
                boundaries,
                total_frames,
                ..
            } => Ok((boundaries, total_frames)),
        }
    }
}

struct SessionStatsSink<'a>(&'a mut DetectionStats);

impl DetectionStatsSink for SessionStatsSink<'_> {
    fn start(&mut self, metric_names: &[String]) -> Result<()> {
        self.0.metric_names = metric_names.to_vec();
        Ok(())
    }

    fn row(&mut self, row: StatsRow) -> Result<()> {
        self.0.rows.push(row);
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        Ok(())
    }
}
