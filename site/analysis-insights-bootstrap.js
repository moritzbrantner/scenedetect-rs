import { createAnalysisInsights } from "./analysis-insights.js";

const detector = document.getElementById("detector");
const detectorControls = document.getElementById("detector-controls");
const video = document.getElementById("video-preview");
const runButton = document.getElementById("run-analysis");

const heatmapCanvas = document.getElementById("detector-score-heatmap");
const metricLabel = document.getElementById("detector-metric-label");
const thresholdControls = document.getElementById("threshold-preview-controls");
const thresholdInput = document.getElementById("threshold-preview");
const thresholdValue = document.getElementById("threshold-preview-value");
const candidateSummary = document.getElementById("threshold-preview-summary");
const applyThresholdButton = document.getElementById("apply-threshold-rerun");
const similarityThreshold = document.getElementById("similarity-threshold");
const similarityThresholdValue = document.getElementById("similarity-threshold-value");
const similarityStatus = document.getElementById("scene-similarity-status");
const similarityList = document.getElementById("scene-similarity-list");

let latestOutput = null;

function setPath(object, path, value) {
  const parts = path.split(".");
  let target = object;
  for (const part of parts.slice(0, -1)) {
    target[part] ??= {};
    target = target[part];
  }
  target[parts.at(-1)] = value;
}

function currentDetectorSettings() {
  const config = { detector: detector.value };
  for (const input of detectorControls.querySelectorAll("input[data-config-key]")) {
    if (input.dataset.optional === "true" && input.value.trim() === "") {
      setPath(config, input.dataset.configKey, null);
      continue;
    }
    setPath(
      config,
      input.dataset.configKey,
      input.type === "checkbox" ? input.checked : Number(input.value),
    );
  }
  return {
    detector: detector.value,
    detector_config: config,
  };
}

function presentedTimeForSample(sample) {
  const numeric = Number(sample);
  const exact = latestOutput?.presented_samples?.find(
    (entry) => Number(entry.sample) === numeric,
  );
  if (exact) {
    return Number(exact.media_time_seconds);
  }
  const fps = Number(document.getElementById("analysis-fps")?.value);
  return Number.isFinite(fps) && fps > 0 ? numeric / fps : 0;
}

function seekSample(sample) {
  if (!latestOutput || !Number.isFinite(video.duration)) {
    return;
  }
  video.pause();
  video.currentTime = Math.min(
    Math.max(0, video.duration - 0.001),
    Math.max(0, presentedTimeForSample(sample)),
  );
  video.scrollIntoView({ behavior: "smooth", block: "center" });
}

async function applyThresholdAndRerun(threshold) {
  const input = detectorControls.querySelector('input[data-config-key="threshold"]');
  if (!input || runButton.disabled) {
    return;
  }
  input.value = String(threshold);
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
  runButton.click();
}

const insights = createAnalysisInsights({
  heatmapCanvas,
  metricLabel,
  thresholdControls,
  thresholdInput,
  thresholdValue,
  candidateSummary,
  applyThresholdButton,
  similarityThreshold,
  similarityStatus,
  similarityList,
  seekSample,
  applyThresholdAndRerun,
});

function updateSimilarityThresholdLabel() {
  similarityThresholdValue.textContent = `${(Number(similarityThreshold.value) * 100).toFixed(0)}%`;
}

similarityThreshold.addEventListener("input", updateSimilarityThresholdLabel);
updateSimilarityThresholdLabel();
insights.reset();

globalThis.addEventListener("scenedetect:analysis-start", () => {
  latestOutput = null;
  insights.reset();
});

globalThis.addEventListener("scenedetect:analysis-complete", (event) => {
  latestOutput = event.detail?.output ?? null;
  if (!latestOutput) {
    insights.reset();
    return;
  }
  insights.load({
    output: latestOutput,
    settings: currentDetectorSettings(),
  });
});
