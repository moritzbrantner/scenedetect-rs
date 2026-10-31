use std::collections::BTreeMap;
use std::io::Write;

use serde::{Deserialize, Serialize};

use crate::{ContentDetectorConfig, DetectionOptions, FrameIndex, FrameRate, Result};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DetectionStats {
    pub metric_names: Vec<String>,
    pub rows: Vec<StatsRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatsRow {
    pub frame: FrameIndex,
    pub metrics: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionStatsDecision {
    NotEvaluated,
    Accepted,
    SuppressedMinSceneLen,
    BelowThreshold,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RichDetectionStatsRow {
    pub frame: FrameIndex,
    pub score: f64,
    pub threshold: f64,
    pub decision: DetectionStatsDecision,
    pub metrics: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentDetectionStats {
    pub frame_rate: FrameRate,
    pub total_frames: u64,
    pub detector_config: ContentDetectorConfig,
    pub options: DetectionOptions,
    pub metric_names: Vec<String>,
    pub rows: Vec<RichDetectionStatsRow>,
}

pub trait DetectionStatsSink {
    fn start(&mut self, metric_names: &[String]) -> Result<()>;
    fn row(&mut self, row: StatsRow) -> Result<()>;
    fn finish(&mut self) -> Result<()>;
}

#[derive(Debug, Default)]
pub struct NoopStatsSink;

impl DetectionStatsSink for NoopStatsSink {
    fn start(&mut self, _metric_names: &[String]) -> Result<()> {
        Ok(())
    }

    fn row(&mut self, _row: StatsRow) -> Result<()> {
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        Ok(())
    }
}

pub struct CsvStatsSink<W: Write> {
    csv: csv::Writer<W>,
    metric_names: Vec<String>,
}

impl<W: Write> CsvStatsSink<W> {
    pub fn new(writer: W) -> Self {
        Self {
            csv: csv::Writer::from_writer(writer),
            metric_names: Vec::new(),
        }
    }
}

impl<W: Write> DetectionStatsSink for CsvStatsSink<W> {
    fn start(&mut self, metric_names: &[String]) -> Result<()> {
        self.metric_names = metric_names.to_vec();
        let mut header = vec!["Frame Number".to_owned()];
        header.extend(self.metric_names.iter().cloned());
        self.csv.write_record(header)?;
        Ok(())
    }

    fn row(&mut self, row: StatsRow) -> Result<()> {
        let mut record = vec![row.frame.0.to_string()];
        for metric in &self.metric_names {
            record.push(format!(
                "{:.6}",
                row.metrics.get(metric).copied().unwrap_or(0.0)
            ));
        }
        self.csv.write_record(record)?;
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        self.csv.flush()?;
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct CollectingStatsSink {
    stats: DetectionStats,
}

impl CollectingStatsSink {
    pub(crate) fn into_stats(self) -> DetectionStats {
        self.stats
    }
}

impl Default for CollectingStatsSink {
    fn default() -> Self {
        Self {
            stats: DetectionStats {
                metric_names: Vec::new(),
                rows: Vec::new(),
            },
        }
    }
}

impl DetectionStatsSink for CollectingStatsSink {
    fn start(&mut self, metric_names: &[String]) -> Result<()> {
        self.stats.metric_names = metric_names.to_vec();
        self.stats.rows.clear();
        Ok(())
    }

    fn row(&mut self, row: StatsRow) -> Result<()> {
        self.stats.rows.push(row);
        Ok(())
    }

    fn finish(&mut self) -> Result<()> {
        Ok(())
    }
}
