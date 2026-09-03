use std::io::{BufRead, BufReader, ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};

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
    time_base: TimeBase,
    decoder_child: Child,
    decoder_stdout: ChildStdout,
    timing_child: Child,
    timing_stdout: BufReader<ChildStdout>,
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

        let mut decoder_child = Command::new(&binaries.ffmpeg)
            .args(["-v", "error", "-i"])
            .arg(path)
            .args([
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

        let timing_spawn = Command::new(&binaries.ffprobe)
            .args([
                "-v",
                "error",
                "-select_streams",
                "v:0",
                "-show_frames",
                "-show_entries",
                "frame=best_effort_timestamp,pkt_duration",
                "-of",
                "compact=p=0:nk=0",
            ])
            .arg(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut timing_child = match timing_spawn {
            Ok(child) => child,
            Err(err) => {
                let _ = decoder_child.kill();
                let _ = decoder_child.wait();
                return Err(match err.kind() {
                    ErrorKind::NotFound => SceneDetectError::FrameSource(
                        FfmpegError::MissingFfprobe(binaries.ffprobe.clone()).to_string(),
                    ),
                    _ => SceneDetectError::FrameSource(err.to_string()),
                });
            }
        };
        let timing_stdout = timing_child.stdout.take().ok_or_else(|| {
            let _ = decoder_child.kill();
            let _ = decoder_child.wait();
            SceneDetectError::FrameSource("ffprobe timing stdout was unavailable".to_owned())
        })?;

        Ok(Self {
            metadata: probe.metadata,
            time_base: probe.time_base,
            decoder_child,
            decoder_stdout,
            timing_child,
            timing_stdout: BufReader::new(timing_stdout),
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
                    let _ = self.timing_child.wait();
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

    fn read_frame_timing(&mut self) -> Result<FrameTiming> {
        let mut line = String::new();
        loop {
            let bytes = self
                .timing_stdout
                .read_line(&mut line)
                .map_err(|err| SceneDetectError::FrameSource(err.to_string()))?;
            if bytes == 0 {
                return Err(SceneDetectError::FrameSource(
                    "ffprobe timing stream ended before the decoded frame stream".to_owned(),
                ));
            }
            let value = line.trim();
            if !value.is_empty() {
                return parse_frame_timing(value, self.time_base);
            }
            line.clear();
        }
    }
}

impl Drop for FfmpegFrameSource {
    fn drop(&mut self) {
        let _ = self.decoder_child.kill();
        let _ = self.timing_child.kill();
        let _ = self.decoder_child.wait();
        let _ = self.timing_child.wait();
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

fn parse_frame_timing(value: &str, time_base: TimeBase) -> Result<FrameTiming> {
    let mut presentation_time = None;
    let mut duration = None;

    for field in value.split('|') {
        let Some((name, raw_value)) = field.split_once('=') else {
            continue;
        };
        let parsed = raw_value.parse::<i64>().ok();
        match name {
            "best_effort_timestamp" => {
                presentation_time = parsed.map(|ticks| MediaTime::new(ticks, time_base));
            }
            "pkt_duration" => {
                duration = parsed.map(|ticks| MediaTime::new(ticks, time_base));
            }
            _ => {}
        }
    }

    Ok(FrameTiming {
        presentation_time,
        duration,
    })
}

fn parse_time_base(value: &str) -> Result<TimeBase> {
    let Some((numerator, denominator)) = value.split_once('/') else {
        return Err(SceneDetectError::FrameSource(format!(
            "invalid ffprobe time base: {value}"
        )));
    };
    let numerator = numerator.parse::<i64>().map_err(|_| {
        SceneDetectError::FrameSource(format!("invalid ffprobe time base: {value}"))
    })?;
    let denominator = denominator.parse::<i64>().map_err(|_| {
        SceneDetectError::FrameSource(format!("invalid ffprobe time base: {value}"))
    })?;
    TimeBase::new(numerator, denominator).ok_or_else(|| {
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
    fn parses_stream_time_base_and_frame_timing() {
        let time_base = parse_time_base("1/1000").unwrap();
        assert_eq!(time_base, TimeBase::new(1, 1000).unwrap());
        let timing = parse_frame_timing(
            "best_effort_timestamp=125|pkt_duration=40",
            time_base,
        )
        .unwrap();
        assert_eq!(timing.presentation_time.unwrap().ticks, 125);
        assert_eq!(timing.duration.unwrap().ticks, 40);
        assert!((timing.presentation_time.unwrap().seconds() - 0.125).abs() < 1.0e-12);
    }
}
