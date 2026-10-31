use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::{Result, SceneDetectError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FrameIndex(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FrameRate(pub f64);

impl FrameRate {
    pub fn frames_from_seconds(self, seconds: f64) -> u64 {
        (seconds * self.0).round() as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timecode {
    frames: u64,
}

impl Timecode {
    pub fn from_frames(frames: u64) -> Self {
        Self { frames }
    }

    pub fn frames(self) -> u64 {
        self.frames
    }

    pub fn parse_at_rate(input: &str, frame_rate: FrameRate) -> Result<Self> {
        if input.contains(':') {
            let parts: Vec<_> = input.split(':').collect();
            if parts.len() != 3 {
                return Err(SceneDetectError::InvalidTimecode(input.to_owned()));
            }
            let hours = parts[0]
                .parse::<f64>()
                .map_err(|_| SceneDetectError::InvalidTimecode(input.to_owned()))?;
            let minutes = parts[1]
                .parse::<f64>()
                .map_err(|_| SceneDetectError::InvalidTimecode(input.to_owned()))?;
            let seconds = parts[2]
                .parse::<f64>()
                .map_err(|_| SceneDetectError::InvalidTimecode(input.to_owned()))?;
            return Ok(Self::from_frames(
                frame_rate.frames_from_seconds(hours * 3600.0 + minutes * 60.0 + seconds),
            ));
        }

        if let Some(seconds) = input.strip_suffix('s') {
            let seconds = seconds
                .parse::<f64>()
                .map_err(|_| SceneDetectError::InvalidTimecode(input.to_owned()))?;
            return Ok(Self::from_frames(frame_rate.frames_from_seconds(seconds)));
        }

        let frames = input
            .parse::<u64>()
            .map_err(|_| SceneDetectError::InvalidTimecode(input.to_owned()))?;
        Ok(Self::from_frames(frames))
    }

    pub fn display_at_rate(self, frame_rate: FrameRate) -> String {
        let total_seconds = self.frames as f64 / frame_rate.0;
        let hours = (total_seconds / 3600.0).floor() as u64;
        let minutes = ((total_seconds % 3600.0) / 60.0).floor() as u64;
        let seconds = total_seconds % 60.0;
        format!("{hours:02}:{minutes:02}:{seconds:06.3}")
    }
}

impl FromStr for Timecode {
    type Err = SceneDetectError;

    fn from_str(value: &str) -> Result<Self> {
        let frames = value
            .parse::<u64>()
            .map_err(|_| SceneDetectError::InvalidTimecode(value.to_owned()))?;
        Ok(Self::from_frames(frames))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub index: FrameIndex,
    pub width: u32,
    pub height: u32,
    pub rgb: Vec<u8>,
}

impl Frame {
    pub fn solid(index: u64, width: u32, height: u32, rgb: [u8; 3]) -> Self {
        let mut pixels = Vec::with_capacity(width as usize * height as usize * 3);
        for _ in 0..(width as usize * height as usize) {
            pixels.extend_from_slice(&rgb);
        }

        Self {
            index: FrameIndex(index),
            width,
            height,
            rgb: pixels,
        }
    }

    pub fn mean_luma(&self) -> f64 {
        let mut sum = 0.0;
        let mut count = 0_u64;
        for px in self.rgb.chunks_exact(3) {
            sum += 0.299 * px[0] as f64 + 0.587 * px[1] as f64 + 0.114 * px[2] as f64;
            count += 1;
        }
        if count == 0 {
            0.0
        } else {
            sum / count as f64
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeBase {
    pub numerator: i64,
    pub denominator: i64,
}

impl TimeBase {
    pub fn new(numerator: i64, denominator: i64) -> Option<Self> {
        if numerator == 0 || denominator == 0 {
            None
        } else {
            Some(Self {
                numerator,
                denominator,
            })
        }
    }

    pub fn seconds_per_tick(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaTime {
    pub ticks: i64,
    pub time_base: TimeBase,
}

impl MediaTime {
    pub fn new(ticks: i64, time_base: TimeBase) -> Self {
        Self { ticks, time_base }
    }

    pub fn seconds(self) -> f64 {
        self.ticks as f64 * self.time_base.seconds_per_tick()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameTiming {
    pub presentation_time: Option<MediaTime>,
    pub duration: Option<MediaTime>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameWithTiming {
    pub frame: Frame,
    pub timing: FrameTiming,
}

impl From<Frame> for FrameWithTiming {
    fn from(frame: Frame) -> Self {
        Self {
            frame,
            timing: FrameTiming::default(),
        }
    }
}

pub trait FrameSource {
    fn frame_rate(&self) -> FrameRate;
    fn next_frame(&mut self) -> Result<Option<Frame>>;

    fn next_frame_with_timing(&mut self) -> Result<Option<FrameWithTiming>> {
        Ok(self.next_frame()?.map(FrameWithTiming::from))
    }
}

impl fmt::Display for FrameIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
