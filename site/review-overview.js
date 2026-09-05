import { seekPresentedVideoFrame } from "./video-frame-sync.js";

const VISUAL_PAGE_SIZE = 12;
const THUMBNAIL_CACHE_LIMIT = 96;
const THUMBNAIL_MAX_WIDTH = 320;
const THUMBNAIL_MAX_HEIGHT = 180;

function option(value, label) {
  const item = document.createElement("option");
  item.value = value;
  item.textContent = label;
  return item;
}

function labelledSelect(labelText, id, options) {
  const label = document.createElement("label");
  label.className = "results-browser-field";
  const labelSpan = document.createElement("span");
  labelSpan.textContent = labelText;
  const select = document.createElement("select");
  select.id = id;
  select.append(...options.map(([value, text]) => option(value, text)));
  label.append(labelSpan, select);
  return { label, select };
}

function setTableHeaders(table, labels) {
  const row = table.querySelector("thead tr");
  row.replaceChildren(
    ...labels.map((label) => {
      const cell = document.createElement("th");
      cell.scope = "col";
      cell.textContent = label;
      return cell;
    }),
  );
}

function appendCells(row, values) {
  for (const value of values) {
    const cell = document.createElement("td");
    cell.textContent = String(value);
    row.append(cell);
  }
}

function candidateReviewGrouping(candidate, review) {
  const detectorThreshold = Number(review.detector_threshold);
  const reviewThreshold = Number(review.review_threshold);
  const reviewBand = Math.max(0, detectorThreshold - reviewThreshold);
  const distance = Number(candidate.threshold_distance);
  return distance <= reviewBand + Number.EPSILON ? "edge_case" : "obvious";
}

function formatReviewGrouping(value) {
  return value === "edge_case" ? "Edge case" : "Obvious";
}

function splitDisposition(status) {
  return status === "accepted" ? "Split made" : "Proposed split";
}

function waitForMetadata(video, signal) {
  if (video.readyState >= HTMLMediaElement.HAVE_METADATA && Number.isFinite(video.duration)) {
    return Promise.resolve();
  }

  return new Promise((resolve, reject) => {
    const cleanup = () => {
      video.removeEventListener("loadedmetadata", loaded);
      video.removeEventListener("error", failed);
      signal?.removeEventListener("abort", aborted);
    };
    const loaded = () => {
      cleanup();
      resolve();
    };
    const failed = () => {
      cleanup();
      reject(new Error("The browser could not decode frames for split previews."));
    };
    const aborted = () => {
      cleanup();
      reject(new DOMException("Preview generation cancelled", "AbortError"));
    };

    video.addEventListener("loadedmetadata", loaded, { once: true });
    video.addEventListener("error", failed, { once: true });
    signal?.addEventListener("abort", aborted, { once: true });
  });
}

function sceneSortComparator(value) {
  if (value === "start_desc") {
    return (left, right) => right.scene.start - left.scene.start;
  }
  if (value === "length_desc") {
    return (left, right) =>
      right.scene.end - right.scene.start - (left.scene.end - left.scene.start);
  }
  if (value === "length_asc") {
    return (left, right) =>
      left.scene.end - left.scene.start - (right.scene.end - right.scene.start);
  }
  return (left, right) => left.scene.start - right.scene.start;
}

function boundarySortComparator(value) {
  if (value === "time_asc") {
    return (left, right) => left.candidate.frame - right.candidate.frame;
  }
  if (value === "time_desc") {
    return (left, right) => right.candidate.frame - left.candidate.frame;
  }
  if (value === "score_desc") {
    return (left, right) => Number(right.candidate.score) - Number(left.candidate.score);
  }
  if (value === "score_asc") {
    return (left, right) => Number(left.candidate.score) - Number(right.candidate.score);
  }
  if (value === "distance_desc") {
    return (left, right) =>
      Number(right.candidate.threshold_distance) - Number(left.candidate.threshold_distance);
  }
  if (value === "distance_asc") {
    return (left, right) =>
      Number(left.candidate.threshold_distance) - Number(right.candidate.threshold_distance);
  }
  return (left, right) => left.rank - right.rank;
}

