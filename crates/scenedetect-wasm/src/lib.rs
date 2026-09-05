use std::cell::RefCell;

use scenedetect_core::{
    write_scene_events_ndjson, write_scene_list_csv, write_scene_list_html, write_scene_list_json,
    write_stats_csv, AdaptiveDetectorConfig, ContentDetectorConfig, ContentWeights,
    DetectionOptions, DetectionResult, DetectionSession, DetectorConfig, Frame, FrameIndex,
    FrameRate, HashDetectorConfig, HistogramDetectorConfig, MinSceneLenPolicy,
    ThresholdDetectorConfig,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

const OK: i32 = 0;
const ERROR: i32 = -1;

thread_local! {
    static SESSIONS: RefCell<Vec<Option<DetectionSession>>> = RefCell::new(Vec::new());
    static LAST_RESULT: RefCell<Vec<u8>> = RefCell::new(Vec::new());
    static LAST_ERROR: RefCell<Vec<u8>> = RefCell::new(Vec::new());
}

type BrowserResult<T> = std::result::Result<T, String>;

#[derive(Debug, Deserialize)]
struct BrowserConfig {
    detector: String,
    min_scene_len: Option<u64>,
    min_scene_len_policy: Option<String>,
    threshold: Option<f64>,
    luma_only: Option<bool>,
    weights: Option<BrowserWeights>,
    min_content_val: Option<f64>,
    frame_window: Option<usize>,
    fade_bias: Option<f64>,
    add_last_scene: Option<bool>,
    bins: Option<usize>,
    size: Option<usize>,
    lowpass: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct BrowserWeights {
    hue: Option<f64>,
    saturation: Option<f64>,
    luminance: Option<f64>,
    edges: Option<f64>,
}

impl BrowserConfig {
    fn options(&self) -> BrowserResult<DetectionOptions> {
        let mut options = DetectionOptions::default();
        if let Some(min_scene_len) = self.min_scene_len {
            options.min_scene_len = min_scene_len;
        }
        if let Some(policy) = self.min_scene_len_policy.as_deref() {
            options.min_scene_len_policy = match policy {
                "suppress" => MinSceneLenPolicy::Suppress,
                "merge_last" => MinSceneLenPolicy::MergeLast,
                other => return Err(format!("unsupported minimum scene length policy: {other}")),
            };
        }
        Ok(options)
    }

    fn detector(&self) -> BrowserResult<DetectorConfig> {
        match self.detector.as_str() {
            "content" => {
                let mut config = ContentDetectorConfig::default();
                if let Some(threshold) = self.threshold {
                    config.threshold = threshold;
                }
                if let Some(luma_only) = self.luma_only {
                    config.luma_only = luma_only;
                }
                apply_weights(&mut config.weights, self.weights.as_ref());
                Ok(DetectorConfig::Content(config))
            }
            "adaptive" => {
                let mut config = AdaptiveDetectorConfig::default();
                if let Some(threshold) = self.threshold {
                    config.threshold = threshold;
                }
                if let Some(min_content_val) = self.min_content_val {
                    config.min_content_val = min_content_val;
                }
                if let Some(frame_window) = self.frame_window {
                    if frame_window == 0 {
                        return Err("adaptive frame_window must be at least 1".to_owned());
                    }
                    config.frame_window = frame_window;
                }
                if let Some(luma_only) = self.luma_only {
                    config.luma_only = luma_only;
                }
                apply_weights(&mut config.weights, self.weights.as_ref());
                Ok(DetectorConfig::Adaptive(config))
            }
            "threshold" => {
                let mut config = ThresholdDetectorConfig::default();
                if let Some(threshold) = self.threshold {
                    config.threshold = threshold;
                }
                if let Some(fade_bias) = self.fade_bias {
                    config.fade_bias = fade_bias;
                }
                if let Some(add_last_scene) = self.add_last_scene {
                    config.add_last_scene = add_last_scene;
                }
                Ok(DetectorConfig::Threshold(config))
            }
            "histogram" => {
                let mut config = HistogramDetectorConfig::default();
                if let Some(threshold) = self.threshold {
                    config.threshold = threshold;
                }
                if let Some(bins) = self.bins {
                    if bins == 0 {
                        return Err("histogram bins must be at least 1".to_owned());
                    }
                    config.bins = bins;
                }
                Ok(DetectorConfig::Histogram(config))
            }
            "hash" => {
                let mut config = HashDetectorConfig::default();
                if let Some(threshold) = self.threshold {
                    config.threshold = threshold;
                }
                if let Some(size) = self.size {
                    if size == 0 {
                        return Err("hash size must be at least 1".to_owned());
                    }
                    config.size = size;
                }
                if let Some(lowpass) = self.lowpass {
                    if lowpass == 0 {
                        return Err("hash lowpass must be at least 1".to_owned());
                    }
                    config.lowpass = lowpass;
                }
                Ok(DetectorConfig::Hash(config))
            }
            other => Err(format!("unsupported detector: {other}")),
        }
    }
}

fn apply_weights(weights: &mut ContentWeights, overrides: Option<&BrowserWeights>) {
    let Some(overrides) = overrides else {
        return;
    };
    if let Some(value) = overrides.hue {
        weights.hue = value;
    }
    if let Some(value) = overrides.saturation {
        weights.saturation = value;
    }
    if let Some(value) = overrides.luminance {
        weights.luminance = value;
    }
    if let Some(value) = overrides.edges {
        weights.edges = value;
    }
}

#[derive(Serialize)]
struct BrowserOutput {
    detection: DetectionResult,
    exports: BrowserExports,
}

#[derive(Serialize)]
struct BrowserExports {
    scene_list_csv: String,
    scene_list_json: String,
    scene_events_ndjson: String,
    stats_csv: String,
    scene_list_html: String,
}

fn build_browser_output(detection: DetectionResult) -> BrowserResult<Vec<u8>> {
    let mut scene_list_csv = Vec::new();
    let mut scene_list_json = Vec::new();
    let mut scene_events_ndjson = Vec::new();
    let mut stats_csv = Vec::new();
    let mut scene_list_html = Vec::new();

    write_scene_list_csv(&detection.scene_list, &mut scene_list_csv)
        .map_err(|error| error.to_string())?;
    write_scene_list_json(&detection.scene_list, &mut scene_list_json)
        .map_err(|error| error.to_string())?;
    write_scene_events_ndjson(&detection.scene_list, &mut scene_events_ndjson)
        .map_err(|error| error.to_string())?;
    write_stats_csv(&detection.stats, &mut stats_csv).map_err(|error| error.to_string())?;
    write_scene_list_html(&detection.scene_list, &mut scene_list_html)
        .map_err(|error| error.to_string())?;

    let output = BrowserOutput {
        detection,
        exports: BrowserExports {
            scene_list_csv: String::from_utf8(scene_list_csv).map_err(|error| error.to_string())?,
            scene_list_json: String::from_utf8(scene_list_json)
                .map_err(|error| error.to_string())?,
            scene_events_ndjson: String::from_utf8(scene_events_ndjson)
                .map_err(|error| error.to_string())?,
            stats_csv: String::from_utf8(stats_csv).map_err(|error| error.to_string())?,
            scene_list_html: String::from_utf8(scene_list_html)
                .map_err(|error| error.to_string())?,
        },
    };
    serde_json::to_vec(&output).map_err(|error| error.to_string())
}

fn default_payload(detector: &str) -> BrowserResult<serde_json::Value> {
    let options = DetectionOptions::default();
    let common = json!({
        "min_scene_len": options.min_scene_len,
        "min_scene_len_policy": "suppress",
    });

    let value = match detector {
        "content" => {
            let config = ContentDetectorConfig::default();
            json!({
                "detector": "content",
                "min_scene_len": common["min_scene_len"],
                "min_scene_len_policy": common["min_scene_len_policy"],
                "threshold": config.threshold,
                "luma_only": config.luma_only,
                "weights": {
                    "hue": config.weights.hue,
                    "saturation": config.weights.saturation,
                    "luminance": config.weights.luminance,
                    "edges": config.weights.edges,
                }
            })
        }
        "adaptive" => {
            let config = AdaptiveDetectorConfig::default();
            json!({
                "detector": "adaptive",
                "min_scene_len": common["min_scene_len"],
                "min_scene_len_policy": common["min_scene_len_policy"],
                "threshold": config.threshold,
                "min_content_val": config.min_content_val,
                "frame_window": config.frame_window,
                "luma_only": config.luma_only,
                "weights": {
                    "hue": config.weights.hue,
                    "saturation": config.weights.saturation,
                    "luminance": config.weights.luminance,
                    "edges": config.weights.edges,
                }
            })
        }
        "threshold" => {
            let config = ThresholdDetectorConfig::default();
            json!({
                "detector": "threshold",
                "min_scene_len": common["min_scene_len"],
                "min_scene_len_policy": common["min_scene_len_policy"],
                "threshold": config.threshold,
                "fade_bias": config.fade_bias,
                "add_last_scene": config.add_last_scene,
            })
        }
        "histogram" => {
            let config = HistogramDetectorConfig::default();
            json!({
                "detector": "histogram",
                "min_scene_len": common["min_scene_len"],
                "min_scene_len_policy": common["min_scene_len_policy"],
                "threshold": config.threshold,
                "bins": config.bins,
            })
        }
        "hash" => {
            let config = HashDetectorConfig::default();
            json!({
                "detector": "hash",
                "min_scene_len": common["min_scene_len"],
                "min_scene_len_policy": common["min_scene_len_policy"],
                "threshold": config.threshold,
                "size": config.size,
                "lowpass": config.lowpass,
            })
        }
        other => return Err(format!("unsupported detector: {other}")),
    };
    Ok(value)
}

fn copy_input(ptr: *const u8, len: usize) -> BrowserResult<Vec<u8>> {
    if len == 0 {
        return Ok(Vec::new());
    }
    if ptr.is_null() {
        return Err("input pointer is null".to_owned());
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    Ok(bytes.to_vec())
}

fn set_result(bytes: Vec<u8>) {
    LAST_RESULT.with(|result| *result.borrow_mut() = bytes);
    LAST_ERROR.with(|error| error.borrow_mut().clear());
}

fn set_error(message: impl Into<String>) {
    LAST_ERROR.with(|error| *error.borrow_mut() = message.into().into_bytes());
    LAST_RESULT.with(|result| result.borrow_mut().clear());
}

fn insert_session(session: DetectionSession) -> u32 {
    SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        if let Some((index, slot)) = sessions
            .iter_mut()
            .enumerate()
            .find(|(_, slot)| slot.is_none())
        {
            *slot = Some(session);
            return index as u32 + 1;
        }
        sessions.push(Some(session));
        sessions.len() as u32
    })
}

fn mutate_session<T>(
    handle: u32,
    operation: impl FnOnce(&mut DetectionSession) -> BrowserResult<T>,
) -> BrowserResult<T> {
    if handle == 0 {
        return Err("invalid session handle".to_owned());
    }
    SESSIONS.with(|sessions| {
        let mut sessions = sessions.borrow_mut();
        let session = sessions
            .get_mut(handle as usize - 1)
            .and_then(Option::as_mut)
            .ok_or_else(|| "unknown or finished session handle".to_owned())?;
        operation(session)
    })
}

fn take_session(handle: u32) -> BrowserResult<DetectionSession> {
    if handle == 0 {
        return Err("invalid session handle".to_owned());
    }
    SESSIONS.with(|sessions| {
        sessions
            .borrow_mut()
            .get_mut(handle as usize - 1)
            .and_then(Option::take)
            .ok_or_else(|| "unknown or finished session handle".to_owned())
    })
}

#[no_mangle]
pub extern "C" fn scenedetect_abi_version() -> u32 {
    1
}

#[no_mangle]
pub extern "C" fn scenedetect_alloc(len: usize) -> *mut u8 {
    if len == 0 {
        return std::ptr::null_mut();
    }
    let mut buffer = Vec::<u8>::with_capacity(len);
    let ptr = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    ptr
}

#[no_mangle]
pub extern "C" fn scenedetect_dealloc(ptr: *mut u8, len: usize) {
    if ptr.is_null() || len == 0 {
        return;
    }
    unsafe {
        drop(Vec::from_raw_parts(ptr, 0, len));
    }
}

#[no_mangle]
pub extern "C" fn scenedetect_result_ptr() -> *const u8 {
    LAST_RESULT.with(|result| result.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn scenedetect_result_len() -> usize {
    LAST_RESULT.with(|result| result.borrow().len())
}

#[no_mangle]
pub extern "C" fn scenedetect_error_ptr() -> *const u8 {
    LAST_ERROR.with(|error| error.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn scenedetect_error_len() -> usize {
    LAST_ERROR.with(|error| error.borrow().len())
}

#[no_mangle]
pub extern "C" fn scenedetect_defaults(detector_ptr: *const u8, detector_len: usize) -> i32 {
    let result = (|| -> BrowserResult<Vec<u8>> {
        let bytes = copy_input(detector_ptr, detector_len)?;
        let detector = std::str::from_utf8(&bytes).map_err(|error| error.to_string())?;
        let payload = default_payload(detector)?;
        serde_json::to_vec(&payload).map_err(|error| error.to_string())
    })();

    match result {
        Ok(bytes) => {
            set_result(bytes);
            OK
        }
        Err(error) => {
            set_error(error);
            ERROR
        }
    }
}

#[no_mangle]
pub extern "C" fn scenedetect_session_new(
    config_ptr: *const u8,
    config_len: usize,
    frame_rate: f64,
) -> u32 {
    let result = (|| -> BrowserResult<DetectionSession> {
        if !frame_rate.is_finite() || frame_rate <= 0.0 {
            return Err("frame rate must be a positive finite number".to_owned());
        }
        let bytes = copy_input(config_ptr, config_len)?;
        let config: BrowserConfig =
            serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
        Ok(DetectionSession::new(
            config.detector()?,
            FrameRate(frame_rate),
            config.options()?,
        ))
    })();

    match result {
        Ok(session) => {
            LAST_ERROR.with(|error| error.borrow_mut().clear());
            insert_session(session)
        }
        Err(error) => {
            set_error(error);
            0
        }
    }
}

#[no_mangle]
pub extern "C" fn scenedetect_session_push(
    handle: u32,
    index: u32,
    width: u32,
    height: u32,
    rgb_ptr: *const u8,
    rgb_len: usize,
) -> i32 {
    let result = (|| -> BrowserResult<()> {
        if width == 0 || height == 0 {
            return Err("frame dimensions must be non-zero".to_owned());
        }
        let expected_len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(3))
            .ok_or_else(|| "frame dimensions overflow RGB buffer size".to_owned())?;
        if rgb_len != expected_len {
            return Err(format!(
                "RGB buffer length mismatch: expected {expected_len}, received {rgb_len}"
            ));
        }
        let rgb = copy_input(rgb_ptr, rgb_len)?;
        mutate_session(handle, |session| {
            session
                .push_frame(Frame {
                    index: FrameIndex(index as u64),
                    width,
                    height,
                    rgb,
                })
                .map_err(|error| error.to_string())
        })
    })();

    match result {
        Ok(()) => {
            LAST_ERROR.with(|error| error.borrow_mut().clear());
            OK
        }
        Err(error) => {
            set_error(error);
            ERROR
        }
    }
}

#[no_mangle]
pub extern "C" fn scenedetect_session_finish(handle: u32) -> i32 {
    let result = (|| -> BrowserResult<Vec<u8>> {
        let session = take_session(handle)?;
        let detection = session.finish().map_err(|error| error.to_string())?;
        build_browser_output(detection)
    })();

    match result {
        Ok(bytes) => {
            set_result(bytes);
            OK
        }
        Err(error) => {
            set_error(error);
            ERROR
        }
    }
}

#[no_mangle]
pub extern "C" fn scenedetect_session_drop(handle: u32) -> i32 {
    match take_session(handle) {
        Ok(_) => {
            LAST_ERROR.with(|error| error.borrow_mut().clear());
            OK
        }
        Err(error) => {
            set_error(error);
            ERROR
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_derived_from_core_detector_defaults() {
        let content = default_payload("content").unwrap();
        assert_eq!(
            content["threshold"],
            ContentDetectorConfig::default().threshold
        );
        assert_eq!(
            content["min_scene_len"],
            DetectionOptions::default().min_scene_len
        );

        let hash = default_payload("hash").unwrap();
        assert_eq!(hash["size"], HashDetectorConfig::default().size);
        assert_eq!(hash["lowpass"], HashDetectorConfig::default().lowpass);
    }

    #[test]
    fn browser_config_rejects_invalid_adaptive_window() {
        let config: BrowserConfig = serde_json::from_value(json!({
            "detector": "adaptive",
            "frame_window": 0
        }))
        .unwrap();
        assert!(config.detector().is_err());
    }
}
