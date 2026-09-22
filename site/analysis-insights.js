import { createLocalFramePreviewer } from "./local-frame-previewer.js";

const THRESHOLD_SLIDER_MAX = 1000;
const THRESHOLD_SLIDER_CURVE = 1000;

function firstMetric(metrics, prefix) {
  return Object.entries(metrics ?? {}).find(([name]) => name.startsWith(prefix))?.[1] ?? 0;
}

export function detectorMetricSpec(settings) {
  const detector = settings?.detector;
  const config = settings?.detector_config ?? {};
  if (detector === "content") {
    return {
      label: "Content score",
      threshold: Number(config.threshold),
      tunable: true,
      score: (row) => Number(row.metrics?.content_val ?? 0),
      eligible: () => true,
    };
  }
  if (detector === "adaptive") {
    return {
      label: "Adaptive ratio",
      threshold: Number(config.threshold),
      tunable: true,
      score: (row) => Number(row.metrics?.adaptive_ratio ?? 0),
      eligible: (row) => Number(row.metrics?.content_val ?? 0) >= Number(config.min_content_val ?? 0),
    };
  }
  if (detector === "histogram") {
    return {
      label: "Histogram distance",
      threshold: Number(config.threshold),
      tunable: true,
      score: (row) =>
        Number(row.frame) === 0 ? 0 : 1 - Number(firstMetric(row.metrics, "hist_diff")),
      eligible: () => true,
    };
  }
  if (detector === "hash") {
    return {
      label: "Perceptual hash distance",
      threshold: Number(config.threshold),
      tunable: true,
      score: (row) => Number(firstMetric(row.metrics, "hash_dist")),
      eligible: () => true,
    };
  }
  return {
    label: "Average luminance",
    threshold: Number(config.threshold),
    tunable: false,
    score: (row) => Number(row.metrics?.average_rgb ?? 0),
    eligible: () => true,
  };
}

export function buildScoreSeries(output, settings) {
  const spec = detectorMetricSpec(settings);
  const rows = output?.detection?.stats?.rows ?? [];
  return {
    spec,
    series: rows.map((row) => ({
      frame: Number(row.frame),
      score: spec.score(row),
      eligible: spec.eligible(row),
    })),
  };
}

export function previewCandidateFrames(series, threshold) {
  return series
    .filter(
      (entry) =>
        entry.frame > 0 && entry.eligible && Number.isFinite(entry.score) && entry.score >= threshold,
    )
    .map((entry) => entry.frame);
}

export function thresholdBoundaryChanges(series, baselineThreshold, previewThreshold) {
  const baseline = new Set(previewCandidateFrames(series, baselineThreshold));
  const preview = new Set(previewCandidateFrames(series, previewThreshold));
  const added = [...preview].filter((frame) => !baseline.has(frame)).sort((left, right) => left - right);
  const removed = [...baseline]
    .filter((frame) => !preview.has(frame))
    .sort((left, right) => left - right);
  const changed = [
    ...added.map((frame) => ({ frame, kind: "added" })),
    ...removed.map((frame) => ({ frame, kind: "removed" })),
  ].sort((left, right) => left.frame - right.frame);
  return { added, removed, changed };
}

export function maximumSeriesScore(series, initial = 0) {
  let maximum = initial;
  for (const entry of series) {
    if (Number.isFinite(entry.score) && entry.score > maximum) {
      maximum = entry.score;
    }
  }
  return maximum;
}

export function scenePreviewSamples(scene, count = 3) {
  const start = Math.trunc(Number(scene?.start));
  const end = Math.trunc(Number(scene?.end));
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) {
    return Number.isFinite(start) ? [Math.max(0, start)] : [];
  }
  const first = Math.max(0, start);
  const last = Math.max(first, end - 1);
  const total = Math.max(1, Math.floor(Number(count) || 1));
  if (total === 1 || first === last) {
    return [first];
  }
  return [
    ...new Set(
      Array.from({ length: total }, (_value, index) =>
        Math.round(first + ((last - first) * index) / (total - 1)),
      ),
    ),
  ];
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

