import { createAnalysisWorker } from "./analysis-worker-client.js";
import { createKeyboardController } from "./keyboard-controls.js";
import { createResultsOverview } from "./review-overview.js";
import { createReviewWorkspace } from "./review-workspace.js";
import {
  fingerprintsMatch,
  listRunSnapshots,
  loadWorkbenchSettings,
  mediaFingerprint,
  saveRunSnapshot,
  saveWorkbenchSettings,
  settingsMatch,
} from "./session-store.js";
import { seekPresentedVideoFrame } from "./video-frame-sync.js";

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
const timelineTrack = document.getElementById("scene-timeline");
const timelineZoom = document.getElementById("timeline-zoom");
const timelineStatus = document.getElementById("timeline-status");
const reviewStatus = document.getElementById("review-status");
const compareStatus = document.getElementById("compare-status");
const compareRun = document.getElementById("compare-run");
const saveRunButton = document.getElementById("save-run");
const importSessionButton = document.getElementById("import-session-button");
const importSessionFile = document.getElementById("import-session-file");
const keyboardBindings = document.getElementById("keyboard-bindings");

const MAX_SAMPLES = 200_000;
const analysis = createAnalysisWorker();

const detectorFields = {
  content: [
    { key: "threshold", label: "Content threshold", step: "0.1", min: "0" },
    {
      key: "review_threshold",
      label: "Boundary review threshold override",
      step: "0.1",
      min: "0",
      optional: true,
    },
    { key: "luma_only", label: "Luma only", type: "checkbox" },
    { key: "weights.hue", label: "Hue weight", step: "0.1", min: "0" },
    { key: "weights.saturation", label: "Saturation weight", step: "0.1", min: "0" },
    { key: "weights.luminance", label: "Luminance weight", step: "0.1", min: "0" },
    { key: "weights.edges", label: "Edge weight", step: "0.1", min: "0" },
  ],
  adaptive: [
    { key: "threshold", label: "Adaptive ratio threshold", step: "0.1", min: "0" },
    {
      key: "review_threshold",
      label: "Boundary review threshold override",
      step: "0.1",
      min: "0",
      optional: true,
    },
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

let objectUrl = null;
let activeAbortController = null;
let running = false;
let workerReady = false;
let currentOutput = null;
let currentResultFps = null;
let currentDetectorDefaults = null;
let renderGeneration = 0;
let settingsHydrated = false;
let settingsTimer = null;
let pendingImportedSession = null;

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
  const safeSeconds = Math.max(0, Number.isFinite(seconds) ? seconds : 0);
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

function presentedTimeForSample(sample, fallbackFps) {
  const numeric = Number(sample);
  const exact = currentOutput?.presented_samples?.find((entry) => Number(entry.sample) === numeric);
  if (exact) {
    return Number(exact.media_time_seconds);
  }
  if (
    currentOutput?.detection?.scene_list?.scenes?.at(-1)?.end === numeric &&
    Number.isFinite(video.duration)
  ) {
    return video.duration;
  }
  return numeric / fallbackFps;
}

const resultsOverview = createResultsOverview({
  video,
  sceneRows,
  boundaryRows,
  boundaryReview,
  boundaryReviewSummary,
  formatTime,
  formatCandidateStatus,
  mediaTimeForSample: presentedTimeForSample,
});

const reviewWorkspace = createReviewWorkspace({
  video,
  timelineTrack,
  timelineZoom,
  timelineStatus,
  reviewStatus,
  compareStatus,
  formatTime,
});

createKeyboardController({
  container: keyboardBindings,
  actions: {
    previous_boundary: () => reviewWorkspace.seekBoundary(-1),
    next_boundary: () => reviewWorkspace.seekBoundary(1),
    previous_scene: () => reviewWorkspace.seekScene(-1),
    next_scene: () => reviewWorkspace.seekScene(1),
    play_pause: () => {
      if (video.paused) {
        void video.play();
      } else {
        video.pause();
      }
    },
    accept_boundary: () => reviewWorkspace.acceptSelectedBoundary(),
    reject_boundary: () => reviewWorkspace.rejectSelectedBoundary(),
    add_cut: () => reviewWorkspace.addCutAtPlayhead(),
    merge_next: () => reviewWorkspace.mergeSelectedWithNext(),
    zoom_in: () => reviewWorkspace.zoomBy(0.5),
    zoom_out: () => reviewWorkspace.zoomBy(-0.5),
  },
});

function updateRunState() {
  runButton.disabled = running || !workerReady || !currentDetectorDefaults || !videoFile.files?.[0];
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

function makeDetectorField(definition, defaults, override) {
  const label = document.createElement("label");
  const title = document.createElement("span");
  title.textContent = definition.label;
  label.append(title);

  const input = document.createElement("input");
  input.dataset.configKey = definition.key;
  const value = getPath(override ?? defaults, definition.key) ?? getPath(defaults, definition.key);
  if (definition.type === "checkbox") {
    input.type = "checkbox";
    input.checked = Boolean(value);
    label.classList.add("checkbox-field");
  } else {
    input.type = "number";
    input.step = definition.step ?? "any";
    if (definition.optional) {
      input.dataset.optional = "true";
      input.value = value == null ? "" : String(value);
      input.placeholder = "Auto: 80% of detector threshold";
    } else {
      input.value = String(value);
    }
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

async function renderDetectorControls({ resetCommon = false, override = null } = {}) {
  const generation = ++renderGeneration;
  currentDetectorDefaults = null;
  updateRunState();
  const defaults = await analysis.defaults(detector.value);
  if (generation !== renderGeneration) {
    return;
  }
  currentDetectorDefaults = defaults;
  detectorControls.replaceChildren(
    ...detectorFields[detector.value].map((definition) =>
      makeDetectorField(definition, defaults, override),
    ),
  );
  if (resetCommon) {
    minSceneLen.value = String(override?.min_scene_len ?? defaults.min_scene_len);
    minScenePolicy.value = override?.min_scene_len_policy ?? defaults.min_scene_len_policy;
  }
  updateMinSceneTime();
  updateRunState();
}

function readDetectorConfig() {
  if (!currentDetectorDefaults) {
    throw new Error("Detector defaults are still loading.");
  }
  const config = structuredClone(currentDetectorDefaults);
  config.min_scene_len = Number(minSceneLen.value);
  config.min_scene_len_policy = minScenePolicy.value;

  for (const input of detectorControls.querySelectorAll("input[data-config-key]")) {
    if (
      input.type !== "checkbox" &&
      input.dataset.optional === "true" &&
      input.value.trim() === ""
    ) {
      setPath(config, input.dataset.configKey, null);
      continue;
    }
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
  if (!Number.isFinite(fps) || fps < 0.5 || fps > 30) {
    throw new Error("Analysis frames per second must be between 0.5 and 30.");
  }
  if (!Number.isInteger(dimension) || dimension <= 0) {
    throw new Error("Maximum frame dimension is invalid.");
  }
  return { fps, dimension };
}

function captureSettings() {
  return {
    schema_version: 1,
    detector: detector.value,
    analysis_fps: Number(analysisFps.value),
    max_dimension: Number(maxDimension.value),
    detector_config: readDetectorConfig(),
  };
}

function scheduleSettingsSave() {
  if (!settingsHydrated || !currentDetectorDefaults) {
    return;
  }
  clearTimeout(settingsTimer);
  settingsTimer = setTimeout(() => {
    try {
      saveWorkbenchSettings(captureSettings());
    } catch (_error) {
      // Invalid in-progress form values should not replace the last valid settings snapshot.
    }
  }, 180);
}

async function applySettings(settings) {
  if (!settings || settings.schema_version !== 1) {
    await renderDetectorControls({ resetCommon: true });
    return;
  }
  if (detectorFields[settings.detector]) {
    detector.value = settings.detector;
  }
  if (Number.isFinite(settings.analysis_fps)) {
    analysisFps.value = String(settings.analysis_fps);
  }
  if (Number.isInteger(settings.max_dimension)) {
    maxDimension.value = String(settings.max_dimension);
  }
  const config = settings.detector_config ?? null;
  await renderDetectorControls({ resetCommon: true, override: config });
}

function abortError() {
  return new DOMException("Analysis cancelled", "AbortError");
}

function assertNotAborted(signal) {
  if (signal.aborted) {
    throw abortError();
  }
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
  const scale = Math.min(1, limit / Math.max(video.videoWidth, video.videoHeight));
  return {
    width: Math.max(1, Math.round(video.videoWidth * scale)),
    height: Math.max(1, Math.round(video.videoHeight * scale)),
  };
}

function yieldToBrowser() {
  return new Promise((resolve) => requestAnimationFrame(resolve));
}

function currentMediaFingerprint() {
  return mediaFingerprint(videoFile.files?.[0] ?? null);
}

function renderResults(output, fps) {
  currentOutput = output;
  currentResultFps = fps;
  const scenes = output.detection.scene_list.scenes;
  const sampledFrames = output.detection.stats.rows.length;
  const timedSamples = output.presented_samples?.length ?? 0;
  resultSummary.textContent = `Rust detected ${scenes.length} scene${
    scenes.length === 1 ? "" : "s"
  } from ${sampledFrames} browser-decoded samples at ${fps} fps. ${timedSamples} presented media timestamps were preserved through the WebAssembly session.`;

  resultsOverview.renderScenes(scenes, fps);
  resultsOverview.renderBoundaryReview(output.boundary_review, fps);
  reviewWorkspace.load({ output, fps, duration: video.duration });

  if (
    pendingImportedSession &&
    fingerprintsMatch(pendingImportedSession.media, currentMediaFingerprint())
  ) {
    if (settingsMatch(pendingImportedSession.settings, captureSettings())) {
      reviewWorkspace.loadReviewDecisions(pendingImportedSession.review?.decisions);
      pendingImportedSession = null;
    } else {
      reviewStatus.textContent =
        "Imported review decisions remain detached because the current detector or sampling settings differ from the imported session.";
    }
  }

  refreshSavedRuns();
  resultsSection.hidden = false;
  resultsSection.scrollIntoView({ behavior: "smooth", block: "start" });
}

function fileStem() {
  const name = videoFile.files?.[0]?.name ?? "video";
  return name.replace(/\.[^.]+$/u, "") || "video";
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

function downloadRustExport(kind) {
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
      JSON.stringify(
        { detection: currentOutput.detection, presented_samples: currentOutput.presented_samples },
        null,
        2,
      ),
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

function downloadWorkbenchExport(kind) {
  if (!currentOutput) {
    return;
  }
  const stem = fileStem();
  if (kind === "review_json") {
    downloadText(
      `${stem}.reviewed-scenes.json`,
      JSON.stringify(reviewWorkspace.reviewArtifact(), null, 2),
      "application/json",
    );
  } else if (kind === "review_csv") {
    downloadText(`${stem}.reviewed-scenes.csv`, reviewWorkspace.reviewedCsv(), "text/csv");
  } else if (kind === "session_json") {
    downloadText(
      `${stem}.scenedetect-session.json`,
      JSON.stringify(
        reviewWorkspace.sessionArtifact({
          media: currentMediaFingerprint(),
          settings: captureSettings(),
        }),
        null,
        2,
      ),
      "application/json",
    );
  }
}

function refreshSavedRuns() {
  const previousValue = compareRun.value;
  const fingerprint = currentMediaFingerprint();
  const snapshots = listRunSnapshots().filter((entry) =>
    fingerprintsMatch(entry.media, fingerprint),
  );
  compareRun.replaceChildren(new Option("No comparison", ""));
  for (const snapshot of snapshots) {
    compareRun.append(new Option(snapshot.label, snapshot.id));
  }
  if (snapshots.some((snapshot) => snapshot.id === previousValue)) {
    compareRun.value = previousValue;
  }
}

async function runAnalysis() {
  const file = videoFile.files?.[0];
  if (!file || !workerReady) {
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
  let sessionStarted = false;
  let lastPresentedMediaTime = Number.NEGATIVE_INFINITY;
  running = true;
  currentOutput = null;
  currentResultFps = null;
  resultsOverview.reset();
  reviewWorkspace.reset();
  resultsSection.hidden = true;
  boundaryReview.hidden = true;
  progress.max = sampleCount;
  progress.value = 0;
  status.textContent = `Starting ${detector.value} detection in the Rust worker…`;
  updateRunState();

  try {
    await analysis.start(config, fps);
    sessionStarted = true;
    for (let index = 0; index < sampleCount; index += 1) {
      assertNotAborted(signal);
      const targetTime = Math.min(index / fps, Math.max(0, video.duration - 0.001));
      const presentedFrame = await seekPresentedVideoFrame(video, targetTime, signal);
      if (presentedFrame.mediaTime + Number.EPSILON < lastPresentedMediaTime) {
        throw new Error(
          `Browser presented video frames out of order: ${presentedFrame.mediaTime.toFixed(6)}s after ${lastPresentedMediaTime.toFixed(6)}s.`,
        );
      }
      lastPresentedMediaTime = Math.max(lastPresentedMediaTime, presentedFrame.mediaTime);
      const rgb = rgbFromCurrentFrame(width, height);
      await analysis.pushFrame(index, width, height, presentedFrame.mediaTime, rgb);
      progress.value = index + 1;
      status.textContent = `Analyzing sample ${index + 1} of ${sampleCount} · presented ${formatTime(
        presentedFrame.mediaTime,
      )}`;
      if (index % 8 === 0) {
        await yieldToBrowser();
      }
    }

    const output = await analysis.finish();
    sessionStarted = false;
    renderResults(output, fps);
    saveWorkbenchSettings(captureSettings());
    status.textContent = "Analysis complete. Timeline, review controls, and exports are ready.";
  } catch (error) {
    if (sessionStarted) {
      try {
        await analysis.drop();
      } catch (_dropError) {
        // The worker may already have dropped the session while reporting an error.
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
  resultsOverview.reset();
  reviewWorkspace.reset();
  resultsSection.hidden = true;
  boundaryReview.hidden = true;
  compareRun.replaceChildren(new Option("No comparison", ""));
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
  refreshSavedRuns();
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

detector.addEventListener("change", () => {
  renderDetectorControls({ resetCommon: true })
    .then(scheduleSettingsSave)
    .catch((error) => {
      status.textContent = `Unable to load detector defaults: ${error.message}`;
    });
});

for (const element of [analysisFps, maxDimension, minSceneLen, minScenePolicy]) {
  element.addEventListener("input", () => {
    updateMinSceneTime();
    scheduleSettingsSave();
  });
  element.addEventListener("change", scheduleSettingsSave);
}

detectorControls.addEventListener("input", scheduleSettingsSave);
detectorControls.addEventListener("change", scheduleSettingsSave);

runButton.addEventListener("click", () => {
  runAnalysis().catch((error) => {
    status.textContent = `Analysis failed: ${error.message}`;
    console.error(error);
  });
});

cancelButton.addEventListener("click", () => activeAbortController?.abort());

for (const exportRow of document.querySelectorAll(".export-row")) {
  exportRow.addEventListener("click", (event) => {
    const rustButton = event.target.closest("button[data-export]");
    if (rustButton) {
      downloadRustExport(rustButton.dataset.export);
      return;
    }
    const workbenchButton = event.target.closest("button[data-workbench-export]");
    if (workbenchButton) {
      downloadWorkbenchExport(workbenchButton.dataset.workbenchExport);
    }
  });
}

document.querySelector(".review-actions").addEventListener("click", (event) => {
  const action = event.target.closest("button[data-review-action]")?.dataset.reviewAction;
  if (action === "accept") {
    reviewWorkspace.acceptSelectedBoundary();
  } else if (action === "reject") {
    reviewWorkspace.rejectSelectedBoundary();
  } else if (action === "add-cut") {
    reviewWorkspace.addCutAtPlayhead();
  } else if (action === "merge-previous") {
    reviewWorkspace.mergeSelectedWithPrevious();
  } else if (action === "merge-next") {
    reviewWorkspace.mergeSelectedWithNext();
  } else if (action === "reset") {
    reviewWorkspace.resetReview();
  }
});

boundaryRows.addEventListener("click", (event) => {
  const boundaryButton = event.target.closest("button[data-boundary-frame]");
  if (!boundaryButton || !currentResultFps || !Number.isFinite(video.duration)) {
    return;
  }
  const sample = Number(boundaryButton.dataset.boundaryFrame);
  if (!Number.isFinite(sample)) {
    return;
  }
  video.pause();
  video.currentTime = Math.min(
    Math.max(0, video.duration - 0.001),
    Math.max(0, presentedTimeForSample(sample, currentResultFps)),
  );
  video.scrollIntoView({ behavior: "smooth", block: "center" });
});

saveRunButton.addEventListener("click", () => {
  if (!currentOutput) {
    return;
  }
  const id = globalThis.crypto?.randomUUID?.() ?? `run-${Date.now()}`;
  const snapshot = reviewWorkspace.snapshot({
    id,
    label: `${detector.value} · ${new Date().toLocaleString()}`,
    media: currentMediaFingerprint(),
    settings: captureSettings(),
  });
  saveRunSnapshot(snapshot);
  refreshSavedRuns();
  compareStatus.textContent = "Current detector run saved locally for later comparison.";
});

compareRun.addEventListener("change", () => {
  const snapshot = listRunSnapshots().find((entry) => entry.id === compareRun.value) ?? null;
  reviewWorkspace.compareWith(snapshot);
});

importSessionButton.addEventListener("click", () => importSessionFile.click());
importSessionFile.addEventListener("change", async () => {
  const file = importSessionFile.files?.[0];
  importSessionFile.value = "";
  if (!file) {
    return;
  }
  try {
    const imported = JSON.parse(await file.text());
    if (imported.schema_version !== 1 || !imported.settings || !imported.review) {
      throw new Error("Unsupported or incomplete workbench session file.");
    }
    await applySettings(imported.settings);
    saveWorkbenchSettings(captureSettings());
    pendingImportedSession = imported;
    status.textContent =
      "Workbench session settings imported. Analyze the matching local video with these exact settings to restore review decisions against the correct detector result.";
  } catch (error) {
    status.textContent = `Unable to import workbench session: ${error.message}`;
  }
});

window.addEventListener("beforeunload", () => {
  analysis.close();
  if (objectUrl) {
    URL.revokeObjectURL(objectUrl);
  }
});

async function initialize() {
  try {
    const ready = await analysis.ready();
    workerReady = ready?.abi === 2;
    if (!workerReady) {
      throw new Error(`Unsupported SceneDetect worker ABI ${ready?.abi ?? "unknown"}.`);
    }
    await applySettings(loadWorkbenchSettings());
    settingsHydrated = true;
    status.textContent = "SceneDetect WebAssembly worker loaded. Choose a local video to begin.";
  } catch (error) {
    status.textContent = `Unable to load SceneDetect WebAssembly worker: ${error.message}`;
    console.error(error);
  } finally {
    updateMinSceneTime();
    updateRunState();
  }
}

void initialize();
