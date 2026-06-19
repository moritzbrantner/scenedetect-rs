use std::collections::BTreeMap;
use std::io::Write;

use serde::Serialize;

use crate::{
    BoundaryCandidateReview, BoundaryCandidateStatus, BoundaryReview, DetectionStats, FrameIndex,
    FrameRate, Result, SceneList, Timecode,
};

pub fn write_scene_list_csv<W: Write>(scene_list: &SceneList, writer: W) -> Result<()> {
    let mut csv = csv::WriterBuilder::new().flexible(true).from_writer(writer);
    let mut timecode_list = vec!["Timecode List:".to_owned()];
    timecode_list.extend(
        scene_list.scenes.iter().skip(1).map(|scene| {
            Timecode::from_frames(scene.start.0).display_at_rate(scene_list.frame_rate)
        }),
    );
    csv.write_record(timecode_list)?;
    csv.write_record([
        "Scene Number",
        "Start Frame",
        "Start Timecode",
        "Start Time (seconds)",
        "End Frame",
        "End Timecode",
        "End Time (seconds)",
        "Length (frames)",
        "Length (timecode)",
        "Length (seconds)",
    ])?;

    for (idx, scene) in scene_list.scenes.iter().enumerate() {
        let length = scene.end.0.saturating_sub(scene.start.0);
        csv.write_record([
            (idx + 1).to_string(),
            (scene.start.0 + 1).to_string(),
            Timecode::from_frames(scene.start.0).display_at_rate(scene_list.frame_rate),
            seconds_at_rate(scene.start.0, scene_list.frame_rate),
            scene.end.0.to_string(),
            Timecode::from_frames(scene.end.0).display_at_rate(scene_list.frame_rate),
            seconds_at_rate(scene.end.0, scene_list.frame_rate),
            length.to_string(),
            Timecode::from_frames(length).display_at_rate(scene_list.frame_rate),
            seconds_at_rate(length, scene_list.frame_rate),
        ])?;
    }

    csv.flush()?;
    Ok(())
}

pub fn write_stats_csv<W: Write>(stats: &DetectionStats, writer: W) -> Result<()> {
    let mut csv = csv::Writer::from_writer(writer);
    let mut header = vec!["Frame Number".to_owned()];
    header.extend(stats.metric_names.iter().cloned());
    csv.write_record(header)?;

    for row in &stats.rows {
        let mut record = vec![row.frame.0.to_string()];
        for metric in &stats.metric_names {
            record.push(format!(
                "{:.6}",
                row.metrics.get(metric).copied().unwrap_or(0.0)
            ));
        }
        csv.write_record(record)?;
    }

    csv.flush()?;
    Ok(())
}

pub fn write_scene_list_json<W: Write>(scene_list: &SceneList, writer: W) -> Result<()> {
    let output = SceneListExport {
        frame_rate: scene_list.frame_rate.0,
        scene_count: scene_list.scenes.len(),
        scenes: scene_exports(scene_list),
    };
    serde_json::to_writer_pretty(writer, &output)?;
    Ok(())
}

pub fn write_scene_events_ndjson<W: Write>(scene_list: &SceneList, mut writer: W) -> Result<()> {
    for scene in scene_exports(scene_list) {
        let event = SceneEventExport {
            event: "scene",
            scene,
        };
        serde_json::to_writer(&mut writer, &event)?;
        writeln!(writer)?;
    }
    Ok(())
}

