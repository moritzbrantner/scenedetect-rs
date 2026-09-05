import { createSceneDetect } from "./scenedetect-wasm.js";

const videoFile = document.getElementById("video-file");
const video = document.getElementById("video-preview");
const canvas = document.getElementById("analysis-canvas");
const context = canvas.getContext("2d", { willReadFrequently: true });
const videoMeta = document.getElementById("video-meta");
const analysisFps = document.getElementById("analysis-fps");
const maxDimension = document.getElementById("max-dimension");
const detector = document.getElementById("detector");
const minSceneLen = document.getElementById("min-scene-len");
const minScenePolicy = document.getElementById("min-scene-policy");
const minSceneTime = document.getElementById("min-scene-time");
const detectorControls = document.getElementById("detector-controls");
const runButton = document.getElementById("run-analysis");
const cancelButton = document.getElementById("cancel-analysis");
const progress = document.getElementById("analysis-progress");
const status = document.getElementById("analysis-status");
const resultsSection = document.getElementById("results");
const resultSummary = document.getElementById("result-summary");
const sceneRows = document.getElementById("scene-rows");
const boundaryReview = document.getElementById("boundary-review");
const boundaryReviewSummary = document.getElementById("boundary-review-summary");
const boundaryRows = document.getElementById("boundary-rows");

const MAX_SAMPLES = 200_000;

const detectorFields = {
  content: [
    { key: "threshold", label: "Content threshold", step: "0.1", min: "0" },
    { key: "review_threshold", label: "Boundary review threshold", step: "0.1", min: "0" },
    { key: "luma_only", label: "Luma only", type: "checkbox" },
    { key: "weights.hue", label: "Hue weight", step: "0.1", min: "0" },
    { key: "weights.saturation", label: "Saturation weight", step: "0.1", min: "0" },
    { key: "weights.luminance", label: "Luminance weight", step: "0.1", min: "0" },
    { key: "weights.edges", label: "Edge weight", step: "0.1", min: "0" },
  ],
  adaptive: [
    { key: "threshold", label: "Adaptive ratio threshold", step: "0.1", min: "0" },
    { key: "review_threshold", label: "Boundary review threshold", step: "0.1", min: "0" },
    { key: "min_content_val", label: "Minimum content value", step: "0.1", min: "0" },
    { key: "frame_window", label: "Frame window", step: "1", min: "1" },
    { key: "luma_only", label: "Luma only", type: "checkbox" },
    { key: "weights.hue", label: "Hue weight", step: "0.1", min: "0" },
    { key: "weights.saturation", label: "Saturation weight", step: "0.1", min: "0" },
    { key: "weights.luminance", label: "Luminance weight", step: "0.1", min: "0" },
    { key: "weights.edges", label: "Edge weight", step: "0.1", min: "0" },
  ],
  threshold: [
    { key: "threshold", label: "Fade threshold", step: "0.1", min: "0" },
    { key: "fade_bias", label: "Fade bias", step: "0.05", min: "-1", max: "1" },
    { key: "add_last_scene", label: "Add final fade-out scene", type: "checkbox" },
  ],
  histogram: [
    { key: "threshold", label: "Histogram threshold", step: "0.01", min: "0" },
    { key: "bins", label: "Histogram bins", step: "1", min: "1" },
  ],
  hash: [
    { key: "threshold", label: "Hash distance threshold", step: "0.001", min: "0", max: "1" },
    { key: "size", label: "Hash size", step: "1", min: "1" },
    { key: "lowpass", label: "Low-pass factor", step: "1", min: "1" },
  ],
};

let sceneDetect = null;
let objectUrl = null;
let activeAbortController = null;
let running = false;
let currentOutput = null;
let currentResultFps = null;

function getPath(object, path) {
  return path.split(".").reduce((value, part) => value?.[part], object);
}