export function thresholdFromSliderPosition(position, maximum) {
  const safeMaximum = Math.max(0, Number(maximum) || 0);
  if (safeMaximum === 0) {
    return 0;
  }
  const fraction = clamp((Number(position) || 0) / THRESHOLD_SLIDER_MAX, 0, 1);
  return (
    safeMaximum *
    Math.expm1(Math.log1p(THRESHOLD_SLIDER_CURVE) * fraction) /
    THRESHOLD_SLIDER_CURVE
  );
}

export function thresholdSliderPosition(threshold, maximum) {
  const safeMaximum = Math.max(0, Number(maximum) || 0);
  if (safeMaximum === 0) {
    return 0;
  }
  const normalized = clamp((Number(threshold) || 0) / safeMaximum, 0, 1);
  const fraction =
    Math.log1p(normalized * THRESHOLD_SLIDER_CURVE) / Math.log1p(THRESHOLD_SLIDER_CURVE);
  return Math.round(fraction * THRESHOLD_SLIDER_MAX);
}

function editableThreshold(value) {
  if (!Number.isFinite(value)) {
    return "";
  }
  return String(Number(value.toPrecision(10)));
}

function canvasPixelSize(canvas) {
  const ratio = globalThis.devicePixelRatio || 1;
  return {
    width: Math.max(320, Math.round((canvas.clientWidth || 720) * ratio)),
    height: Math.max(160, Math.round((canvas.clientHeight || 180) * ratio)),
    ratio,
  };
}

function drawHeatmap(canvas, series, threshold, candidates) {
  const context = canvas.getContext("2d");
  if (!context) {
    return;
  }
  const { width, height, ratio } = canvasPixelSize(canvas);
  if (canvas.width !== width || canvas.height !== height) {
    canvas.width = width;
    canvas.height = height;
  }
  context.clearRect(0, 0, width, height);
  if (series.length === 0) {
    return;
  }

  const maxScore = Math.max(1e-9, threshold, maximumSeriesScore(series));
  const plotHeight = height - 28 * ratio;
  const columnMax = new Float64Array(width);
  for (let index = 0; index < series.length; index += 1) {
    const x = Math.min(width - 1, Math.floor((index / Math.max(1, series.length - 1)) * (width - 1)));
    const normalized = clamp(series[index].score / maxScore, 0, 1);
    columnMax[x] = Math.max(columnMax[x], normalized);
  }

  context.fillStyle = "rgba(90, 118, 165, 0.5)";
  for (let x = 0; x < width; x += 1) {
    const normalized = columnMax[x];
    if (normalized <= 0) {
      continue;
    }
    const barHeight = normalized * plotHeight;
    context.fillRect(x, plotHeight - barHeight, 1, barHeight);
  }

  const thresholdY = plotHeight - clamp(threshold / maxScore, 0, 1) * plotHeight;
  context.strokeStyle = "rgba(25, 25, 25, 0.8)";
  context.lineWidth = Math.max(1, ratio);
  context.setLineDash([6 * ratio, 4 * ratio]);
  context.beginPath();
  context.moveTo(0, thresholdY);
  context.lineTo(width, thresholdY);
  context.stroke();
  context.setLineDash([]);

  context.strokeStyle = "rgba(180, 70, 70, 0.85)";
  for (const frame of candidates) {
    const x = Math.round((frame / Math.max(1, series.length - 1)) * (width - 1));
    context.beginPath();
    context.moveTo(x, plotHeight);
    context.lineTo(x, Math.max(0, plotHeight - 9 * ratio));
    context.stroke();
  }

  context.fillStyle = "rgba(25, 25, 25, 0.78)";
  context.font = `${12 * ratio}px system-ui, sans-serif`;
  context.fillText("0", 4 * ratio, height - 6 * ratio);
  context.fillText(maxScore.toFixed(maxScore < 2 ? 3 : 1), 4 * ratio, 14 * ratio);
}

function sceneForPair(output, sceneNumber, fallbackStart) {
  const scenes = output?.detection?.scene_list?.scenes ?? [];
  const indexed = scenes[Number(sceneNumber) - 1];
  if (indexed && Number(indexed.start) === Number(fallbackStart)) {
    return indexed;
  }
  return scenes.find((scene) => Number(scene.start) === Number(fallbackStart)) ?? indexed ?? null;
}

