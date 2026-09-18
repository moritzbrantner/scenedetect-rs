import {
  createLocalFramePreviewer,
  evenlySpacedPreviewTimes,
} from "./local-frame-previewer.js";

const SELECTION_EVENT = "scenedetect:review-selection";
const CONTEXT_SECONDS = 1.25;
const CONTEXT_FRAMES = 3;

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

const previewer = createLocalFramePreviewer(sourceVideo, {
  cacheLimit: 48,
  maxWidth: 640,
  maxHeight: 360,
  quality: 0.88,
});

function addFilmstrip(image, label) {
  const figure = image.closest("figure");
  const strip = document.createElement("div");
  strip.className = "cut-inspector-filmstrip";
  strip.setAttribute("aria-label", label);
  strip.hidden = true;
  figure?.insertBefore(strip, figure.querySelector("figcaption"));
  return strip;
}

const beforeStrip = addFilmstrip(beforeImage, "Frames leading into the selected cut");
const afterStrip = addFilmstrip(afterImage, "Frames after the selected cut");

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

function clampTime(time) {
  if (!Number.isFinite(sourceVideo.duration)) {
    return Math.max(0, Number(time) || 0);
  }
  return Math.min(Math.max(0, sourceVideo.duration - 0.001), Math.max(0, Number(time) || 0));
}

function clearImage(image, placeholder) {
  image.removeAttribute("src");
  placeholder.hidden = false;
  placeholder.textContent = "Loading local frame…";
}

function clearFilmstrip(strip) {
  strip.replaceChildren();
  strip.hidden = true;
}

function renderFilmstrip(strip, previews, label) {
  strip.replaceChildren(
    ...previews.map(({ time, url }, index) => {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "cut-inspector-filmstrip-frame";
      button.title = `Open ${label.toLowerCase()} context at ${formatTime(time)}`;
      button.addEventListener("click", () => openMainVideo(time));
      const image = document.createElement("img");
      image.src = url;
      image.alt = `${label} context frame ${index + 1} of ${previews.length}`;
      button.append(image);
      return button;
    }),
  );
  strip.hidden = previews.length === 0;
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
  clearFilmstrip(beforeStrip);
  clearFilmstrip(afterStrip);
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

async function loadContextStrips(signal, currentGeneration) {
  const durationEnd = Number.isFinite(sourceVideo.duration)
    ? Math.max(0, sourceVideo.duration - 0.001)
    : selectedTimes.after + CONTEXT_SECONDS;
  const beforeTimes = evenlySpacedPreviewTimes(
    Math.max(0, selectedTimes.before - CONTEXT_SECONDS),
    selectedTimes.before,
    CONTEXT_FRAMES,
  );
  const afterTimes = evenlySpacedPreviewTimes(
    selectedTimes.after,
    Math.min(durationEnd, selectedTimes.after + CONTEXT_SECONDS),
    CONTEXT_FRAMES,
  );

  try {
    const beforePreviews = await previewer.captureMany(beforeTimes, {
      signal,
      maxWidth: 240,
      maxHeight: 135,
      quality: 0.8,
    });
    if (signal.aborted || currentGeneration !== generation) {
      return;
    }
    renderFilmstrip(beforeStrip, beforePreviews, "Before cut");

    const afterPreviews = await previewer.captureMany(afterTimes, {
      signal,
      maxWidth: 240,
      maxHeight: 135,
      quality: 0.8,
    });
    if (signal.aborted || currentGeneration !== generation) {
      return;
    }
    renderFilmstrip(afterStrip, afterPreviews, "After cut");
    status.textContent =
      "Showing the exact analyzed samples plus short local context strips on both sides of the cut. Click any strip frame to open it in the video.";
  } catch (error) {
    if (error?.name === "AbortError") {
      return;
    }
    status.textContent =
      "The exact cut frames are shown, but the surrounding local context frames could not be decoded.";
    console.warn("Unable to generate cut context previews", error);
  }
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
  beforeCaption.textContent = `Before cut · sample ${detail.beforeSample} · ${formatTime(
    selectedTimes.before,
  )}`;
  afterCaption.textContent = `After cut · sample ${detail.afterSample} · ${formatTime(
    selectedTimes.after,
  )}`;
  status.textContent = "Decoding the two analyzed frames locally…";
  openBefore.disabled = false;
  openAfter.disabled = false;
  clearImage(beforeImage, beforePlaceholder);
  clearImage(afterImage, afterPlaceholder);
  clearFilmstrip(beforeStrip);
  clearFilmstrip(afterStrip);

  try {
    const beforeUrl = await previewer.capture(selectedTimes.before, {
      signal,
      maxWidth: 640,
      maxHeight: 360,
      quality: 0.88,
    });
    if (signal.aborted || currentGeneration !== generation) {
      return;
    }
    beforeImage.src = beforeUrl;
    beforePlaceholder.hidden = true;

    const afterUrl = await previewer.capture(selectedTimes.after, {
      signal,
      maxWidth: 640,
      maxHeight: 360,
      quality: 0.88,
    });
    if (signal.aborted || currentGeneration !== generation) {
      return;
    }
    afterImage.src = afterUrl;
    afterPlaceholder.hidden = true;
    status.textContent =
      detail.beforeSample === detail.afterSample
        ? "This boundary has no earlier analyzed sample; both exact previews resolve to the same sample. Loading local context frames…"
        : "Showing the analyzed sample immediately before the boundary and the boundary sample itself. Loading local context frames…";
    void loadContextStrips(signal, currentGeneration);
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
  sourceVideo.currentTime = clampTime(time);
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
