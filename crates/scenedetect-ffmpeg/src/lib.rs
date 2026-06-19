use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout, Command, Stdio};

use scenedetect_core::{Frame, FrameIndex, FrameRate, FrameSource, Result, SceneDetectError};
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
    child: Child,
    stdout: ChildStdout,
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
        let mut metadata = probe_video_with_binaries(path, &binaries)?;
        if let Some(frame_rate) = frame_rate_override {
            metadata.frame_rate = frame_rate;
        }

        let mut child = Command::new(&binaries.ffmpeg)
            .args(["-v", "error", "-i"])
            .arg(path)
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24", "pipe:1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| match err.kind() {
                ErrorKind::NotFound => SceneDetectError::FrameSource(
                    FfmpegError::MissingFfmpeg(binaries.ffmpeg.clone()).to_string(),
                ),
                _ => SceneDetectError::FrameSource(err.to_string()),
            })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            SceneDetectError::FrameSource("ffmpeg stdout was unavailable".to_owned())
        })?;

        Ok(Self {
            metadata,
            child,
            stdout,
            next_index: 0,
        })
    }

    pub fn metadata(&self) -> &VideoMetadata {
        &self.metadata
    }
}

impl FrameSource for FfmpegFrameSource {
    fn frame_rate(&self) -> FrameRate {
        self.metadata.frame_rate
    }

    fn next_frame(&mut self) -> Result<Option<Frame>> {
        let frame_bytes = self.metadata.width as usize * self.metadata.height as usize * 3;
        let mut rgb = vec![0; frame_bytes];
        let mut read = 0;

        while read < frame_bytes {
            match self.stdout.read(&mut rgb[read..]) {
                Ok(0) if read == 0 => {
                    let _ = self.child.wait();
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

        let frame = Frame {
            index: FrameIndex(self.next_index),
            width: self.metadata.width,
            height: self.metadata.height,
            rgb,
        };
        self.next_index += 1;
        Ok(Some(frame))
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
}

pub fn probe_video(path: impl AsRef<Path>) -> Result<VideoMetadata> {
    probe_video_with_binaries(path, &FfmpegBinaries::default())
}

fn probe_video_with_binaries(
    path: impl AsRef<Path>,
    binaries: &FfmpegBinaries,
) -> Result<VideoMetadata> {
    let path = path.as_ref();
    let output = Command::new(&binaries.ffprobe)
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate",
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

    Ok(VideoMetadata {
        width: stream.width,
        height: stream.height,
        frame_rate: parse_frame_rate(&stream.r_frame_rate)?,
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
}