export function createResultsOverview({
  video,
  sceneRows,
  boundaryRows,
  boundaryReview,
  boundaryReviewSummary,
  formatTime,
  formatCandidateStatus,
}) {
  const sceneTable = sceneRows.closest("table");
  const sceneTableFrame = sceneRows.closest(".result-table-frame");
  const boundaryTable = boundaryRows.closest("table");
  const boundaryTableFrame = boundaryRows.closest(".result-table-frame");

  setTableHeaders(sceneTable, [
    "Scene",
    "Start sample",
    "Start time",
    "End sample",
    "End time",
    "Length",
  ]);
  setTableHeaders(boundaryTable, [
    "Rust rank",
    "Status",
    "Review",
    "Candidate sample",
    "Candidate time",
    "Score",
    "Threshold distance",
    "Preview",
  ]);

  const sceneToolbar = document.createElement("div");
  sceneToolbar.className = "results-browser-toolbar";
  sceneToolbar.hidden = true;

  const sceneSearchLabel = document.createElement("label");
  sceneSearchLabel.className = "results-browser-field results-browser-search";
  const sceneSearchTitle = document.createElement("span");
  sceneSearchTitle.textContent = "Filter scenes";
  const sceneSearch = document.createElement("input");
  sceneSearch.id = "scene-search-filter";
  sceneSearch.type = "search";
  sceneSearch.placeholder = "Sample, time, or scene number";
  sceneSearchLabel.append(sceneSearchTitle, sceneSearch);

  const sceneSortControl = labelledSelect("Sort", "scene-sort", [
    ["start_asc", "Start sample · earliest first"],
    ["start_desc", "Start sample · latest first"],
    ["length_desc", "Length · longest first"],
    ["length_asc", "Length · shortest first"],
  ]);
  const sceneCount = document.createElement("p");
  sceneCount.className = "results-browser-count";
  sceneCount.setAttribute("aria-live", "polite");
  sceneToolbar.append(sceneSearchLabel, sceneSortControl.label, sceneCount);
  sceneTableFrame.before(sceneToolbar);

  const boundaryToolbar = document.createElement("div");
  boundaryToolbar.className = "results-browser-toolbar boundary-browser-toolbar";

  const statusControl = labelledSelect("Decision", "boundary-status-filter", [
    ["all", "All decisions"],
    ["accepted", "Accepted"],
    ["suppressed_min_scene_len", "Suppressed"],
    ["near_miss", "Near miss"],
  ]);
  const groupingControl = labelledSelect("Review", "boundary-review-filter", [
    ["all", "All review distances"],
    ["edge_case", "Edge cases"],
    ["obvious", "Obvious"],
  ]);
  const boundarySortControl = labelledSelect("Sort", "boundary-sort", [
    ["rust_rank", "Rust review rank"],
    ["time_asc", "Candidate time · earliest first"],
    ["time_desc", "Candidate time · latest first"],
    ["score_desc", "Score · highest first"],
    ["score_asc", "Score · lowest first"],
    ["distance_asc", "Threshold distance · closest first"],
    ["distance_desc", "Threshold distance · farthest first"],
  ]);
  const boundaryCount = document.createElement("p");
  boundaryCount.className = "results-browser-count";
  boundaryCount.setAttribute("aria-live", "polite");
  boundaryToolbar.append(
    statusControl.label,
    groupingControl.label,
    boundarySortControl.label,
    boundaryCount,
  );

  const visualSection = document.createElement("section");
  visualSection.className = "split-review-overview";
  visualSection.setAttribute("aria-labelledby", "split-review-overview-heading");
  const visualHeading = document.createElement("div");
  visualHeading.className = "split-review-heading";
  const visualTitle = document.createElement("h4");
  visualTitle.id = "split-review-overview-heading";
  visualTitle.textContent = "Before / after split review";
  const visualHelp = document.createElement("p");
  visualHelp.textContent =
    "Accepted candidates are splits SceneDetect made. Suppressed and near-miss candidates remain proposed review points. Frame previews are decoded locally and never affect Rust detection.";
  visualHeading.append(visualTitle, visualHelp);

  const visualGrid = document.createElement("div");
  visualGrid.className = "split-review-grid";
  const visualPager = document.createElement("div");
  visualPager.className = "split-review-pager";
  const previousPage = document.createElement("button");
  previousPage.type = "button";
  previousPage.className = "action-button";
  previousPage.textContent = "Previous previews";
  const pageStatus = document.createElement("span");
  pageStatus.setAttribute("aria-live", "polite");
  const nextPage = document.createElement("button");
  nextPage.type = "button";
  nextPage.className = "action-button";
  nextPage.textContent = "Next previews";
  visualPager.append(previousPage, pageStatus, nextPage);
  visualSection.append(visualHeading, visualGrid, visualPager);

  boundaryTableFrame.before(boundaryToolbar, visualSection);

  const previewVideo = document.createElement("video");
  previewVideo.muted = true;
  previewVideo.playsInline = true;
  previewVideo.preload = "auto";
  previewVideo.className = "review-preview-video";
  previewVideo.setAttribute("aria-hidden", "true");
  document.body.append(previewVideo);

  const previewCanvas = document.createElement("canvas");
  const previewContext = previewCanvas.getContext("2d");

  let scenes = [];
  let sceneFps = null;
  let review = null;
  let reviewFps = null;
  let visualPage = 0;
  let previewAbortController = null;
  let previewGeneration = 0;
  let previewSource = "";
  const thumbnailCache = new Map();

  function filteredScenes() {
    const query = sceneSearch.value.trim().toLowerCase();
    return scenes
      .map((scene, index) => ({ scene, sceneNumber: index + 1 }))
      .filter(({ scene, sceneNumber }) => {
        if (!query) {
          return true;
        }
        const startTime = formatTime(Math.min(video.duration, scene.start / sceneFps));
        const endTime = formatTime(Math.min(video.duration, scene.end / sceneFps));
        const haystack = `${sceneNumber} ${scene.start} ${scene.end} ${startTime} ${endTime}`.toLowerCase();
        return haystack.includes(query);
      })
      .sort(sceneSortComparator(sceneSortControl.select.value));
  }

  function renderSceneTable() {
    sceneRows.replaceChildren();
    if (!Number.isFinite(sceneFps)) {
      sceneToolbar.hidden = true;
      return;
    }

    sceneToolbar.hidden = false;
    const visibleScenes = filteredScenes();
    sceneCount.textContent = `Showing ${visibleScenes.length} of ${scenes.length} scenes.`;
    for (const { scene, sceneNumber } of visibleScenes) {
      const row = document.createElement("tr");
      appendCells(row, [
        sceneNumber,
        scene.start,
        formatTime(Math.min(video.duration, scene.start / sceneFps)),
        scene.end,
        formatTime(Math.min(video.duration, scene.end / sceneFps)),
        formatTime((scene.end - scene.start) / sceneFps),
      ]);
      sceneRows.append(row);
    }
  }

  function filteredCandidates() {
    if (!review) {
      return [];
    }
    const status = statusControl.select.value;
    const grouping = groupingControl.select.value;
    return review.candidates
      .map((candidate, index) => ({
        candidate,
        rank: index + 1,
        grouping: candidateReviewGrouping(candidate, review),
      }))
      .filter((entry) => status === "all" || entry.candidate.status === status)
      .filter((entry) => grouping === "all" || entry.grouping === grouping)
      .sort(boundarySortComparator(boundarySortControl.select.value));
  }

  function renderBoundaryTable(entries) {
    boundaryRows.replaceChildren();
    for (const entry of entries) {
      const row = document.createElement("tr");
      appendCells(row, [
        entry.rank,
        formatCandidateStatus(entry.candidate.status),
        formatReviewGrouping(entry.grouping),
        entry.candidate.frame,
        formatTime(Math.min(video.duration, entry.candidate.frame / reviewFps)),
        Number(entry.candidate.score).toFixed(6),
        Number(entry.candidate.threshold_distance).toFixed(6),
      ]);
      const previewCell = document.createElement("td");
      const previewButton = document.createElement("button");
      previewButton.type = "button";
      previewButton.className = "action-button";
      previewButton.dataset.boundaryFrame = String(entry.candidate.frame);
      previewButton.textContent = "Seek";
      previewCell.append(previewButton);
      row.append(previewCell);
      boundaryRows.append(row);
    }
  }

  function ensureCacheLimit() {
    while (thumbnailCache.size > THUMBNAIL_CACHE_LIMIT) {
      const oldest = thumbnailCache.keys().next().value;
      thumbnailCache.delete(oldest);
    }
  }

  async function ensurePreviewVideo(signal) {
    const source = video.currentSrc || video.src;
    if (!source) {
      throw new Error("No local video is available for split previews.");
    }
    if (source !== previewSource) {
      previewSource = source;
      thumbnailCache.clear();
      previewVideo.src = source;
      previewVideo.load();
    }
    await waitForMetadata(previewVideo, signal);
  }

  async function captureSample(sample, fps, signal) {
    await ensurePreviewVideo(signal);
    const cacheKey = `${previewSource}|${fps}|${sample}`;
    const cached = thumbnailCache.get(cacheKey);
    if (cached) {
      return cached;
    }

    const time = Math.min(
      Math.max(0, previewVideo.duration - 0.001),
      Math.max(0, sample / fps),
    );
    await seekPresentedVideoFrame(previewVideo, time, signal);
    const scale = Math.min(
      1,
      THUMBNAIL_MAX_WIDTH / previewVideo.videoWidth,
      THUMBNAIL_MAX_HEIGHT / previewVideo.videoHeight,
    );
    previewCanvas.width = Math.max(1, Math.round(previewVideo.videoWidth * scale));
    previewCanvas.height = Math.max(1, Math.round(previewVideo.videoHeight * scale));
    previewContext.drawImage(previewVideo, 0, 0, previewCanvas.width, previewCanvas.height);
    const dataUrl = previewCanvas.toDataURL("image/jpeg", 0.82);
    thumbnailCache.set(cacheKey, dataUrl);
    ensureCacheLimit();
    return dataUrl;
  }

  function framePreview(label, sample) {
    const figure = document.createElement("figure");
    figure.className = "split-review-frame";
    const image = document.createElement("img");
    image.alt = `${label} frame at sample ${sample}`;
    const placeholder = document.createElement("span");
    placeholder.className = "split-review-frame-placeholder";
    placeholder.textContent = "Loading local frame…";
    const caption = document.createElement("figcaption");
    caption.textContent = `${label} · sample ${sample}`;
    figure.append(image, placeholder, caption);
    return { figure, image, placeholder };
  }

  function candidateCard(entry) {
    const card = document.createElement("article");
    card.className = "split-review-card";

    const header = document.createElement("div");
    header.className = "split-review-card-header";
    const title = document.createElement("div");
    const disposition = document.createElement("strong");
    disposition.textContent = splitDisposition(entry.candidate.status);
    const candidateLabel = document.createElement("span");
    candidateLabel.textContent = `Candidate sample ${entry.candidate.frame} · ${formatTime(
      Math.min(video.duration, entry.candidate.frame / reviewFps),
    )}`;
    title.append(disposition, candidateLabel);

    const badges = document.createElement("div");
    badges.className = "split-review-badges";
    const statusBadge = document.createElement("span");
    statusBadge.className = `split-review-badge status-${entry.candidate.status}`;
    statusBadge.textContent = formatCandidateStatus(entry.candidate.status);
    const groupingBadge = document.createElement("span");
    groupingBadge.className = `split-review-badge review-${entry.grouping}`;
    groupingBadge.textContent = formatReviewGrouping(entry.grouping);
    badges.append(statusBadge, groupingBadge);
    header.append(title, badges);

    const beforeSample = Math.max(0, Number(entry.candidate.frame) - 1);
    const afterSample = Number(entry.candidate.frame);
    const before = framePreview("Before", beforeSample);
    const after = framePreview("After", afterSample);
    const frames = document.createElement("div");
    frames.className = "split-review-frames";
    const splitMarker = document.createElement("div");
    splitMarker.className = "split-review-marker";
    splitMarker.setAttribute("aria-hidden", "true");
    splitMarker.textContent = "→";
    frames.append(before.figure, splitMarker, after.figure);

    const footer = document.createElement("div");
    footer.className = "split-review-card-footer";
    const score = document.createElement("span");
    score.textContent = `Score ${Number(entry.candidate.score).toFixed(3)} · distance ${Number(
      entry.candidate.threshold_distance,
    ).toFixed(3)}`;
    const seek = document.createElement("button");
    seek.type = "button";
    seek.className = "action-button";
    seek.dataset.previewBoundaryFrame = String(entry.candidate.frame);
    seek.textContent = "Open at split";
    footer.append(score, seek);

    card.append(header, frames, footer);
    return { card, before, after, beforeSample, afterSample };
  }

  function seekMainVideo(frame) {
    if (!Number.isFinite(reviewFps) || !Number.isFinite(video.duration)) {
      return;
    }
    video.pause();
    video.currentTime = Math.min(
      Math.max(0, video.duration - 0.001),
      Math.max(0, frame / reviewFps),
    );
    video.scrollIntoView({ behavior: "smooth", block: "center" });
  }

  async function renderVisualPage(entries) {
    previewAbortController?.abort();
    previewAbortController = new AbortController();
    const { signal } = previewAbortController;
    const generation = ++previewGeneration;
    visualGrid.replaceChildren();

    const pageCount = Math.max(1, Math.ceil(entries.length / VISUAL_PAGE_SIZE));
    visualPage = Math.min(visualPage, pageCount - 1);
    const pageStart = visualPage * VISUAL_PAGE_SIZE;
    const pageEntries = entries.slice(pageStart, pageStart + VISUAL_PAGE_SIZE);

    previousPage.disabled = visualPage === 0 || entries.length === 0;
    nextPage.disabled = visualPage >= pageCount - 1 || entries.length === 0;
    pageStatus.textContent = entries.length
      ? `Preview page ${visualPage + 1} of ${pageCount}`
      : "No candidates match these filters.";

    for (const entry of pageEntries) {
      const preview = candidateCard(entry);
      visualGrid.append(preview.card);
      try {
        const beforeUrl = await captureSample(preview.beforeSample, reviewFps, signal);
        if (generation !== previewGeneration || signal.aborted) {
          return;
        }
        preview.before.image.src = beforeUrl;
        preview.before.placeholder.hidden = true;

        const afterUrl = await captureSample(preview.afterSample, reviewFps, signal);
        if (generation !== previewGeneration || signal.aborted) {
          return;
        }
        preview.after.image.src = afterUrl;
        preview.after.placeholder.hidden = true;
      } catch (error) {
        if (error?.name === "AbortError") {
          return;
        }
        preview.before.placeholder.textContent = "Preview unavailable";
        preview.after.placeholder.textContent = "Preview unavailable";
        console.warn("Unable to generate split preview", error);
      }
    }
  }

  function renderBoundaryBrowser({ resetPage = false } = {}) {
    if (!review || !Number.isFinite(reviewFps)) {
      return;
    }
    if (resetPage) {
      visualPage = 0;
    }
    const entries = filteredCandidates();
    boundaryCount.textContent = `Showing ${entries.length} of ${review.candidates.length} candidates.`;
    renderBoundaryTable(entries);
    void renderVisualPage(entries);
  }

  sceneSearch.addEventListener("input", renderSceneTable);
  sceneSortControl.select.addEventListener("change", renderSceneTable);
  statusControl.select.addEventListener("change", () => renderBoundaryBrowser({ resetPage: true }));
  groupingControl.select.addEventListener("change", () =>
    renderBoundaryBrowser({ resetPage: true }),
  );
  boundarySortControl.select.addEventListener("change", () =>
    renderBoundaryBrowser({ resetPage: true }),
  );
  previousPage.addEventListener("click", () => {
    if (visualPage > 0) {
      visualPage -= 1;
      renderBoundaryBrowser();
    }
  });
  nextPage.addEventListener("click", () => {
    visualPage += 1;
    renderBoundaryBrowser();
  });
  visualGrid.addEventListener("click", (event) => {
    const button = event.target.closest("button[data-preview-boundary-frame]");
    if (!button) {
      return;
    }
    const frame = Number(button.dataset.previewBoundaryFrame);
    if (Number.isFinite(frame)) {
      seekMainVideo(frame);
    }
  });

  return {
    renderScenes(nextScenes, fps) {
      scenes = nextScenes;
      sceneFps = fps;
      renderSceneTable();
    },

    renderBoundaryReview(nextReview, fps) {
      review = nextReview;
      reviewFps = fps;
      boundaryRows.replaceChildren();
      previewAbortController?.abort();
      if (!review) {
        boundaryReview.hidden = true;
        visualGrid.replaceChildren();
        return;
      }

      boundaryReview.hidden = false;
      boundaryReviewSummary.textContent = `Rust ranked ${review.candidates.length} boundary candidate${
        review.candidates.length === 1 ? "" : "s"
      } by distance from the ${Number(review.detector_threshold).toFixed(
        3,
      )} detector threshold, using a ${Number(review.review_threshold).toFixed(
        3,
      )} review threshold. Edge case / obvious is a display-only grouping derived from that Rust-provided review band; accepted, suppressed, and near-miss decisions remain unchanged.`;
      visualPage = 0;
      renderBoundaryBrowser();
    },

    reset() {
      previewAbortController?.abort();
      previewGeneration += 1;
      scenes = [];
      sceneFps = null;
      review = null;
      reviewFps = null;
      visualPage = 0;
      sceneRows.replaceChildren();
      boundaryRows.replaceChildren();
      visualGrid.replaceChildren();
      sceneToolbar.hidden = true;
      thumbnailCache.clear();
      previewSource = "";
      previewVideo.removeAttribute("src");
      previewVideo.load();
    },
  };
}
