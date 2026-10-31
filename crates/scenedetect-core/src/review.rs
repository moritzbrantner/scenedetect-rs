use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::scene::build_scene_list;
use crate::{DetectionOptions, FrameIndex, FrameRate, SceneBoundary, SceneList};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BoundaryReviewOptions {
    pub review_threshold: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryReview {
    pub frame_rate: FrameRate,
    pub detector: String,
    pub score_metric: String,
    pub detector_threshold: f64,
    pub review_threshold: f64,
    pub scene_list: SceneList,
    pub candidates: Vec<BoundaryCandidateReview>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundaryCandidateReview {
    pub candidate_number: usize,
    pub status: BoundaryCandidateStatus,
    pub frame: FrameIndex,
    pub score_metric: String,
    pub score: f64,
    pub detector_threshold: f64,
    pub review_threshold: f64,
    pub threshold_distance: f64,
    pub metrics: BTreeMap<String, f64>,
    pub before: ReviewSceneContext,
    pub after: ReviewSceneContext,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryCandidateStatus {
    Accepted,
    SuppressedMinSceneLen,
    NearMiss,
}

impl BoundaryCandidateStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::SuppressedMinSceneLen => "suppressed_min_scene_len",
            Self::NearMiss => "near_miss",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewSceneContext {
    pub start: FrameIndex,
    pub end: FrameIndex,
}

#[derive(Debug, Clone)]
pub(crate) struct BoundaryCandidateSeed {
    pub(crate) status: BoundaryCandidateStatus,
    pub(crate) frame: FrameIndex,
    pub(crate) score_metric: String,
    pub(crate) score: f64,
    pub(crate) detector_threshold: f64,
    pub(crate) review_threshold: f64,
    pub(crate) metrics: BTreeMap<String, f64>,
}

impl BoundaryCandidateSeed {
    fn into_review(
        self,
        candidate_number: usize,
        scene_list: &SceneList,
    ) -> BoundaryCandidateReview {
        let (before, after) = review_context(scene_list, self.frame);
        BoundaryCandidateReview {
            candidate_number,
            status: self.status,
            frame: self.frame,
            score_metric: self.score_metric,
            score: self.score,
            detector_threshold: self.detector_threshold,
            review_threshold: self.review_threshold,
            threshold_distance: (self.score - self.detector_threshold).abs(),
            metrics: self.metrics,
            before,
            after,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_boundary_review(
    frame_rate: FrameRate,
    detector: &str,
    score_metric: &str,
    detector_threshold: f64,
    review_threshold: f64,
    total_frames: u64,
    boundaries: Vec<SceneBoundary>,
    options: DetectionOptions,
    candidate_seeds: Vec<BoundaryCandidateSeed>,
) -> BoundaryReview {
    let scene_list = build_scene_list(frame_rate, total_frames, boundaries, options);
    let mut candidates: Vec<_> = candidate_seeds
        .into_iter()
        .enumerate()
        .map(|(idx, candidate)| candidate.into_review(idx + 1, &scene_list))
        .collect();
    candidates.sort_by(|left, right| {
        left.threshold_distance
            .partial_cmp(&right.threshold_distance)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.frame.cmp(&right.frame))
    });

    BoundaryReview {
        frame_rate,
        detector: detector.to_owned(),
        score_metric: score_metric.to_owned(),
        detector_threshold,
        review_threshold,
        scene_list,
        candidates,
    }
}

fn review_context(
    scene_list: &SceneList,
    frame: FrameIndex,
) -> (ReviewSceneContext, ReviewSceneContext) {
    if scene_list.scenes.is_empty() {
        return (
            ReviewSceneContext {
                start: FrameIndex(0),
                end: FrameIndex(0),
            },
            ReviewSceneContext {
                start: FrameIndex(0),
                end: FrameIndex(0),
            },
        );
    }

    if let Some(index) = scene_list
        .scenes
        .iter()
        .position(|scene| scene.start == frame && frame.0 > 0)
    {
        let before = scene_list.scenes[index - 1].clone();
        let after = scene_list.scenes[index].clone();
        return (
            ReviewSceneContext {
                start: before.start,
                end: before.end,
            },
            ReviewSceneContext {
                start: after.start,
                end: after.end,
            },
        );
    }

    let containing = scene_list
        .scenes
        .iter()
        .find(|scene| frame.0 >= scene.start.0 && frame.0 < scene.end.0)
        .or_else(|| scene_list.scenes.last())
        .expect("non-empty scene list has a last scene");

    (
        ReviewSceneContext {
            start: containing.start,
            end: frame,
        },
        ReviewSceneContext {
            start: frame,
            end: containing.end,
        },
    )
}
