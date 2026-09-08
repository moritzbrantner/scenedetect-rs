function clamp(value, min, max) {
  return Math.min(max, Math.max(min, value));
}

function button(className, label) {
  const element = document.createElement("button");
  element.type = "button";
  element.className = className;
  element.textContent = label;
  return element;
}

function sortedUnique(values) {
  return [...new Set(values.map(Number).filter(Number.isFinite))].sort((left, right) => left - right);
}

function csvCell(value) {
  const text = String(value);
  return /[",\n]/u.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}

export function createReviewWorkspace({
  video,
  timelineTrack,
  timelineZoom,
  timelineStatus,
  reviewStatus,
  compareStatus,
  formatTime,
}) {
  let output = null;
  let fps = null;
  let duration = null;
  let selectedBoundary = null;
  let selectedScene = null;
  let decisions = new Map();
  let comparison = null;
  let comparisonCurrentOnly = new Set();
  let comparisonSavedOnly = [];
  let mediaBySample = new Map();
  let presentedSamples = [];

  function rawScenes() {
    return output?.detection?.scene_list?.scenes ?? [];
  }

  function finalSample() {
    const scenes = rawScenes();
    if (scenes.length) {
      return Number(scenes.at(-1).end);
    }
    return Number(output?.detection?.stats?.rows?.length ?? 0);
  }

  function rawBoundaries() {
    return rawScenes()
      .slice(1)
      .map((scene) => Number(scene.start))
      .filter(Number.isFinite);
  }

  function candidateBoundaries() {
    return (output?.boundary_review?.candidates ?? [])
      .map((candidate) => Number(candidate.frame))
      .filter(Number.isFinite);
  }

  function mediaTimeForSample(sample) {
    const numeric = Number(sample);
    if (!Number.isFinite(numeric)) {
      return 0;
    }
    if (mediaBySample.has(numeric)) {
      return mediaBySample.get(numeric);
    }
    if (numeric >= finalSample() && Number.isFinite(duration)) {
      return duration;
    }
    if (Number.isFinite(fps) && fps > 0) {
      return Math.min(Number.isFinite(duration) ? duration : Number.POSITIVE_INFINITY, numeric / fps);
    }
    return 0;
  }

  function sampleForMediaTime(time) {
    if (!presentedSamples.length) {
      return Math.round(time * fps);
    }
    let low = 0;
    let high = presentedSamples.length - 1;
    while (low < high) {
      const middle = Math.floor((low + high) / 2);
      if (Number(presentedSamples[middle].media_time_seconds) < time) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    const right = presentedSamples[low];
    const left = presentedSamples[Math.max(0, low - 1)];
    const chosen =
      Math.abs(Number(left.media_time_seconds) - time) <=
      Math.abs(Number(right.media_time_seconds) - time)
        ? left
        : right;
    return Number(chosen.sample);
  }

  function effectiveBoundaries() {
    const boundaries = new Set(rawBoundaries());
    for (const [sample, action] of decisions) {
      if (action === "reject") {
        boundaries.delete(sample);
      } else if (action === "accept" || action === "manual") {
        boundaries.add(sample);
      }
    }
    const end = finalSample();
    return sortedUnique([...boundaries]).filter((sample) => sample > 0 && sample < end);
  }

  function reviewedScenes() {
    const end = finalSample();
    if (!Number.isFinite(end) || end <= 0) {
      return [];
    }
    const boundaries = effectiveBoundaries();
    const starts = [0, ...boundaries];
    const ends = [...boundaries, end];
    return starts.map((start, index) => ({ start, end: ends[index] }));
  }

  function decisionLabel(sample) {
    const action = decisions.get(sample);
    if (action === "manual") {
      return "Manual cut";
    }
    if (action === "accept") {
      return "Accepted by reviewer";
    }
    if (action === "reject") {
      return "Rejected by reviewer";
    }
    if (rawBoundaries().includes(sample)) {
      return "Rust detector boundary";
    }
    const candidate = output?.boundary_review?.candidates?.find(
      (entry) => Number(entry.frame) === sample,
    );
    return candidate ? `Rust candidate · ${candidate.status}` : "Review point";
  }

  function seekSample(sample) {
    if (!Number.isFinite(video.duration)) {
      return;
    }
    const time = mediaTimeForSample(sample);
    video.pause();
    video.currentTime = clamp(time, 0, Math.max(0, video.duration - 0.001));
  }

  function selectBoundary(sample, { seek = true } = {}) {
    selectedBoundary = Number(sample);
    selectedScene = null;
    if (seek) {
      seekSample(selectedBoundary);
    }
    render();
  }

  function selectScene(index, { seek = true } = {}) {
    const scenes = reviewedScenes();
    if (!scenes[index]) {
      return;
    }
    selectedScene = index;
    selectedBoundary = null;
    if (seek) {
      seekSample(scenes[index].start);
    }
    render();
  }

  function renderStatus() {
    const scenes = reviewedScenes();
    if (selectedBoundary != null) {
      reviewStatus.textContent = `${decisionLabel(selectedBoundary)} at ${formatTime(
        mediaTimeForSample(selectedBoundary),
      )} · sample ${selectedBoundary}.`;
    } else if (selectedScene != null && scenes[selectedScene]) {
      const scene = scenes[selectedScene];
      reviewStatus.textContent = `Selected reviewed scene ${selectedScene + 1}: ${formatTime(
        mediaTimeForSample(scene.start),
      )} → ${formatTime(mediaTimeForSample(scene.end))}.`;
    } else {
      reviewStatus.textContent = `${scenes.length} reviewed scene${
        scenes.length === 1 ? "" : "s"
      }. Rust output is unchanged; edits are stored as a separate review decision log.`;
    }
  }

  function candidateClass(sample) {
    const candidate = output?.boundary_review?.candidates?.find(
      (entry) => Number(entry.frame) === sample,
    );
    if (!candidate) {
      return "raw";
    }
    return candidate.status === "accepted"
      ? "accepted"
      : candidate.status === "suppressed_min_scene_len"
        ? "suppressed"
        : "near-miss";
  }

  function renderTimeline() {
    timelineTrack.replaceChildren();
    if (!output || !Number.isFinite(duration) || duration <= 0) {
      timelineStatus.textContent = "Run an analysis to populate the scene timeline.";
      return;
    }

    const zoom = Number(timelineZoom.value);
    const width = Math.max(timelineTrack.parentElement.clientWidth - 2, duration * 52 * zoom);
    timelineTrack.style.width = `${Math.ceil(width)}px`;

    const scenes = reviewedScenes();
    for (const [index, scene] of scenes.entries()) {
      const start = mediaTimeForSample(scene.start);
      const end = mediaTimeForSample(scene.end);
      const element = button("timeline-scene", `Scene ${index + 1}`);
      element.style.left = `${(start / duration) * 100}%`;
      element.style.width = `${Math.max(0.35, ((end - start) / duration) * 100)}%`;
      element.dataset.sceneIndex = String(index);
      element.title = `Scene ${index + 1}: ${formatTime(start)} → ${formatTime(end)}`;
      if (selectedScene === index) {
        element.classList.add("selected");
      }
      timelineTrack.append(element);
    }

    const reviewPoints = sortedUnique([
      ...rawBoundaries(),
      ...candidateBoundaries(),
      ...[...decisions.keys()],
    ]);
    for (const sample of reviewPoints) {
      const time = mediaTimeForSample(sample);
      const marker = button(
        `timeline-boundary status-${candidateClass(sample)}`,
        decisionLabel(sample),
      );
      marker.style.left = `${(time / duration) * 100}%`;
      marker.dataset.boundarySample = String(sample);
      marker.title = `${decisionLabel(sample)} · ${formatTime(time)} · sample ${sample}`;
      const action = decisions.get(sample);
      if (action) {
        marker.classList.add(`decision-${action}`);
      }
      if (selectedBoundary === sample) {
        marker.classList.add("selected");
      }
      if (comparisonCurrentOnly.has(sample)) {
        marker.classList.add("comparison-current-only");
      }
      timelineTrack.append(marker);
    }

    for (const entry of comparisonSavedOnly) {
      const marker = document.createElement("span");
      marker.className = "timeline-comparison-saved-only";
      marker.style.left = `${(Number(entry.media_time_seconds) / duration) * 100}%`;
      marker.title = `Saved run only · ${formatTime(Number(entry.media_time_seconds))}`;
      timelineTrack.append(marker);
    }

    timelineStatus.textContent = `${scenes.length} reviewed scenes · ${effectiveBoundaries().length} effective cuts · zoom ${zoom.toFixed(
      1,
    )}×.`;
  }

  function render() {
    renderTimeline();
    renderStatus();
  }

  timelineTrack.addEventListener("click", (event) => {
    const boundary = event.target.closest("button[data-boundary-sample]");
    if (boundary) {
      selectBoundary(Number(boundary.dataset.boundarySample));
      return;
    }
    const scene = event.target.closest("button[data-scene-index]");
    if (scene) {
      selectScene(Number(scene.dataset.sceneIndex));
    }
  });

  timelineZoom.addEventListener("input", renderTimeline);

  function currentSceneIndex() {
    if (selectedScene != null) {
      return selectedScene;
    }
    const sample = sampleForMediaTime(video.currentTime || 0);
    return reviewedScenes().findIndex((scene) => sample >= scene.start && sample < scene.end);
  }

  function removeBoundary(sample) {
    if (decisions.get(sample) === "manual") {
      decisions.delete(sample);
    } else {
      decisions.set(sample, "reject");
    }
    selectedBoundary = sample;
    selectedScene = null;
    render();
  }

  function compareWith(snapshot) {
    comparison = snapshot;
    comparisonCurrentOnly = new Set();
    comparisonSavedOnly = [];
    if (!snapshot) {
      compareStatus.textContent = "Choose a saved run to compare its detector boundaries.";
      renderTimeline();
      return;
    }

    const current = rawBoundaries().map((sample) => ({
      sample,
      media_time_seconds: mediaTimeForSample(sample),
    }));
    const saved = Array.isArray(snapshot.boundaries) ? snapshot.boundaries : [];
    const usedSaved = new Set();
    const tolerance = Math.max(0.05, 0.5 / Math.max(1, fps));
    let shared = 0;

    for (const entry of current) {
      let bestIndex = -1;
      let bestDistance = Number.POSITIVE_INFINITY;
      for (let index = 0; index < saved.length; index += 1) {
        if (usedSaved.has(index)) {
          continue;
        }
        const distance = Math.abs(
          Number(entry.media_time_seconds) - Number(saved[index].media_time_seconds),
        );
        if (distance < bestDistance) {
          bestDistance = distance;
          bestIndex = index;
        }
      }
      if (bestIndex >= 0 && bestDistance <= tolerance) {
        usedSaved.add(bestIndex);
        shared += 1;
      } else {
        comparisonCurrentOnly.add(entry.sample);
      }
    }

    comparisonSavedOnly = saved.filter((_entry, index) => !usedSaved.has(index));
    compareStatus.textContent = `${shared} shared cuts · ${comparisonCurrentOnly.size} current-only · ${comparisonSavedOnly.length} saved-only · tolerance ${tolerance.toFixed(
      3,
    )}s.`;
    renderTimeline();
  }

  return {
    load(next) {
      output = next.output;
      fps = next.fps;
      duration = next.duration;
      presentedSamples = Array.isArray(output?.presented_samples) ? output.presented_samples : [];
      mediaBySample = new Map(
        presentedSamples.map((entry) => [Number(entry.sample), Number(entry.media_time_seconds)]),
      );
      selectedBoundary = null;
      selectedScene = null;
      decisions = new Map();
      comparison = null;
      comparisonCurrentOnly = new Set();
      comparisonSavedOnly = [];
      compareStatus.textContent = "Choose a saved run to compare its detector boundaries.";
      render();
    },

    reset() {
      output = null;
      fps = null;
      duration = null;
      selectedBoundary = null;
      selectedScene = null;
      decisions = new Map();
      comparison = null;
      comparisonCurrentOnly = new Set();
      comparisonSavedOnly = [];
      mediaBySample = new Map();
      presentedSamples = [];
      timelineTrack.replaceChildren();
      timelineTrack.style.width = "100%";
      timelineStatus.textContent = "Run an analysis to populate the scene timeline.";
      reviewStatus.textContent = "No review session is active.";
      compareStatus.textContent = "Choose a saved run to compare its detector boundaries.";
    },

    acceptSelectedBoundary() {
      if (selectedBoundary == null) {
        return;
      }
      decisions.set(selectedBoundary, "accept");
      render();
    },

    rejectSelectedBoundary() {
      if (selectedBoundary == null) {
        return;
      }
      decisions.set(selectedBoundary, "reject");
      render();
    },

    addCutAtPlayhead() {
      if (!output || !Number.isFinite(video.currentTime)) {
        return;
      }
      const sample = sampleForMediaTime(video.currentTime);
      if (sample <= 0 || sample >= finalSample()) {
        return;
      }
      decisions.set(sample, "manual");
      selectedBoundary = sample;
      selectedScene = null;
      render();
    },

    mergeSelectedWithNext() {
      const scenes = reviewedScenes();
      const index = currentSceneIndex();
      if (index < 0 || index >= scenes.length - 1) {
        return;
      }
      removeBoundary(scenes[index].end);
      selectedScene = index;
      selectedBoundary = null;
      render();
    },

    mergeSelectedWithPrevious() {
      const scenes = reviewedScenes();
      const index = currentSceneIndex();
      if (index <= 0 || !scenes[index]) {
        return;
      }
      removeBoundary(scenes[index].start);
      selectedScene = index - 1;
      selectedBoundary = null;
      render();
    },

    resetReview() {
      decisions = new Map();
      selectedBoundary = null;
      selectedScene = null;
      render();
    },

    seekBoundary(direction) {
      const points = sortedUnique([
        ...rawBoundaries(),
        ...candidateBoundaries(),
        ...[...decisions.keys()],
      ]);
      if (!points.length) {
        return;
      }
      const currentSample =
        selectedBoundary ?? sampleForMediaTime(Number.isFinite(video.currentTime) ? video.currentTime : 0);
      const next =
        direction > 0
          ? points.find((sample) => sample > currentSample) ?? points[0]
          : [...points].reverse().find((sample) => sample < currentSample) ?? points.at(-1);
      selectBoundary(next);
    },

    seekScene(direction) {
      const scenes = reviewedScenes();
      if (!scenes.length) {
        return;
      }
      const current = currentSceneIndex();
      const next = clamp((current < 0 ? 0 : current) + direction, 0, scenes.length - 1);
      selectScene(next);
    },

    zoomBy(delta) {
      const min = Number(timelineZoom.min || 1);
      const max = Number(timelineZoom.max || 8);
      timelineZoom.value = String(clamp(Number(timelineZoom.value) + delta, min, max));
      renderTimeline();
    },

    compareWith,

    loadReviewDecisions(nextDecisions) {
      decisions = new Map(
        (Array.isArray(nextDecisions) ? nextDecisions : [])
          .map((entry) => [Number(entry.sample), entry.action])
          .filter(
            ([sample, action]) =>
              Number.isFinite(sample) && ["accept", "reject", "manual"].includes(action),
          ),
      );
      render();
    },

    reviewArtifact() {
      return {
        schema_version: 1,
        authority: {
          detector: "rust",
          review: "human-browser",
        },
        decisions: [...decisions]
          .map(([sample, action]) => ({
            sample,
            media_time_seconds: mediaTimeForSample(sample),
            action,
          }))
          .sort((left, right) => left.sample - right.sample),
        detector_scene_list: output?.detection?.scene_list ?? null,
        reviewed_scene_list: { scenes: reviewedScenes() },
        presented_samples: presentedSamples,
      };
    },

    reviewedCsv() {
      const rows = [["Scene", "Start Sample", "Start Time", "End Sample", "End Time"]];
      for (const [index, scene] of reviewedScenes().entries()) {
        rows.push([
          index + 1,
          scene.start,
          formatTime(mediaTimeForSample(scene.start)),
          scene.end,
          formatTime(mediaTimeForSample(scene.end)),
        ]);
      }
      return rows.map((row) => row.map(csvCell).join(",")).join("\n") + "\n";
    },

    snapshot({ id, label, media, settings }) {
      return {
        id,
        label,
        media,
        settings,
        boundaries: rawBoundaries().map((sample) => ({
          sample,
          media_time_seconds: mediaTimeForSample(sample),
        })),
      };
    },

    sessionArtifact({ media, settings }) {
      return {
        schema_version: 1,
        media,
        settings,
        detector_snapshot: {
          scenes: rawScenes(),
          boundaries: rawBoundaries().map((sample) => ({
            sample,
            media_time_seconds: mediaTimeForSample(sample),
          })),
        },
        review: this.reviewArtifact(),
      };
    },
  };
}