export function createAnalysisInsights({
  heatmapCanvas,
  metricLabel,
  thresholdControls,
  thresholdInput,
  thresholdNumberInput,
  candidateSummary,
  thresholdPreviewVideo,
  thresholdImpactSummary,
  thresholdPreviousChangeButton,
  thresholdNextChangeButton,
  applyThresholdButton,
  similarityThreshold,
  similarityStatus,
  similarityList,
  video,
  mediaTimeForSample,
  seekSample,
  applyThresholdAndRerun,
}) {
  let current = null;
  let thresholdMaximum = 1;
  let thresholdChanges = [];
  let previousThresholdChange = null;
  let nextThresholdChange = null;
  let syncingThresholdPreviewVideo = false;
  let similarityPreviewController = null;
  let similarityPreviewGeneration = 0;
  const similarityPreviewer = createLocalFramePreviewer(video, {
    cacheLimit: 128,
    maxWidth: 260,
    maxHeight: 146,
    quality: 0.8,
  });

  function currentThreshold() {
    const raw = thresholdNumberInput.value.trim();
    return raw === "" ? Number.NaN : Number(raw);
  }

  function ensureThresholdMaximum(threshold) {
    if (Number.isFinite(threshold) && threshold > thresholdMaximum) {
      thresholdMaximum = Math.max(threshold * 1.05, threshold + Number.EPSILON);
    }
  }

  function syncSliderFromNumber() {
    const threshold = currentThreshold();
    if (!Number.isFinite(threshold) || threshold < 0) {
      return false;
    }
    ensureThresholdMaximum(threshold);
    thresholdInput.value = String(thresholdSliderPosition(threshold, thresholdMaximum));
    return true;
  }

  function setThresholdFromSlider() {
    const threshold = thresholdFromSliderPosition(thresholdInput.value, thresholdMaximum);
    thresholdNumberInput.value = editableThreshold(threshold);
  }

  function syncThresholdPreviewVideo() {
    if (!thresholdPreviewVideo || !video) {
      return;
    }
    const source = video.currentSrc || video.src;
    if (!source) {
      thresholdPreviewVideo.hidden = true;
      return;
    }
    thresholdPreviewVideo.hidden = false;
    if (thresholdPreviewVideo.dataset.source !== source) {
      syncingThresholdPreviewVideo = true;
      thresholdPreviewVideo.src = source;
      thresholdPreviewVideo.dataset.source = source;
      thresholdPreviewVideo.load();
      syncingThresholdPreviewVideo = false;
    }
    thresholdPreviewVideo.playbackRate = video.playbackRate || 1;
    const targetTime = Number(video.currentTime);
    if (
      Number.isFinite(targetTime) &&
      thresholdPreviewVideo.readyState >= HTMLMediaElement.HAVE_METADATA &&
      Math.abs(Number(thresholdPreviewVideo.currentTime) - targetTime) > 0.04
    ) {
      syncingThresholdPreviewVideo = true;
      thresholdPreviewVideo.currentTime = targetTime;
      syncingThresholdPreviewVideo = false;
    }
    if (video.paused) {
      thresholdPreviewVideo.pause();
    } else if (thresholdPreviewVideo.paused) {
      void thresholdPreviewVideo.play().catch(() => {});
    }
  }

  function renderThresholdImpact() {
    previousThresholdChange = null;
    nextThresholdChange = null;
    if (!current) {
      thresholdImpactSummary.textContent = "";
      thresholdPreviousChangeButton.disabled = true;
      thresholdNextChangeButton.disabled = true;
      return;
    }
    if (!current.spec.tunable) {
      thresholdImpactSummary.textContent =
        "Stateful fade semantics require an authoritative Rust rerun before Scene Boundary impact can be shown.";
      thresholdPreviousChangeButton.disabled = true;
      thresholdNextChangeButton.disabled = true;
      return;
    }
    if (thresholdChanges.length === 0) {
      thresholdImpactSummary.textContent =
        "No raw threshold crossings change relative to the analyzed threshold at this preview value.";
      thresholdPreviousChangeButton.disabled = true;
      thresholdNextChangeButton.disabled = true;
      return;
    }

    const playhead = Number(video?.currentTime) || 0;
    const timed = thresholdChanges
      .map((change) => ({ ...change, time: Number(mediaTimeForSample?.(change.frame)) }))
      .filter((change) => Number.isFinite(change.time));
    previousThresholdChange = [...timed].reverse().find((change) => change.time <= playhead) ?? null;
    nextThresholdChange = timed.find((change) => change.time > playhead) ?? null;
    const nearest = timed.reduce(
      (best, change) =>
        !best || Math.abs(change.time - playhead) < Math.abs(best.time - playhead) ? change : best,
      null,
    );
    const nearestText = nearest
      ? ` Nearest affected sample: ${nearest.frame} at ${nearest.time.toFixed(3)}s (${
          nearest.kind === "added" ? "potential split" : "potential merge"
        } before authoritative minimum-scene-length handling).`
      : "";
    const added = thresholdChanges.filter((change) => change.kind === "added").length;
    const removed = thresholdChanges.length - added;
    thresholdImpactSummary.textContent =
      `${added} potential new raw crossing${added === 1 ? "" : "s"}, ${removed} removed raw crossing${
        removed === 1 ? "" : "s"
      } versus the analyzed threshold.${nearestText}`;
    thresholdPreviousChangeButton.disabled = !previousThresholdChange;
    thresholdNextChangeButton.disabled = !nextThresholdChange;
  }

  function renderThresholdPreview() {
    if (!current) {
      return;
    }
    const threshold = currentThreshold();
    if (!Number.isFinite(threshold) || threshold < 0) {
      candidateSummary.textContent = "Enter a non-negative threshold value.";
      applyThresholdButton.disabled = true;
      return;
    }
    const candidates = current.spec.tunable
      ? previewCandidateFrames(current.series, threshold)
      : [];
    thresholdChanges = current.spec.tunable
      ? thresholdBoundaryChanges(current.series, current.spec.threshold, threshold).changed
      : [];
    if (current.spec.tunable) {
      const added = thresholdChanges.filter((change) => change.kind === "added").length;
      const removed = thresholdChanges.length - added;
      candidateSummary.textContent = `${candidates.length} raw threshold crossing${
        candidates.length === 1 ? "" : "s"
      } at ${editableThreshold(threshold)} · ${added} added / ${removed} removed versus analyzed threshold ${editableThreshold(
        current.spec.threshold,
      )}. This preview intentionally does not reproduce minimum-scene-length suppression; rerun to obtain authoritative Rust scenes.`;
    } else {
      candidateSummary.textContent =
        "Fade detection is stateful, so threshold-only boundary preview is disabled. The heatmap still shows the Rust luminance metric; use the detector controls and rerun for authoritative fade semantics.";
    }
    applyThresholdButton.disabled = !current.spec.tunable;
    drawHeatmap(heatmapCanvas, current.series, threshold, candidates);
    syncThresholdPreviewVideo();
    renderThresholdImpact();
  }


  function cancelSimilarityPreviews() {
    similarityPreviewController?.abort();
    similarityPreviewController = null;
    similarityPreviewGeneration += 1;
  }

  function scenePreview(sceneNumber, fallbackStart) {
    const scene = sceneForPair(current?.output, sceneNumber, fallbackStart) ?? {
      start: Number(fallbackStart),
      end: Number(fallbackStart) + 1,
    };
    const figure = document.createElement("figure");
    figure.className = "similarity-scene-preview";
    const caption = document.createElement("figcaption");
    caption.textContent = `Scene ${sceneNumber}`;
    const strip = document.createElement("div");
    strip.className = "similarity-filmstrip";
    const frames = scenePreviewSamples(scene, 3).map((sample, index, samples) => {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "similarity-preview-frame";
      button.title = `Open Scene ${sceneNumber} at sample ${sample}`;
      button.addEventListener("click", () => seekSample(sample));
      const image = document.createElement("img");
      image.alt = `Scene ${sceneNumber}, preview ${index + 1} of ${samples.length}`;
      const placeholder = document.createElement("span");
      placeholder.className = "similarity-preview-placeholder";
      placeholder.textContent = "Loading…";
      button.append(image, placeholder);
      strip.append(button);
      return { image, placeholder, sample };
    });
    figure.append(caption, strip);
    return { figure, frames };
  }

  async function hydratePairPreviews(pairPreviews, signal, generation) {
    const maxFrames = Math.max(0, ...pairPreviews.map((preview) => preview.frames.length));
    for (let index = 0; index < maxFrames; index += 1) {
      for (const preview of pairPreviews) {
        const frame = preview.frames[index];
        if (!frame) {
          continue;
        }
        try {
          const time = Number(mediaTimeForSample?.(frame.sample));
          const url = await similarityPreviewer.capture(Number.isFinite(time) ? time : 0, {
            signal,
            maxWidth: 260,
            maxHeight: 146,
            quality: 0.8,
          });
          if (signal.aborted || generation !== similarityPreviewGeneration) {
            return;
          }
          frame.image.src = url;
          frame.placeholder.hidden = true;
        } catch (error) {
          if (error?.name === "AbortError") {
            return;
          }
          frame.placeholder.textContent = "Preview unavailable";
          console.warn("Unable to generate scene-similarity preview", error);
        }
      }
    }
  }

  async function hydrateSimilarityPreviews(rows, signal, generation) {
    for (const row of rows) {
      if (signal.aborted || generation !== similarityPreviewGeneration) {
        return;
      }
      await hydratePairPreviews(row, signal, generation);
    }
  }

  function renderSimilarity() {
    cancelSimilarityPreviews();
    const report = current?.output?.scene_similarity;
    if (!report) {
      similarityStatus.textContent = "No Rust scene-similarity report is available for this run.";
      similarityList.replaceChildren();
      return;
    }
    const minimum = Number(similarityThreshold.value);
    const matches = (report.pairs ?? []).filter((pair) => Number(pair.similarity) >= minimum);
    const selected = Number(report.scenes_selected ?? report.scenes_considered ?? 0);
    const fingerprinted = Number(report.scenes_considered ?? 0);
    const total = Number(report.total_scenes ?? selected);
    const targetRate = Number(report.target_hashes_per_second ?? 1);
    const selectionScope = report.truncated
      ? `Selected ${selected} evenly distributed scenes from ${total}.`
      : `Selected all ${total} detected scenes.`;
    const coverage = `${fingerprinted} selected scene${fingerprinted === 1 ? " has" : "s have"} pHash evidence.`;
    const omitted = Number(report.scenes_without_fingerprint ?? 0);
    const omissionNote = omitted > 0
      ? ` ${omitted} selected scene${omitted === 1 ? " was" : "s were"} too short to receive the bounded visual sample and ${omitted === 1 ? "is" : "are"} omitted from pairwise comparison.`
      : "";
    similarityStatus.textContent = `${selectionScope} ${coverage}${omissionNote} Rust caps shared pHash work at about ${targetRate} visual hash${targetRate === 1 ? "" : "es"}/second and uses the shared visual-analysis DCT perceptual hash; Hamming distance is recurrence/near-duplicate evidence, not semantic classification. Each reported match below shows local three-frame strips from both scenes; click any frame to open it in the video.`;

    const previewRows = [];
    const visibleMatches = matches.slice(0, 16);
    similarityList.replaceChildren(
      ...visibleMatches.map((pair) => {
        const row = document.createElement("article");
        row.className = "similarity-row";
        const description = document.createElement("div");
        description.className = "similarity-description";
        const score = document.createElement("strong");
        score.textContent = `${(Number(pair.similarity) * 100).toFixed(1)}%`;
        const label = document.createElement("span");
        const distance = Number.isFinite(Number(pair.hash_distance))
          ? ` · pHash distance ${pair.hash_distance}/64`
          : "";
        label.textContent = `Scene ${pair.first_scene} ↔ Scene ${pair.second_scene}${distance}`;
        description.append(score, label);

        const first = scenePreview(pair.first_scene, pair.first_start);
        const second = scenePreview(pair.second_scene, pair.second_start);
        const previews = document.createElement("div");
        previews.className = "similarity-preview-pair";
        const divider = document.createElement("span");
        divider.className = "similarity-pair-divider";
        divider.setAttribute("aria-hidden", "true");
        divider.textContent = "↔";
        previews.append(first.figure, divider, second.figure);
        previewRows.push([first, second]);

        const actions = document.createElement("div");
        actions.className = "similarity-actions";
        for (const [labelText, sample] of [
          [`Open ${pair.first_scene}`, pair.first_start],
          [`Open ${pair.second_scene}`, pair.second_start],
        ]) {
          const button = document.createElement("button");
          button.type = "button";
          button.className = "action-button";
          button.textContent = labelText;
          button.addEventListener("click", () => seekSample(Number(sample)));
          actions.append(button);
        }
        row.append(description, actions, previews);
        return row;
      }),
    );

    if (matches.length === 0) {
      const empty = document.createElement("p");
      empty.className = "support-text";
      empty.textContent = `No reported pair reaches ${(minimum * 100).toFixed(0)}% similarity.`;
      similarityList.append(empty);
      return;
    }

    similarityPreviewController = new AbortController();
    const generation = similarityPreviewGeneration;
    void hydrateSimilarityPreviews(previewRows, similarityPreviewController.signal, generation);
  }

  thresholdInput.addEventListener("input", () => {
    setThresholdFromSlider();
    renderThresholdPreview();
  });
  thresholdNumberInput.addEventListener("input", () => {
    if (syncSliderFromNumber()) {
      renderThresholdPreview();
    }
  });
  thresholdPreviousChangeButton.addEventListener("click", () => {
    if (previousThresholdChange) {
      seekSample(previousThresholdChange.frame);
    }
  });
  thresholdNextChangeButton.addEventListener("click", () => {
    if (nextThresholdChange) {
      seekSample(nextThresholdChange.frame);
    }
  });
  for (const eventName of ["loadedmetadata", "seeked", "timeupdate", "play", "pause", "ratechange"]) {
    video?.addEventListener(eventName, () => {
      if (!syncingThresholdPreviewVideo) {
        syncThresholdPreviewVideo();
      }
      if (eventName === "seeked" || eventName === "timeupdate") {
        renderThresholdImpact();
      }
    });
  }
  thresholdPreviewVideo?.addEventListener("loadedmetadata", syncThresholdPreviewVideo);
  similarityThreshold.addEventListener("input", renderSimilarity);
  applyThresholdButton.addEventListener("click", () => {
    if (!current?.spec.tunable) {
      return;
    }
    const threshold = currentThreshold();
    if (Number.isFinite(threshold) && threshold >= 0) {
      void applyThresholdAndRerun(threshold);
    }
  });

  return {
    load({ output, settings }) {
      const { spec, series } = buildScoreSeries(output, settings);
      const threshold = Number.isFinite(spec.threshold) ? spec.threshold : 0;
      const maximum = Math.max(1, threshold * 2, maximumSeriesScore(series) * 1.05);
      thresholdMaximum = maximum;
      thresholdInput.min = "0";
      thresholdInput.max = String(THRESHOLD_SLIDER_MAX);
      thresholdInput.step = "1";
      thresholdNumberInput.min = "0";
      thresholdNumberInput.step = "any";
      thresholdNumberInput.value = editableThreshold(clamp(threshold, 0, maximum));
      thresholdInput.value = String(thresholdSliderPosition(threshold, maximum));
      thresholdControls.hidden = false;
      metricLabel.textContent = `${spec.label} across the analyzed timeline`;
      current = { output, settings, spec, series };
      renderThresholdPreview();
      renderSimilarity();
    },
    reset() {
      cancelSimilarityPreviews();
      current = null;
      thresholdChanges = [];
      previousThresholdChange = null;
      nextThresholdChange = null;
      metricLabel.textContent = "Run an analysis to inspect detector scores.";
      candidateSummary.textContent = "";
      thresholdImpactSummary.textContent = "";
      thresholdPreviousChangeButton.disabled = true;
      thresholdNextChangeButton.disabled = true;
      thresholdControls.hidden = true;
      if (thresholdPreviewVideo) {
        thresholdPreviewVideo.pause();
        thresholdPreviewVideo.removeAttribute("src");
        thresholdPreviewVideo.dataset.source = "";
        thresholdPreviewVideo.load();
        thresholdPreviewVideo.hidden = true;
      }
      similarityStatus.textContent = "Run an analysis to compare visually recurring scenes.";
      similarityList.replaceChildren();
      const context = heatmapCanvas.getContext("2d");
      context?.clearRect(0, 0, heatmapCanvas.width, heatmapCanvas.height);
    },
  };
}
