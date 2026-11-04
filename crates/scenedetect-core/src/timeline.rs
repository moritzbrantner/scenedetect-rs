use media_core::{Timebase as CanonicalTimebase, Timestamp as CanonicalTimestamp};
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
    if duration.ticks < 0 {
        return None;
    }

    let canonical_presentation = canonical_timestamp(presentation)?;
    let canonical_duration = canonical_timestamp(duration)?;
    // Foundation owns the exact rescaling, addition, and overflow recovery.
    let end = canonical_presentation
        .checked_add_exact(canonical_duration)
        .ok()?;

    // Keep the published Scene Timeline wire shape, including its original
    // (possibly unreduced or doubly-negative) timebase when addition stayed
    // in the presentation base. Foundation's wire is deliberately different.
    let time_base = if end.timebase == canonical_presentation.timebase {
        presentation.time_base
    } else {
        TimeBase::new(i64::from(end.timebase.num), i64::from(end.timebase.den))?
    };
    Some(MediaTime::new(end.pts, time_base))
}

/// Translate the legacy i64 rational shape into Foundation's validated i32
/// timebase, reducing only the timebase ratio (not presentation ticks).
/// Values outside Foundation's exact representational domain have no endpoint.
fn canonical_timestamp(value: MediaTime) -> Option<CanonicalTimestamp> {
    let mut numerator = i128::from(value.time_base.numerator);
    let mut denominator = i128::from(value.time_base.denominator);
    if numerator == 0 || denominator == 0 || (numerator < 0) != (denominator < 0) {
        return None;
    }
    numerator = numerator.abs();
    denominator = denominator.abs();

    let mut divisor_left = numerator;
    let mut divisor_right = denominator;
    while divisor_right != 0 {
        (divisor_left, divisor_right) = (divisor_right, divisor_left % divisor_right);
    }
    let num = i32::try_from(numerator / divisor_left).ok()?;
    let den = i32::try_from(denominator / divisor_left).ok()?;
    let timebase = CanonicalTimebase::try_new(num, den).ok()?;
    CanonicalTimestamp::try_new(value.ticks, timebase).ok()
}