pub fn write_scene_list_html<W: Write>(scene_list: &SceneList, mut writer: W) -> Result<()> {
    writeln!(writer, "<!doctype html>")?;
    writeln!(writer, "<html lang=\"en\">")?;
    writeln!(writer, "<head>")?;
    writeln!(writer, "<meta charset=\"utf-8\">")?;
    writeln!(writer, "<title>Scene List</title>")?;
    writeln!(
        writer,
        "<style>body{{font-family:system-ui,sans-serif;margin:2rem;line-height:1.4}}table{{border-collapse:collapse;width:100%}}th,td{{border:1px solid #d0d7de;padding:0.35rem 0.5rem;text-align:right}}th{{background:#f6f8fa}}th:first-child,td:first-child{{text-align:left}}</style>"
    )?;
    writeln!(writer, "</head>")?;
    writeln!(writer, "<body>")?;
    writeln!(writer, "<h1>Scene List</h1>")?;
    writeln!(writer, "<p>Frame rate: {:.6}</p>", scene_list.frame_rate.0)?;
    writeln!(writer, "<p>Scene count: {}</p>", scene_list.scenes.len())?;
    writeln!(writer, "<table>")?;
    writeln!(writer, "<thead>")?;
    writeln!(
        writer,
        "<tr><th>Scene Number</th><th>Start Frame</th><th>Start Timecode</th><th>Start Seconds</th><th>End Frame</th><th>End Timecode</th><th>End Seconds</th><th>Length Frames</th><th>Length Timecode</th><th>Length Seconds</th></tr>"
    )?;
    writeln!(writer, "</thead>")?;
    writeln!(writer, "<tbody>")?;
    for scene in scene_exports(scene_list) {
        writeln!(
            writer,
            "<tr><td>{}</td><td>{}</td><td>{}</td><td>{:.6}</td><td>{}</td><td>{}</td><td>{:.6}</td><td>{}</td><td>{}</td><td>{:.6}</td></tr>",
            scene.scene_number,
            scene.start_frame,
            scene.start_timecode,
            scene.start_seconds,
            scene.end_frame,
            scene.end_timecode,
            scene.end_seconds,
            scene.length_frames,
            scene.length_timecode,
            scene.length_seconds,
        )?;
    }
    writeln!(writer, "</tbody>")?;
    writeln!(writer, "</table>")?;
    writeln!(writer, "</body>")?;
    writeln!(writer, "</html>")?;
    Ok(())
}

pub fn write_boundary_review_csv<W: Write>(review: &BoundaryReview, writer: W) -> Result<()> {
    let mut csv = csv::Writer::from_writer(writer);
    let mut header = vec![
        "Rank".to_owned(),
        "Status".to_owned(),
        "Boundary Candidate Number".to_owned(),
        "Boundary Frame".to_owned(),
        "Boundary Frame Index".to_owned(),
        "Boundary Timecode".to_owned(),
        "Boundary Seconds".to_owned(),
        "Score Metric".to_owned(),
        "Boundary Score".to_owned(),
        "Detector Threshold".to_owned(),
        "Review Threshold".to_owned(),
        "Threshold Distance".to_owned(),
        "Before Start Frame".to_owned(),
        "Before End Frame".to_owned(),
        "After Start Frame".to_owned(),
        "After End Frame".to_owned(),
    ];
    header.extend(review_metric_names(review));
    csv.write_record(header)?;

    for (rank, candidate) in review.candidates.iter().enumerate() {
        let mut record = vec![
            (rank + 1).to_string(),
            candidate.status.as_str().to_owned(),
            candidate.candidate_number.to_string(),
            (candidate.frame.0 + 1).to_string(),
            candidate.frame.0.to_string(),
            Timecode::from_frames(candidate.frame.0).display_at_rate(review.frame_rate),
            seconds_at_rate(candidate.frame.0, review.frame_rate),
            candidate.score_metric.clone(),
            format!("{:.6}", candidate.score),
            format!("{:.6}", candidate.detector_threshold),
            format!("{:.6}", candidate.review_threshold),
            format!("{:.6}", candidate.threshold_distance),
            review_start_frame(candidate.before.start),
            candidate.before.end.0.to_string(),
            review_start_frame(candidate.after.start),
            candidate.after.end.0.to_string(),
        ];
        for metric in review_metric_names(review) {
            record.push(format!(
                "{:.6}",
                candidate.metrics.get(&metric).copied().unwrap_or(0.0)
            ));
        }
        csv.write_record(record)?;
    }

    csv.flush()?;
    Ok(())
}

pub fn write_boundary_review_json<W: Write>(review: &BoundaryReview, writer: W) -> Result<()> {
    let output = BoundaryReviewExport {
        frame_rate: review.frame_rate.0,
        detector: review.detector.clone(),
        sort: "threshold_distance",
        score_metric: review.score_metric.clone(),
        detector_threshold: review.detector_threshold,
        review_threshold: review.review_threshold,
        candidate_count: review.candidates.len(),
        boundary_candidates: boundary_candidate_exports(review),
    };
    serde_json::to_writer_pretty(writer, &output)?;
    Ok(())
}

fn seconds_at_rate(frames: u64, frame_rate: FrameRate) -> String {
    format!("{:.6}", seconds_value_at_rate(frames, frame_rate))
}

fn seconds_value_at_rate(frames: u64, frame_rate: FrameRate) -> f64 {
    frames as f64 / frame_rate.0
}

