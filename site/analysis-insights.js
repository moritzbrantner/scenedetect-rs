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

export function maximumSeriesScore(series, initial = 0) {
  let maximum = initial;
  for (const entry of series) {
    if (Number.isFinite(entry.score) && entry.score > maximum) {
      maximum = entry.score;
    }
  }
  return maximum;
}

function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
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

export function createAnalysisInsights({
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
}) {
  let current = null;

  function renderThresholdPreview() {
    if (!current) {
      return;
    }
    const threshold = Number(thresholdInput.value);
    const candidates = current.spec.tunable
      ? previewCandidateFrames(current.series, threshold)
      : [];
    thresholdValue.textContent = Number.isFinite(threshold) ? threshold.toFixed(3) : "—";
    if (current.spec.tunable) {
      candidateSummary.textContent = `${candidates.length} raw threshold crossing${
        candidates.length === 1 ? "" : "s"
      }. This preview intentionally does not reproduce minimum-scene-length suppression; rerun to obtain authoritative Rust scenes.`;
    } else {
      candidateSummary.textContent =
        "Fade detection is stateful, so threshold-only boundary preview is disabled. The heatmap still shows the Rust luminance metric; use the detector controls and rerun for authoritative fade semantics.";
    }
    applyThresholdButton.disabled = !current.spec.tunable;
    drawHeatmap(heatmapCanvas, current.series, threshold, candidates);
  }

  function renderSimilarity() {
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
    similarityStatus.textContent = `${selectionScope} ${coverage}${omissionNote} Rust samples at about ${targetRate} visual hash${targetRate === 1 ? "" : "es"}/second and uses the shared visual-analysis DCT perceptual hash; Hamming distance is recurrence/near-duplicate evidence, not semantic classification.`;

    similarityList.replaceChildren(
      ...matches.slice(0, 16).map((pair) => {
        const row = document.createElement("div");
        row.className = "similarity-row";
        const description = document.createElement("div");
        description.className = "similarity-description";
        const score = document.createElement("strong");
        score.textContent = `${(Number(pair.similarity) * 100).toFixed(1)}%`;
        const label = document.createElement("span");
        label.textContent = `Scene ${pair.first_scene} ↔ Scene ${pair.second_scene} · pHash distance ${pair.hash_distance}/64`;
        description.append(score, label);

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
        row.append(description, actions);
        return row;
      }),
    );

    if (matches.length === 0) {
      const empty = document.createElement("p");
      empty.className = "support-text";
      empty.textContent = `No reported pair reaches ${(minimum * 100).toFixed(0)}% similarity.`;
      similarityList.append(empty);
    }
  }

  thresholdInput.addEventListener("input", renderThresholdPreview);
  similarityThreshold.addEventListener("input", renderSimilarity);
  applyThresholdButton.addEventListener("click", () => {
    if (!current?.spec.tunable) {
      return;
    }
    const threshold = Number(thresholdInput.value);
    if (Number.isFinite(threshold)) {
      void applyThresholdAndRerun(threshold);
    }
  });

  return {
    load({ output, settings }) {
      const { spec, series } = buildScoreSeries(output, settings);
      const threshold = Number.isFinite(spec.threshold) ? spec.threshold : 0;
      const maximum = Math.max(1, threshold * 2, maximumSeriesScore(series) * 1.05);
      thresholdInput.min = "0";
      thresholdInput.max = String(maximum);
      thresholdInput.step = String(maximum <= 2 ? 0.001 : 0.1);
      thresholdInput.value = String(clamp(threshold, 0, maximum));
      thresholdControls.hidden = false;
      metricLabel.textContent = `${spec.label} across the analyzed timeline`;
      current = { output, settings, spec, series };
      renderThresholdPreview();
      renderSimilarity();
    },
    reset() {
      current = null;
      metricLabel.textContent = "Run an analysis to inspect detector scores.";
      candidateSummary.textContent = "";
      thresholdControls.hidden = true;
      similarityStatus.textContent = "Run an analysis to compare visually recurring scenes.";
      similarityList.replaceChildren();
      const context = heatmapCanvas.getContext("2d");
      context?.clearRect(0, 0, heatmapCanvas.width, heatmapCanvas.height);
    },
  };
}
