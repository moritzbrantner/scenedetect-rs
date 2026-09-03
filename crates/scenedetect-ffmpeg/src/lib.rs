use std::io::{BufRead, BufReader, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};

use scenedetect_core::{
    Frame, FrameIndex, FrameRate, FrameSource, FrameTiming, FrameWithTiming, MediaTime, Result,
    SceneDetectError, TimeBase,
};
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FfmpegError {
    #[error(
        "ffmpeg executable was not found (tried {0}). Install FFmpeg and ensure ffmpeg is on PATH, or configure an explicit binary path"
    )]
    MissingFfmpeg(PathBuf),
    #[error(
        "ffprobe executable was not found (tried {0}). Install FFmpeg and ensure ffprobe is on PATH, or configure an explicit binary path"
    )]
    MissingFfprobe(PathBuf),
    #[error("ffprobe did not report a video stream for {0}")]
    MissingVideoStream(PathBuf),
    #[error("invalid ffprobe frame rate: {0}")]
    InvalidFrameRate(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct VideoMetadata {
    pub width: u32,
    pub height: u32,
    pub frame_rate: FrameRate,
}

pub struct FfmpegFrameSource {
    metadata: VideoMetadata,
    decoder_child: Child,
    decoder_stdout: ChildStdout,
    timing_receiver: Receiver<FrameTiming>,
    timing_thread: Option<JoinHandle<()>>,
    next_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegBinaries {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl Default for FfmpegBinaries {
    fn default() -> Self {
        Self {
            ffmpeg: PathBuf::from("ffmpeg"),
            ffprobe: PathBuf::from("ffprobe"),
        }
    }
}

impl FfmpegFrameSource {
    pub fn open(path: impl AsRef<Path>, frame_rate_override: Option<FrameRate>) -> Result<Self> {
        Self::open_with_binaries(path, frame_rate_override, FfmpegBinaries::default())
    }

    pub fn open_with_binaries(
        path: impl AsRef<Path>,
        frame_rate_override: Option<FrameRate>,
        binaries: FfmpegBinaries,
    ) -> Result<Self> {
        let path = path.as_ref();
        let mut probe = probe_video_details_with_binaries(path, &binaries)?;
        if let Some(frame_rate) = frame_rate_override {
            probe.metadata.frame_rate = frame_rate;
        }

        // `showinfo` is metadata-only and logs one line per decoded frame. Reading
        // those lines from the decoder's stderr gives us PTS/duration without a
        // second full-media ffprobe traversal. stderr is drained on a dedicated
        // thread so frame metadata cannot back up and block rawvideo stdout.
        let mut decoder_child = Command::new(&binaries.ffmpeg)
            .args(["-hide_banner", "-loglevel", "info", "-nostats", "-i"])
            .arg(path)
            .args([
                "-vf",
                "showinfo",
                "-fps_mode",
                "passthrough",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| match err.kind() {
                ErrorKind::NotFound => SceneDetectError::FrameSource(
                    FfmpegError::MissingFfmpeg(binaries.ffmpeg.clone()).to_string(),
                ),
                _ => SceneDetectError::FrameSource(err.to_string()),
            })?;

        let decoder_stdout = decoder_child.stdout.take().ok_or_else(|| {
            SceneDetectError::FrameSource("ffmpeg stdout was unavailable".to_owned())
        })?;
        let decoder_stderr = decoder_child.stderr.take().ok_or_else(|| {
            let _ = decoder_child.kill();
            let _ = decoder_child.wait();
            SceneDetectError::FrameSource("ffmpeg stderr was unavailable".to_owned())
        })?;

        let (timing_sender, timing_receiver) = mpsc::channel();
        let fallback_time_base = probe.time_base;
        let timing_thread = thread::spawn(move || {
            let reader = BufReader::new(decoder_stderr);
            let mut time_base = fallback_time_base;
            for line in reader.lines() {
                let Ok(line) = line else {
                    break;
                };
                if let Some(configured_time_base) = parse_showinfo_time_base(&line) {
                    time_base = configured_time_base;
                    continue;
                }
                if let Some(timing) = parse_showinfo_timing(&line, time_base) {
                    if timing_sender.send(timing).is_err() {
                        break;
                    }
                }
            }
        });

        Ok(Self {
            metadata: probe.metadata,
            decoder_child,
            decoder_stdout,
            timing_receiver,
            timing_thread: Some(timing_thread),
            next_index: 0,
        })
    }

    pub fn metadata(&self) -> &VideoMetadata {
        &self.metadata
    }

    fn read_frame_with_timing(&mut self) -> Result<Option<FrameWithTiming>> {
        let frame_bytes = self.metadata.width as usize * self.metadata.height as usize * 3;
        let mut rgb = vec![0; frame_bytes];
        let mut read = 0;

        while read < frame_bytes {
            match self.decoder_stdout.read(&mut rgb[read..]) {
                Ok(0) if read == 0 => {
                    let _ = self.decoder_child.wait();
                    self.join_timing_thread();
                    return Ok(None);
                }
                Ok(0) => {
                    return Err(SceneDetectError::FrameSource(
                        "ffmpeg ended in the middle of a frame".to_owned(),
                    ));
                }
                Ok(bytes) => read += bytes,
                Err(err) => return Err(SceneDetectError::FrameSource(err.to_string())),
            }
        }

        let timing = self.read_frame_timing()?;
        let frame = Frame {
            index: FrameIndex(self.next_index),
            width: self.metadata.width,
            height: self.metadata.height,
            rgb,
        };
        self.next_index += 1;
        Ok(Some(FrameWithTiming { frame, timing }))
    }

    fn read_frame_timing(&self) -> Result<FrameTiming> {
        self.timing_receiver.recv().map_err(|_| {
            SceneDetectError::FrameSource(
                "ffmpeg timing stream ended before the decoded frame stream".to_owned(),
            )
        })
    }

    fn join_timing_thread(&mut self) {
        if let Some(thread) = self.timing_thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for FfmpegFrameSource {
    fn drop(&mut self) {
        let _ = self.decoder_child.kill();
        let _ = self.decoder_child.wait();
        self.join_timing_thread();
    }
}

impl FrameSource for FfmpegFrameSource {
    fn frame_rate(&self) -> FrameRate {
        self.metadata.frame_rate
    }

    fn next_frame(&mut self) -> Result<Option<Frame>> {
        Ok(self
            .read_frame_with_timing()?
            .map(|frame_with_timing| frame_with_timing.frame))
    }

    fn next_frame_with_timing(&mut self) -> Result<Option<FrameWithTiming>> {
        self.read_frame_with_timing()
    }
}

#[derive(Debug, Deserialize)]
struct ProbeOutput {
    streams: Vec<ProbeStream>,
}

#[derive(Debug, Deserialize)]
struct ProbeStream {
    width: u32,
    height: u32,
    r_frame_rate: String,
    time_base: String,
}

struct ProbedVideo {
    metadata: VideoMetadata,
    time_base: TimeBase,
}

pub fn probe_video(path: impl AsRef<Path>) -> Result<VideoMetadata> {
    probe_video_with_binaries(path, &FfmpegBinaries::default())
}

fn probe_video_with_binaries(
    path: impl AsRef<Path>,
    binaries: &FfmpegBinaries,
) -> Result<VideoMetadata> {
    Ok(probe_video_details_with_binaries(path, binaries)?.metadata)
}

fn probe_video_details_with_binaries(
    path: impl AsRef<Path>,
    binaries: &FfmpegBinaries,
) -> Result<ProbedVideo> {
    let path = path.as_ref();
    let output = Command::new(&binaries.ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate,time_base",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|err| match err.kind() {
            ErrorKind::NotFound => SceneDetectError::FrameSource(
                FfmpegError::MissingFfprobe(binaries.ffprobe.clone()).to_string(),
            ),
            _ => SceneDetectError::FrameSource(err.to_string()),
        })?;

    if !output.status.success() {
        return Err(SceneDetectError::FrameSource(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }

    let probe: ProbeOutput = serde_json::from_slice(&output.stdout)?;
    let stream = probe.streams.into_iter().next().ok_or_else(|| {
        SceneDetectError::FrameSource(
            FfmpegError::MissingVideoStream(path.to_path_buf()).to_string(),
        )
    })?;

    Ok(ProbedVideo {
        metadata: VideoMetadata {
            width: stream.width,
            height: stream.height,
            frame_rate: parse_frame_rate(&stream.r_frame_rate)?,
        },
        time_base: parse_time_base(&stream.time_base)?,
    })
}

fn parse_showinfo_time_base(line: &str) -> Option<TimeBase> {
    let marker = "config in time_base:";
    let start = line.find(marker)? + marker.len();
    let value = line[start..]
        .trim_start()
        .split_whitespace()
        .next()?
        .trim_end_matches(',');
    parse_time_base_components(value)
}

fn parse_showinfo_timing(line: &str, time_base: TimeBase) -> Option<FrameTiming> {
    // showinfo also logs configuration lines. Requiring the per-frame n/pts
    // fields keeps exactly one timing message aligned with each raw RGB frame.
    if !line.contains("showinfo") || !line.contains(" n:") || !line.contains(" pts:") {
        return None;
    }

    Some(FrameTiming {
        presentation_time: parse_showinfo_integer(line, "pts:")
            .map(|ticks| MediaTime::new(ticks, time_base)),
        duration: parse_showinfo_integer(line, "duration:")
            .map(|ticks| MediaTime::new(ticks, time_base)),
    })
}

fn parse_showinfo_integer(line: &str, field: &str) -> Option<i64> {
    let start = line.find(field)? + field.len();
    line[start..]
        .trim_start()
        .split_whitespace()
        .next()?
        .parse::<i64>()
        .ok()
}

fn parse_time_base_components(value: &str) -> Option<TimeBase> {
    let (numerator, denominator) = value.split_once('/')?;
    TimeBase::new(numerator.parse().ok()?, denominator.parse().ok()?)
}

fn parse_time_base(value: &str) -> Result<TimeBase> {
    parse_time_base_components(value).ok_or_else(|| {
        SceneDetectError::FrameSource(format!("invalid ffprobe time base: {value}"))
    })
}

fn parse_frame_rate(value: &str) -> Result<FrameRate> {
    if let Some((numerator, denominator)) = value.split_once('/') {
        let numerator = numerator.parse::<f64>().map_err(|_| {
            SceneDetectError::FrameSource(
                FfmpegError::InvalidFrameRate(value.to_owned()).to_string(),
            )
        })?;
        let denominator = denominator.parse::<f64>().map_err(|_| {
            SceneDetectError::FrameSource(
                FfmpegError::InvalidFrameRate(value.to_owned()).to_string(),
            )
        })?;
        if denominator == 0.0 {
            return Err(SceneDetectError::FrameSource(
                FfmpegError::InvalidFrameRate(value.to_owned()).to_string(),
            ));
        }
        return Ok(FrameRate(numerator / denominator));
    }

    value.parse::<f64>().map(FrameRate).map_err(|_| {
        SceneDetectError::FrameSource(FfmpegError::InvalidFrameRate(value.to_owned()).to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fractional_frame_rate() {
        assert_eq!(parse_frame_rate("30000/1001").unwrap().0, 30000.0 / 1001.0);
        assert_eq!(parse_frame_rate("25").unwrap().0, 25.0);
    }

    #[test]
    fn parses_showinfo_time_base_and_frame_timing() {
        let configured = "[Parsed_showinfo_0 @ 0x1234] config in time_base: 1/1000, frame_rate: 0/0";
        let time_base = parse_showinfo_time_base(configured).unwrap();
        assert_eq!(time_base, TimeBase::new(1, 1000).unwrap());

        let line = "[Parsed_showinfo_0 @ 0x1234] n:   1 pts:    125 pts_time:0.125 duration:     40 duration_time:0.04 fmt:rgb24";
        let timing = parse_showinfo_timing(line, time_base).unwrap();
        assert_eq!(timing.presentation_time.unwrap().ticks, 125);
        assert_eq!(timing.duration.unwrap().ticks, 40);
        assert!((timing.presentation_time.unwrap().seconds() - 0.125).abs() < 1.0e-12);
    }
}
