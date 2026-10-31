use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MinSceneLenPolicy {
    Suppress,
    MergeLast,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DetectionOptions {
    pub min_scene_len: u64,
    pub min_scene_len_policy: MinSceneLenPolicy,
}

impl Default for DetectionOptions {
    fn default() -> Self {
        Self {
            min_scene_len: 15,
            min_scene_len_policy: MinSceneLenPolicy::Suppress,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentDetectorConfig {
    pub threshold: f64,
    pub weights: ContentWeights,
    pub luma_only: bool,
}

impl Default for ContentDetectorConfig {
    fn default() -> Self {
        Self {
            threshold: 27.0,
            weights: ContentWeights::default(),
            luma_only: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdaptiveDetectorConfig {
    pub threshold: f64,
    pub min_content_val: f64,
    pub frame_window: usize,
    pub weights: ContentWeights,
    pub luma_only: bool,
}

impl Default for AdaptiveDetectorConfig {
    fn default() -> Self {
        Self {
            threshold: 3.0,
            min_content_val: 15.0,
            frame_window: 2,
            weights: ContentWeights::default(),
            luma_only: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThresholdDetectorConfig {
    pub threshold: f64,
    pub fade_bias: f64,
    pub add_last_scene: bool,
}

impl Default for ThresholdDetectorConfig {
    fn default() -> Self {
        Self {
            threshold: 12.0,
            fade_bias: 0.0,
            add_last_scene: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HistogramDetectorConfig {
    pub threshold: f64,
    pub bins: usize,
}

impl Default for HistogramDetectorConfig {
    fn default() -> Self {
        Self {
            threshold: 0.05,
            bins: 256,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HashDetectorConfig {
    pub threshold: f64,
    pub size: usize,
    pub lowpass: usize,
}

impl Default for HashDetectorConfig {
    fn default() -> Self {
        Self {
            threshold: 0.395,
            size: 16,
            lowpass: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentWeights {
    pub hue: f64,
    pub saturation: f64,
    pub luminance: f64,
    pub edges: f64,
}

impl Default for ContentWeights {
    fn default() -> Self {
        Self {
            hue: 1.0,
            saturation: 1.0,
            luminance: 1.0,
            edges: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DetectorConfig {
    Content(ContentDetectorConfig),
    Adaptive(AdaptiveDetectorConfig),
    Threshold(ThresholdDetectorConfig),
    Histogram(HistogramDetectorConfig),
    Hash(HashDetectorConfig),
}

pub trait Detector {
    fn config(&self) -> DetectorConfig;
}

impl Detector for DetectorConfig {
    fn config(&self) -> DetectorConfig {
        self.clone()
    }
}
