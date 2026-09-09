use std::cell::Cell;
use std::rc::Rc;

use super::*;
use crate::{Frame, FrameRate, FrameWithTiming, SceneSpan};

struct TimingFrameSource {
    frame_rate: FrameRate,
    frames: std::vec::IntoIter<FrameWithTiming>,
    rich_reads: Rc<Cell<usize>>,
    plain_reads: Rc<Cell<usize>>,
}

impl FrameSource for TimingFrameSource {
    fn frame_rate(&self) -> FrameRate {
        self.frame_rate
    }

    fn next_frame(&mut self) -> Result<Option<Frame>> {
        self.plain_reads.set(self.plain_reads.get() + 1);
        Err(SceneDetectError::FrameSource(
            "Scene Timeline must use the timing-aware source path".to_owned(),
        ))
    }

    fn next_frame_with_timing(&mut self) -> Result<Option<FrameWithTiming>> {
        self.rich_reads.set(self.rich_reads.get() + 1);
        Ok(self.frames.next())
    }
}

fn timed_frame(index: u64, pts: i64, duration: i64, time_base: TimeBase) -> FrameWithTiming {
    FrameWithTiming {
        frame: Frame::solid(index, 2, 2, [index as u8, index as u8, index as u8]),
        timing: FrameTiming {
            presentation_time: Some(MediaTime::new(pts, time_base)),
            duration: Some(MediaTime::new(duration, time_base)),
        },
    }
}

fn one_scene_timeline_end(presentation: MediaTime, duration: MediaTime) -> Option<MediaTime> {
    let source = TimingFrameSource {
        frame_rate: FrameRate(10.0),
        frames: vec![FrameWithTiming {
            frame: Frame::solid(0, 2, 2, [0, 0, 0]),
            timing: FrameTiming {
                presentation_time: Some(presentation),
                duration: Some(duration),
            },
        }]
        .into_iter(),
        rich_reads: Rc::new(Cell::new(0)),
        plain_reads: Rc::new(Cell::new(0)),
    };
    let scene_list = SceneList {
        frame_rate: FrameRate(10.0),
        scenes: vec![SceneSpan {
            start: FrameIndex(0),
            end: FrameIndex(1),
        }],
    };

    scene_timeline_from_source(&scene_list, source).unwrap().scenes[0].end_time
}

#[test]
fn timeline_preserves_exact_vfr_scene_endpoints_without_plain_frame_fallback() {
    let time_base = TimeBase::new(1, 1_000).unwrap();
    let rich_reads = Rc::new(Cell::new(0));
    let plain_reads = Rc::new(Cell::new(0));
    let source = TimingFrameSource {
        frame_rate: FrameRate(10.0),
        frames: vec![
            timed_frame(0, 0, 100, time_base),
            timed_frame(1, 100, 300, time_base),
            timed_frame(2, 400, 500, time_base),
            timed_frame(3, 900, 100, time_base),
        ]
        .into_iter(),
        rich_reads: Rc::clone(&rich_reads),
        plain_reads: Rc::clone(&plain_reads),
    };
    let scene_list = SceneList {
        frame_rate: FrameRate(10.0),
        scenes: vec![
            SceneSpan {
                start: FrameIndex(0),
                end: FrameIndex(2),
            },
            SceneSpan {
                start: FrameIndex(2),
                end: FrameIndex(4),
            },
        ],
    };

    let timeline = scene_timeline_from_source(&scene_list, source).unwrap();

    assert_eq!(timeline.scenes.len(), 2);
    assert_eq!(timeline.scenes[0].start, FrameIndex(0));
    assert_eq!(timeline.scenes[0].end, FrameIndex(2));
    assert_eq!(
        timeline.scenes[0].start_time,
        Some(MediaTime::new(0, time_base))
    );
    assert_eq!(
        timeline.scenes[0].end_time,
        Some(MediaTime::new(400, time_base))
    );
    assert_eq!(
        timeline.scenes[1].start_time,
        Some(MediaTime::new(400, time_base))
    );
    assert_eq!(
        timeline.scenes[1].end_time,
        Some(MediaTime::new(1_000, time_base))
    );
    assert_eq!(rich_reads.get(), 5, "four frames plus one EOF read");
    assert_eq!(
        plain_reads.get(),
        0,
        "timeline generation must opt into rich timing"
    );
}

#[test]
fn timeline_does_not_invent_final_media_time_when_duration_is_unknown() {
    let time_base = TimeBase::new(1, 1_000).unwrap();
    let source = TimingFrameSource {
        frame_rate: FrameRate(10.0),
        frames: vec![
            timed_frame(0, 0, 100, time_base),
            FrameWithTiming {
                frame: Frame::solid(1, 2, 2, [1, 1, 1]),
                timing: FrameTiming {
                    presentation_time: Some(MediaTime::new(100, time_base)),
                    duration: None,
                },
            },
        ]
        .into_iter(),
        rich_reads: Rc::new(Cell::new(0)),
        plain_reads: Rc::new(Cell::new(0)),
    };
    let scene_list = SceneList {
        frame_rate: FrameRate(10.0),
        scenes: vec![SceneSpan {
            start: FrameIndex(0),
            end: FrameIndex(2),
        }],
    };

    let timeline = scene_timeline_from_source(&scene_list, source).unwrap();

    assert_eq!(
        timeline.scenes[0].start_time,
        Some(MediaTime::new(0, time_base))
    );
    assert_eq!(timeline.scenes[0].end_time, None);
}

#[test]
fn timeline_adds_exact_duration_across_compatible_rational_bases() {
    let presentation_base = TimeBase::new(1, 10).unwrap();
    let duration_base = TimeBase::new(1, 1_000).unwrap();

    assert_eq!(
        one_scene_timeline_end(
            MediaTime::new(9, presentation_base),
            MediaTime::new(100, duration_base),
        ),
        Some(MediaTime::new(10, presentation_base))
    );
}

#[test]
fn timeline_uses_a_common_exact_base_when_direct_conversion_is_fractional() {
    assert_eq!(
        one_scene_timeline_end(
            MediaTime::new(1, TimeBase::new(1, 6).unwrap()),
            MediaTime::new(1, TimeBase::new(1, 4).unwrap()),
        ),
        Some(MediaTime::new(5, TimeBase::new(1, 12).unwrap()))
    );
}

#[test]
fn timeline_uses_common_base_when_direct_tick_addition_overflows() {
    let time_base = TimeBase::new(1, 1_000).unwrap();

    assert_eq!(
        one_scene_timeline_end(
            MediaTime::new(i64::MAX, time_base),
            MediaTime::new(1, time_base),
        ),
        Some(MediaTime::new(1_i64 << 60, TimeBase::new(1, 125).unwrap()))
    );
}
