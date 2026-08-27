use std::path::PathBuf;

use anyhow::Result;
use clap::Args;
use serde::Serialize;

use crate::native_stats::{self, DetectionStatsDetector, DetectionStatsInput};

#[derive(Debug, Args)]
pub struct InspectArgs {
    #[arg(short = 'i', long = "input")]
    input: PathBuf,
    #[arg(long = "json")]
    json: bool,
}

#[derive(Debug, Serialize)]
struct Inspection<'a> {
    detection_stats_path: String,
    detector: &'a DetectionStatsDetector,
    input: &'a DetectionStatsInput,
    scene_boundary_count: usize,
}

pub fn run(args: &InspectArgs) -> Result<()> {
    let (stats_path, document) = native_stats::read_detection_stats_document(&args.input)?;
    let scene_boundary_count = document.scene_list()?.scenes.len().saturating_sub(1);
    let inspection = Inspection {
        detection_stats_path: stats_path.display().to_string(),
        detector: &document.detector,
        input: &document.input,
        scene_boundary_count,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&inspection)?);
        return Ok(());
    }

    let detector = serde_json::to_value(&document.detector)?;
    println!("Detection Stats: {}", stats_path.display());
    println!("Detector: {}", document.detector.name());
    println!("Config: {}", serde_json::to_string(&detector["config"])?);
    println!("Input: {}", document.input.path);
    println!("Input bytes: {}", document.input.byte_len);
    println!("Frame rate: {}", document.input.frame_rate);
    println!("Frames: {}", document.input.total_frames);
    println!("Scene boundaries: {scene_boundary_count}");
    Ok(())
}