fn review_metric_names(review: &BoundaryReview) -> Vec<String> {
    review
        .candidates
        .iter()
        .flat_map(|candidate| candidate.metrics.keys().cloned())
        .fold(Vec::new(), |mut names, name| {
            if !names.contains(&name) {
                names.push(name);
            }
            names
        })
}

fn review_start_frame(frame: FrameIndex) -> String {
    (frame.0 + 1).to_string()
}

#[derive(Debug, Serialize)]
struct SceneListExport {
    frame_rate: f64,
    scene_count: usize,
    scenes: Vec<SceneExport>,
}

#[derive(Debug, Serialize)]
struct SceneEventExport {
    event: &'static str,
    #[serde(flatten)]
    scene: SceneExport,
}

#[derive(Debug, Serialize)]
struct SceneExport {
    scene_number: usize,
    start_frame: u64,
    start_timecode: String,
    start_seconds: f64,
    end_frame: u64,
    end_timecode: String,
    end_seconds: f64,
    length_frames: u64,
    length_timecode: String,
    length_seconds: f64,
}

#[derive(Debug, Serialize)]
struct BoundaryReviewExport {
    frame_rate: f64,
    detector: String,
    sort: &'static str,
    score_metric: String,
    detector_threshold: f64,
    review_threshold: f64,
    candidate_count: usize,
    boundary_candidates: Vec<BoundaryCandidateExport>,
}

#[derive(Debug, Serialize)]
struct BoundaryCandidateExport {
    rank: usize,
    status: BoundaryCandidateStatus,
    boundary_candidate_number: usize,
    boundary_frame: u64,
    boundary_frame_index: u64,
    boundary_timecode: String,
    boundary_seconds: f64,
    score_metric: String,
    boundary_score: f64,
    detector_threshold: f64,
    review_threshold: f64,
    threshold_distance: f64,
    before: ReviewSceneContextExport,
    after: ReviewSceneContextExport,
    metrics: BTreeMap<String, f64>,
}

#[derive(Debug, Serialize)]
struct ReviewSceneContextExport {
    start_frame: u64,
    end_frame: u64,
}

fn scene_exports(scene_list: &SceneList) -> Vec<SceneExport> {
    scene_list
        .scenes
        .iter()
        .enumerate()
        .map(|(idx, scene)| {
            let length = scene.end.0.saturating_sub(scene.start.0);
            SceneExport {
                scene_number: idx + 1,
                start_frame: scene.start.0 + 1,
                start_timecode: Timecode::from_frames(scene.start.0)
                    .display_at_rate(scene_list.frame_rate),
                start_seconds: seconds_value_at_rate(scene.start.0, scene_list.frame_rate),
                end_frame: scene.end.0,
                end_timecode: Timecode::from_frames(scene.end.0)
                    .display_at_rate(scene_list.frame_rate),
                end_seconds: seconds_value_at_rate(scene.end.0, scene_list.frame_rate),
                length_frames: length,
                length_timecode: Timecode::from_frames(length)
                    .display_at_rate(scene_list.frame_rate),
                length_seconds: seconds_value_at_rate(length, scene_list.frame_rate),
            }
        })
        .collect()
}

fn boundary_candidate_exports(review: &BoundaryReview) -> Vec<BoundaryCandidateExport> {
    review
        .candidates
        .iter()
        .enumerate()
        .map(|(idx, candidate)| boundary_candidate_export(review, idx, candidate))
        .collect()
}

fn boundary_candidate_export(
    review: &BoundaryReview,
    idx: usize,
    candidate: &BoundaryCandidateReview,
) -> BoundaryCandidateExport {
    BoundaryCandidateExport {
        rank: idx + 1,
        status: candidate.status,
        boundary_candidate_number: candidate.candidate_number,
        boundary_frame: candidate.frame.0 + 1,
        boundary_frame_index: candidate.frame.0,
        boundary_timecode: Timecode::from_frames(candidate.frame.0)
            .display_at_rate(review.frame_rate),
        boundary_seconds: seconds_value_at_rate(candidate.frame.0, review.frame_rate),
        score_metric: candidate.score_metric.clone(),
        boundary_score: candidate.score,
        detector_threshold: candidate.detector_threshold,
        review_threshold: candidate.review_threshold,
        threshold_distance: candidate.threshold_distance,
        before: ReviewSceneContextExport {
            start_frame: candidate.before.start.0 + 1,
            end_frame: candidate.before.end.0,
        },
        after: ReviewSceneContextExport {
            start_frame: candidate.after.start.0 + 1,
            end_frame: candidate.after.end.0,
        },
        metrics: candidate.metrics.clone(),
    }
}
