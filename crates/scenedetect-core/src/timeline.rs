use serde::{Deserialize, Serialize};

use crate::{
    FrameIndex, FrameSource, FrameTiming, MediaTime, Result, SceneDetectError, SceneList, TimeBase,
};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneTimeline {
    pub scenes: Vec<SceneTimelineSpan>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneTimelineSpan {
    pub start: FrameIndex,
    pub end: FrameIndex,
    pub start_time: Option<MediaTime>,
    pub end_time: Option<MediaTime>,
}

pub fn scene_timeline_from_source<S>(scene_list: &SceneList, mut source: S) -> Result<SceneTimeline>
where
    S: FrameSource,
{
    let mut scenes = scene_list
        .scenes
        .iter()
        .map(|scene| SceneTimelineSpan {
            start: scene.start,
            end: scene.end,
            start_time: None,
            end_time: None,
        })
        .collect::<Vec<_>>();

    let Some(final_end) = scenes.last().map(|scene| scene.end.0) else {
        return Ok(SceneTimeline { scenes });
    };

    let mut start_cursor = 0_usize;
    let mut end_cursor = 0_usize;
    let mut final_frame_timing = None;
    let mut saw_final_frame = false;

    while let Some(frame) = source.next_frame_with_timing()? {
        let frame_index = frame.frame.index.0;

        if start_cursor < scenes.len() && scenes[start_cursor].start.0 < frame_index {
            return Err(SceneDetectError::FrameSource(format!(
                "timing source skipped Scene Timeline start frame {}",
                scenes[start_cursor].start.0
            )));
        }
        while start_cursor < scenes.len() && scenes[start_cursor].start.0 == frame_index {
            scenes[start_cursor].start_time = frame.timing.presentation_time;
            start_cursor += 1;
        }

        if end_cursor < scenes.len()
            && scenes[end_cursor].end.0 < final_end
            && scenes[end_cursor].end.0 < frame_index
        {
            return Err(SceneDetectError::FrameSource(format!(
                "timing source skipped Scene Timeline end frame {}",
                scenes[end_cursor].end.0
            )));
        }
        while end_cursor < scenes.len()
            && scenes[end_cursor].end.0 < final_end
            && scenes[end_cursor].end.0 == frame_index
        {
            scenes[end_cursor].end_time = frame.timing.presentation_time;
            end_cursor += 1;
        }

        if frame_index.checked_add(1) == Some(final_end) {
            saw_final_frame = true;
            final_frame_timing = Some(frame.timing);
        }
    }

    if start_cursor != scenes.len() || !saw_final_frame {
        return Err(SceneDetectError::FrameSource(
            "timing source ended before all Scene Timeline frames were observed".to_owned(),
        ));
    }

    if let Some(last_scene) = scenes.last_mut() {
        last_scene.end_time = final_frame_timing.as_ref().and_then(exact_frame_end_time);
    }

    Ok(SceneTimeline { scenes })
}

fn exact_frame_end_time(timing: &FrameTiming) -> Option<MediaTime> {
    let presentation = timing.presentation_time?;
    let duration = timing.duration?;
    if duration.ticks < 0
        || !is_positive_time_base(presentation.time_base)
        || !is_positive_time_base(duration.time_base)
    {
        return None;
    }

    if let Some(duration_ticks) = convert_ticks(duration, presentation.time_base) {
        return presentation
            .ticks
            .checked_add(duration_ticks)
            .map(|ticks| MediaTime::new(ticks, presentation.time_base));
    }

    add_media_times_exact(presentation, duration)
}

fn is_positive_time_base(time_base: TimeBase) -> bool {
    (time_base.numerator > 0 && time_base.denominator > 0)
        || (time_base.numerator < 0 && time_base.denominator < 0)
}

fn convert_ticks(value: MediaTime, target: TimeBase) -> Option<i64> {
    let numerator = i128::from(value.ticks)
        .checked_mul(i128::from(value.time_base.numerator))?
        .checked_mul(i128::from(target.denominator))?;
    let denominator = i128::from(value.time_base.denominator)
        .checked_mul(i128::from(target.numerator))?;
    if denominator == 0 || numerator % denominator != 0 {
        return None;
    }
    i64::try_from(numerator / denominator).ok()
}

fn add_media_times_exact(left: MediaTime, right: MediaTime) -> Option<MediaTime> {
    let (left_numerator, left_denominator) = media_time_fraction(left)?;
    let (right_numerator, right_denominator) = media_time_fraction(right)?;

    let numerator = left_numerator
        .checked_mul(right_denominator)?
        .checked_add(right_numerator.checked_mul(left_denominator)?)?;
    let denominator = left_denominator.checked_mul(right_denominator)?;
    let divisor = greatest_common_divisor(numerator, denominator);
    let reduced_numerator = numerator / divisor;
    let reduced_denominator = denominator / divisor;

    let ticks = i64::try_from(reduced_numerator).ok()?;
    let denominator = i64::try_from(reduced_denominator).ok()?;
    let time_base = TimeBase::new(1, denominator)?;
    Some(MediaTime::new(ticks, time_base))
}

fn media_time_fraction(value: MediaTime) -> Option<(i128, i128)> {
    let mut numerator = i128::from(value.ticks).checked_mul(i128::from(value.time_base.numerator))?;
    let mut denominator = i128::from(value.time_base.denominator);
    if denominator < 0 {
        numerator = numerator.checked_neg()?;
        denominator = denominator.checked_neg()?;
    }
    Some((numerator, denominator))
}

fn greatest_common_divisor(mut left: i128, mut right: i128) -> i128 {
    left = left.abs();
    right = right.abs();
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left.max(1)
}