function setPath(object, path, value) {
  const parts = path.split(".");
  let target = object;
  for (const part of parts.slice(0, -1)) {
    target[part] ??= {};
    target = target[part];
  }
  target[parts.at(-1)] = value;
}

function formatDuration(seconds) {
  if (!Number.isFinite(seconds)) {
    return "unknown duration";
  }
  const minutes = Math.floor(seconds / 60);
  const remainder = seconds - minutes * 60;
  return `${minutes}:${remainder.toFixed(2).padStart(5, "0")}`;
}

function formatTime(seconds) {
  const safeSeconds = Math.max(0, seconds);
  const hours = Math.floor(safeSeconds / 3600);
  const minutes = Math.floor((safeSeconds % 3600) / 60);
  const secs = safeSeconds % 60;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${secs
    .toFixed(3)
    .padStart(6, "0")}`;
}

function formatCandidateStatus(value) {
  if (value === "accepted") {
    return "Accepted";
  }
  if (value === "suppressed_min_scene_len") {
    return "Suppressed by minimum scene length";
  }
  if (value === "near_miss") {
    return "Near miss";
  }
  return value;
}

function updateRunState() {
  runButton.disabled = running || !sceneDetect || !videoFile.files?.[0];
  cancelButton.disabled = !running;
  videoFile.disabled = running;
  detector.disabled = running;
}

function updateMinSceneTime() {
  const fps = Number(analysisFps.value);
  const frames = Number(minSceneLen.value);
  if (Number.isFinite(fps) && fps > 0 && Number.isFinite(frames) && frames >= 0) {
    minSceneTime.textContent = `At ${fps} fps: ${(frames / fps).toFixed(2)} seconds.`;
  } else {
    minSceneTime.textContent = "Choose a valid sampling rate and frame count.";
  }
}

function makeDetectorField(definition, defaults) {
  const label = document.createElement("label");
  const title = document.createElement("span");
  title.textContent = definition.label;
  label.append(title);

  const input = document.createElement("input");
  input.dataset.configKey = definition.key;
  const value = getPath(defaults, definition.key);
  if (definition.type === "checkbox") {
    input.type = "checkbox";
    input.checked = Boolean(value);
    label.classList.add("checkbox-field");
  } else {
    input.type = "number";
    input.value = String(value);
    input.step = definition.step ?? "any";
    if (definition.min !== undefined) {
      input.min = definition.min;
    }
    if (definition.max !== undefined) {
      input.max = definition.max;
    }
  }
  label.append(input);
  return label;
}

function renderDetectorControls({ resetCommon = false } = {}) {
  if (!sceneDetect) {
    return;
  }
  const defaults = sceneDetect.defaults(detector.value);
  detectorControls.replaceChildren(
    ...detectorFields[detector.value].map((definition) => makeDetectorField(definition, defaults)),
  );
  if (resetCommon) {
    minSceneLen.value = String(defaults.min_scene_len);
    minScenePolicy.value = defaults.min_scene_len_policy;
    updateMinSceneTime();
  }
}

function readDetectorConfig() {
  const config = sceneDetect.defaults(detector.value);
  config.min_scene_len = Number(minSceneLen.value);
  config.min_scene_len_policy = minScenePolicy.value;

  for (const input of detectorControls.querySelectorAll("input[data-config-key]")) {
    const value = input.type === "checkbox" ? input.checked : Number(input.value);
    if (input.type !== "checkbox" && !Number.isFinite(value)) {
      throw new Error(`Invalid numeric value for ${input.dataset.configKey}.`);
    }
    setPath(config, input.dataset.configKey, value);
  }
  return config;
}

function samplingConfig() {
  const fps = Number(analysisFps.value);
  const dimension = Number(maxDimension.value);
  if (!Number.isFinite(fps) || fps <= 0 || fps > 30) {
    throw new Error("Analysis frames per second must be between 0.5 and 30.");
  }
  if (!Number.isInteger(dimension) || dimension <= 0) {
    throw new Error("Maximum frame dimension is invalid.");
  }
  return { fps, dimension };
}

function abortError() {
  return new DOMException("Analysis cancelled", "AbortError");
}

function assertNotAborted(signal) {
  if (signal.aborted) {
    throw abortError();
  }
}

function waitForMediaEvent(target, eventName, signal) {
  return new Promise((resolve, reject) => {
    const cleanup = () => {
      target.removeEventListener(eventName, onEvent);
      target.removeEventListener("error", onError);
      signal?.removeEventListener("abort", onAbort);
    };
    const onEvent = () => {
      cleanup();
      resolve();
    };
    const onError = () => {
      cleanup();
      reject(new Error("The browser could not decode this video at the requested position."));
    };
    const onAbort = () => {
      cleanup();
      reject(abortError());
    };

    target.addEventListener(eventName, onEvent, { once: true });
    target.addEventListener("error", onError, { once: true });
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

async function ensureVideoData(signal) {
  assertNotAborted(signal);
  if (video.readyState >= HTMLMediaElement.HAVE_CURRENT_DATA) {
    return;
  }
  await waitForMediaEvent(video, "loadeddata", signal);
}

async function seekVideo(time, signal) {
  assertNotAborted(signal);
  await ensureVideoData(signal);
  if (Math.abs(video.currentTime - time) < 0.0005) {
    return;
  }
  video.currentTime = time;
  await waitForMediaEvent(video, "seeked", signal);
}

function rgbFromCurrentFrame(width, height) {
  context.drawImage(video, 0, 0, width, height);
  const rgba = context.getImageData(0, 0, width, height).data;
  const rgb = new Uint8Array(width * height * 3);
  for (let source = 0, target = 0; source < rgba.length; source += 4) {
    rgb[target] = rgba[source];
    rgb[target + 1] = rgba[source + 1];
    rgb[target + 2] = rgba[source + 2];
    target += 3;
  }
  return rgb;
}

function analysisDimensions(limit) {
  const sourceWidth = video.videoWidth;
  const sourceHeight = video.videoHeight;
  const scale = Math.min(1, limit / Math.max(sourceWidth, sourceHeight));
  return {
    width: Math.max(1, Math.round(sourceWidth * scale)),
    height: Math.max(1, Math.round(sourceHeight * scale)),
  };
}

function yieldToBrowser() {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

function renderBoundaryReview(review, fps) {
  boundaryRows.replaceChildren();
  if (!review) {
    boundaryReview.hidden = true;
    return;
  }

  boundaryReview.hidden = false;
  boundaryReviewSummary.textContent = `Rust ranked ${review.candidates.length} boundary candidate${
    review.candidates.length === 1 ? "" : "s"
  } by distance from the ${review.detector_threshold.toFixed(3)} detector threshold, using a ${review.review_threshold.toFixed(3)} review threshold.`;

  review.candidates.forEach((candidate, index) => {
    const row = document.createElement("tr");
    const values = [
      index + 1,
      formatCandidateStatus(candidate.status),
      candidate.frame,
      formatTime(Math.min(video.duration, candidate.frame / fps)),
      Number(candidate.score).toFixed(6),
      Number(candidate.threshold_distance).toFixed(6),
    ];
    for (const value of values) {
      const cell = document.createElement("td");
      cell.textContent = String(value);
      row.append(cell);
    }

    const previewCell = document.createElement("td");
    const previewButton = document.createElement("button");
    previewButton.type = "button";
    previewButton.className = "action-button";
    previewButton.dataset.boundaryFrame = String(candidate.frame);
    previewButton.textContent = "Seek";
    previewCell.append(previewButton);
    row.append(previewCell);
    boundaryRows.append(row);
  });
}

function renderResults(output, fps) {
  currentOutput = output;
  currentResultFps = fps;
  const scenes = output.detection.scene_list.scenes;
  const sampledFrames = output.detection.stats.rows.length;
  resultSummary.textContent = `Rust detected ${scenes.length} scene${
    scenes.length === 1 ? "" : "s"
  } from ${sampledFrames} browser-decoded samples at ${fps} fps. Boundary times below are in the sampled browser timeline.`;

  sceneRows.replaceChildren();
  scenes.forEach((scene, index) => {
    const row = document.createElement("tr");
    const values = [
      index + 1,
      scene.start,
      formatTime(Math.min(video.duration, scene.start / fps)),
      scene.end,
      formatTime(Math.min(video.duration, scene.end / fps)),
      formatTime((scene.end - scene.start) / fps),
    ];
    for (const value of values) {
      const cell = document.createElement("td");
      cell.textContent = String(value);
      row.append(cell);
    }
    sceneRows.append(row);
  });

  renderBoundaryReview(output.boundary_review, fps);
  resultsSection.hidden = false;
  resultsSection.scrollIntoView({ behavior: "smooth", block: "start" });
}

function fileStem() {
  const name = videoFile.files?.[0]?.name ?? "video";
  return name.replace(/\.[^.]+$/, "") || "video";
}

function downloadText(filename, text, type) {
  const blob = new Blob([text], { type });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

function downloadExport(kind) {
  if (!currentOutput) {
    return;
  }
  const stem = fileStem();
  const exports = currentOutput.exports;
  const definitions = {
    scene_list_csv: [`${stem}.scenes.csv`, exports.scene_list_csv, "text/csv"],
    scene_list_json: [`${stem}.scenes.json`, exports.scene_list_json, "application/json"],
    scene_events_ndjson: [
      `${stem}.scenes.ndjson`,
      exports.scene_events_ndjson,
      "application/x-ndjson",
    ],
    stats_csv: [`${stem}.stats.csv`, exports.stats_csv, "text/csv"],
    scene_list_html: [`${stem}.scenes.html`, exports.scene_list_html, "text/html"],
    detection_json: [
      `${stem}.detection.json`,
      JSON.stringify(currentOutput.detection, null, 2),
      "application/json",
    ],
    boundary_review_csv: exports.boundary_review_csv
      ? [`${stem}.boundaries.csv`, exports.boundary_review_csv, "text/csv"]
      : null,
    boundary_review_json: exports.boundary_review_json
      ? [`${stem}.boundaries.json`, exports.boundary_review_json, "application/json"]
      : null,
  };
  const definition = definitions[kind];
  if (definition) {
    downloadText(...definition);
  }
}

async function runAnalysis() {
  const file = videoFile.files?.[0];
  if (!file || !sceneDetect) {
    return;
  }

  const { fps, dimension } = samplingConfig();
  const config = readDetectorConfig();
  if (!Number.isInteger(config.min_scene_len) || config.min_scene_len < 0) {
    throw new Error("Minimum scene length must be a non-negative whole number of sampled frames.");
  }
  if (!Number.isFinite(video.duration) || video.duration <= 0) {
    throw new Error("The browser did not report a finite video duration.");
  }
  if (!video.videoWidth || !video.videoHeight) {
    throw new Error("The browser did not report decodable video dimensions.");
  }

  const sampleCount = Math.max(1, Math.ceil(video.duration * fps));
  if (sampleCount > MAX_SAMPLES) {
    throw new Error(
      `This configuration would analyze ${sampleCount} frames. Reduce the sampling rate so the run stays below ${MAX_SAMPLES} samples.`,
    );
  }

  const { width, height } = analysisDimensions(dimension);
  canvas.width = width;
  canvas.height = height;
  video.pause();

  activeAbortController = new AbortController();
  const { signal } = activeAbortController;
  let session = null;
  running = true;
  currentOutput = null;
  currentResultFps = null;
  resultsSection.hidden = true;
  boundaryReview.hidden = true;
  progress.max = sampleCount;
  progress.value = 0;
  status.textContent = `Starting ${detector.value} detection in Rust…`;
  updateRunState();

  try {
    session = sceneDetect.createSession(config, fps);
    for (let index = 0; index < sampleCount; index += 1) {
      assertNotAborted(signal);
      const time = Math.min(index / fps, Math.max(0, video.duration - 0.001));
      await seekVideo(time, signal);
      const rgb = rgbFromCurrentFrame(width, height);
      session.pushFrame(index, width, height, rgb);
      progress.value = index + 1;
      status.textContent = `Analyzing sample ${index + 1} of ${sampleCount} · ${formatTime(time)}`;
      if (index % 4 === 0) {
        await yieldToBrowser();
      }
    }

    const output = session.finish();
    session = null;
    renderResults(output, fps);
    status.textContent = "Analysis complete. Results and Rust-rendered exports are ready.";
  } catch (error) {
    if (session) {
      try {
        session.drop();
      } catch (_dropError) {
        // The Rust side may already have consumed the session while reporting an error.
      }
    }
    if (error?.name === "AbortError") {
      status.textContent = "Analysis cancelled.";
    } else {
      status.textContent = `Analysis failed: ${error.message}`;
      throw error;
    }
  } finally {
    running = false;
    activeAbortController = null;
    updateRunState();
  }
}

videoFile.addEventListener("change", () => {
  currentOutput = null;
  currentResultFps = null;
  resultsSection.hidden = true;
  boundaryReview.hidden = true;
  if (objectUrl) {
    URL.revokeObjectURL(objectUrl);
    objectUrl = null;
  }
  const file = videoFile.files?.[0];
  if (!file) {
    video.removeAttribute("src");
    videoMeta.textContent = "No video selected.";
    updateRunState();
    return;
  }
  objectUrl = URL.createObjectURL(file);
  video.src = objectUrl;
  video.load();
  videoMeta.textContent = `${file.name} · ${(file.size / (1024 * 1024)).toFixed(1)} MiB · reading local metadata…`;
  updateRunState();
});

video.addEventListener("loadedmetadata", () => {
  const file = videoFile.files?.[0];
  if (!file) {
    return;
  }
  videoMeta.textContent = `${file.name} · ${video.videoWidth}×${video.videoHeight} · ${formatDuration(
    video.duration,
  )} · ${(file.size / (1024 * 1024)).toFixed(1)} MiB`;
});

video.addEventListener("error", () => {
  videoMeta.textContent = "This browser could not decode the selected video file.";
});

detector.addEventListener("change", () => renderDetectorControls());
analysisFps.addEventListener("input", updateMinSceneTime);
minSceneLen.addEventListener("input", updateMinSceneTime);
runButton.addEventListener("click", () => {
  runAnalysis().catch((error) => {
    status.textContent = `Analysis failed: ${error.message}`;
    console.error(error);
  });
});
cancelButton.addEventListener("click", () => activeAbortController?.abort());

for (const exportRow of document.querySelectorAll(".export-row")) {
  exportRow.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-export]");
    if (button) {
      downloadExport(button.dataset.export);
    }
  });
}

boundaryRows.addEventListener("click", (event) => {
  const button = event.target.closest("button[data-boundary-frame]");
  if (!button || !currentResultFps || !Number.isFinite(video.duration)) {
    return;
  }
  const frame = Number(button.dataset.boundaryFrame);
  if (!Number.isFinite(frame)) {
    return;
  }
  video.pause();
  video.currentTime = Math.min(
    Math.max(0, video.duration - 0.001),
    Math.max(0, frame / currentResultFps),
  );
  video.scrollIntoView({ behavior: "smooth", block: "center" });
});

createSceneDetect()
  .then((loaded) => {
    sceneDetect = loaded;
    renderDetectorControls({ resetCommon: true });
    status.textContent = "SceneDetect WebAssembly loaded. Choose a local video to begin.";
    updateRunState();
  })
  .catch((error) => {
    status.textContent = `Unable to load SceneDetect WebAssembly: ${error.message}`;
    console.error(error);
  });

updateMinSceneTime();
updateRunState();
