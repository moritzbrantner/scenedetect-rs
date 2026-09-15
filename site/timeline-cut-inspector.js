import { seekPresentedVideoFrame } from "./video-frame-sync.js";

const CACHE_LIMIT = 24;
const MAX_WIDTH = 640;
const MAX_HEIGHT = 360;
const SELECTION_EVENT = "scenedetect:review-selection";

const timeline = document.getElementById("scene-timeline");
const sourceVideo = document.getElementById("video-preview");
const inspector = document.getElementById("cut-inspector");
const summary = document.getElementById("cut-inspector-summary");
const status = document.getElementById("cut-inspector-status");
const beforeImage = document.getElementById("cut-before-frame");
const afterImage = document.getElementById("cut-after-frame");
const beforePlaceholder = document.getElementById("cut-before-placeholder");
const afterPlaceholder = document.getElementById("cut-after-placeholder");
const beforeCaption = document.getElementById("cut-before-caption");
const afterCaption = document.getElementById("cut-after-caption");
const openBefore = document.getElementById("cut-open-before");
const openAfter = document.getElementById("cut-open-after");

const previewVideo = document.createElement("video");
previewVideo.muted = true;
previewVideo.playsInline = true;
previewVideo.preload = "auto";
previewVideo.className = "cut-inspector-preview-video";
previewVideo.setAttribute("aria-hidden", "true");
document.body.append(previewVideo);

const previewCanvas = document.createElement("canvas");
const previewContext = previewCanvas.getContext("2d");
const cache = new Map();
let previewSource = "";
let generation = 0;
let abortController = null;
let selectedTimes = null;

function formatTime(seconds) {
  const safe = Math.max(0, Number.isFinite(seconds) ? seconds : 0);
  const hours = Math.floor(safe / 3600);
  const minutes = Math.floor((safe % 3600) / 60);
  const secs = safe % 60;
  return `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${secs
    .toFixed(3)
    .padStart(6, "0")}`;
}

function clampTime(video, time) {
  return Math.min(Math.max(0, video.duration - 0.001), Math.max(0, time));
}

function waitForMetadata(video, signal) {
  if (video.readyState >= HTMLMediaElement.HAVE_METADATA && Number.isFinite(video.duration)) {
    return Promise.resolve();
  }
  return new Promise((resolve, reject) => {
    const cleanup = () => {
      video.removeEventListener("loadedmetadata", onLoaded);
      video.removeEventListener("error", onError);
      signal?.removeEventListener("abort", onAbort);
    };
    const onLoaded = () => {
      cleanup();
      resolve();
    };
    const onError = () => {
      cleanup();
      reject(new Error("The browser could not decode the selected cut preview."));
    };
    const onAbort = () => {
      cleanup();
      reject(new DOMException("Cut preview cancelled", "AbortError"));
    };
    video.addEventListener("loadedmetadata", onLoaded, { once: true });
    video.addEventListener("error", onError, { once: true });
    signal?.addEventListener("abort", onAbort, { once: true });
  });
}

async function ensurePreviewVideo(signal) {
  const source = sourceVideo.currentSrc || sourceVideo.src;
  if (!source) {
    throw new Error("No local video is available for the selected cut preview.");
  }
  if (source !== previewSource) {
    previewSource = source;
    cache.clear();
    previewVideo.src = source;
    previewVideo.load();
  }
  await waitForMetadata(previewVideo, signal);
}

function ensureCacheLimit() {
  while (cache.size > CACHE_LIMIT) {
    cache.delete(cache.keys().next().value);
  }
}

async function capture(sample, mediaTime, signal) {
  await ensurePreviewVideo(signal);
  const key = `${previewSource}|${sample}|${Number(mediaTime).toFixed(6)}`;
  const cached = cache.get(key);
  if (cached) {
    return cached;
  }

  await seekPresentedVideoFrame(previewVideo, clampTime(previewVideo, mediaTime), signal);
  if (!previewContext || !previewVideo.videoWidth || !previewVideo.videoHeight) {
    throw new Error("The selected frame could not be drawn.");
  }
  const scale = Math.min(
    1,
    MAX_WIDTH / previewVideo.videoWidth,
    MAX_HEIGHT / previewVideo.videoHeight,
  );
  previewCanvas.width = Math.max(1, Math.round(previewVideo.videoWidth * scale));
  previewCanvas.height = Math.max(1, Math.round(previewVideo.videoHeight * scale));
  previewContext.drawImage(previewVideo, 0, 0, previewCanvas.width, previewCanvas.height);
  const dataUrl = previewCanvas.toDataURL("image/jpeg", 0.88);
  cache.set(key, dataUrl);
  ensureCacheLimit();
  return dataUrl;
}

