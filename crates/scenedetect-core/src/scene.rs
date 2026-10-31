use serde::{Deserialize, Serialize};

use crate::{DetectionOptions, DetectionStats, FrameIndex, FrameRate, MinSceneLenPolicy};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneBoundary {
    pub frame: FrameIndex,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneSpan {
    pub start: FrameIndex,
    pub end: FrameIndex,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneList {
    pub frame_rate: FrameRate,
    pub scenes: Vec<SceneSpan>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectionResult {
    pub scene_list: SceneList,
    pub stats: DetectionStats,
}

pub(crate) fn build_scene_list(
    frame_rate: FrameRate,
    total_frames: u64,
    boundaries: Vec<SceneBoundary>,
    options: DetectionOptions,
) -> SceneList {
    if total_frames == 0 {
        return SceneList {
            frame_rate,
            scenes: Vec::new(),
        };
    }

    let mut starts = vec![0];
    starts.extend(boundaries.into_iter().map(|boundary| boundary.frame.0));
    starts.sort_unstable();
    starts.dedup();

    let mut scenes = Vec::new();
    for pair in starts.windows(2) {
        scenes.push(SceneSpan {
            start: FrameIndex(pair[0]),
            end: FrameIndex(pair[1]),
        });
    }
    scenes.push(SceneSpan {
        start: FrameIndex(*starts.last().unwrap_or(&0)),
        end: FrameIndex(total_frames),
    });

    if matches!(options.min_scene_len_policy, MinSceneLenPolicy::MergeLast) && scenes.len() > 1 {
        let last_len = scenes
            .last()
            .map(|scene| scene.end.0 - scene.start.0)
            .unwrap_or(0);
        if last_len < options.min_scene_len {
            let last = scenes.pop().expect("last scene exists");
            if let Some(previous) = scenes.last_mut() {
                previous.end = last.end;
            }
        }
    }

    SceneList { frame_rate, scenes }
}