function clearImage(image, placeholder) {
  image.removeAttribute("src");
  placeholder.hidden = false;
  placeholder.textContent = "Loading local frame…";
}

function setIdle() {
  abortController?.abort();
  abortController = null;
  generation += 1;
  selectedTimes = null;
  inspector.hidden = true;
  openBefore.disabled = true;
  openAfter.disabled = true;
  clearImage(beforeImage, beforePlaceholder);
  clearImage(afterImage, afterPlaceholder);
}

function describeSelection(detail) {
  const candidate = detail.candidate;
  if (!candidate) {
    return `${detail.label} · sample ${detail.sample}.`;
  }
  const score = Number(candidate.score);
  const distance = Number(candidate.threshold_distance);
  const metrics = [];
  if (Number.isFinite(score)) {
    metrics.push(`score ${score.toFixed(3)}`);
  }
  if (Number.isFinite(distance)) {
    metrics.push(`threshold distance ${distance.toFixed(3)}`);
  }
  const suffix = metrics.length ? ` · ${metrics.join(" · ")}` : "";
  return `${detail.label} · sample ${detail.sample}${suffix}.`;
}

async function showSelection(detail) {
  abortController?.abort();
  abortController = new AbortController();
  const { signal } = abortController;
  const currentGeneration = ++generation;
  inspector.hidden = false;
  selectedTimes = {
    before: Number(detail.beforeMediaTime),
    after: Number(detail.afterMediaTime),
  };
  summary.textContent = describeSelection(detail);
  beforeCaption.textContent = `Before · sample ${detail.beforeSample} · ${formatTime(
    selectedTimes.before,
  )}`;
  afterCaption.textContent = `After · sample ${detail.afterSample} · ${formatTime(
    selectedTimes.after,
  )}`;
  status.textContent = "Decoding the two analyzed frames locally…";
  openBefore.disabled = false;
  openAfter.disabled = false;
  clearImage(beforeImage, beforePlaceholder);
  clearImage(afterImage, afterPlaceholder);

  try {
    const beforeUrl = await capture(detail.beforeSample, selectedTimes.before, signal);
    if (signal.aborted || currentGeneration !== generation) {
      return;
    }
    beforeImage.src = beforeUrl;
    beforePlaceholder.hidden = true;

    const afterUrl = await capture(detail.afterSample, selectedTimes.after, signal);
    if (signal.aborted || currentGeneration !== generation) {
      return;
    }
    afterImage.src = afterUrl;
    afterPlaceholder.hidden = true;
    status.textContent =
      detail.beforeSample === detail.afterSample
        ? "This boundary has no earlier analyzed sample; both previews resolve to the same sample."
        : "Showing the analyzed sample immediately before the boundary and the boundary sample itself.";
  } catch (error) {
    if (error?.name === "AbortError") {
      return;
    }
    beforePlaceholder.hidden = false;
    afterPlaceholder.hidden = false;
    beforePlaceholder.textContent = "Preview unavailable";
    afterPlaceholder.textContent = "Preview unavailable";
    status.textContent = error instanceof Error ? error.message : "Selected cut preview unavailable.";
  }
}

function openMainVideo(time) {
  if (!Number.isFinite(time) || !Number.isFinite(sourceVideo.duration)) {
    return;
  }
  sourceVideo.pause();
  sourceVideo.currentTime = clampTime(sourceVideo, time);
  sourceVideo.scrollIntoView({ behavior: "smooth", block: "center" });
}

openBefore.addEventListener("click", () => openMainVideo(selectedTimes?.before));
openAfter.addEventListener("click", () => openMainVideo(selectedTimes?.after));

timeline.addEventListener(SELECTION_EVENT, (event) => {
  const detail = event.detail;
  if (!detail || detail.type !== "boundary") {
    setIdle();
    return;
  }
  void showSelection(detail);
});

setIdle();
