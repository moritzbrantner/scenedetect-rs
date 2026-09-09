import assert from "node:assert/strict";
import test from "node:test";

import { createReviewWorkspace } from "./review-workspace.js";
import { detectorSnapshotsMatch, reviewRestorationResult } from "./session-store.js";

const baselineOutput = {
  detection: {
    scene_list: {
      scenes: [
        { start: 0, end: 12 },
        { start: 12, end: 24 },
      ],
    },
    stats: {
      rows: [
        { frame: 0, score: 1.5 },
        { frame: 12, score: 42.5 },
        { frame: 18, score: 2.1 },
      ],
    },
  },
  boundary_review: {
    detector: "content",
    score_metric: "content_val",
    candidates: [
      {
        frame: 12,
        status: "accepted",
        score: 42.5,
        threshold_distance: 15.5,
      },
      {
        frame: 18,
        status: "near_miss",
        score: 24.5,
        threshold_distance: -2.5,
      },
    ],
  },
  presented_samples: [
    { sample: 0, media_time_seconds: 0 },
    { sample: 12, media_time_seconds: 1.375 },
    { sample: 18, media_time_seconds: 2.05 },
    { sample: 24, media_time_seconds: 3.0 },
  ],
};

const media = { name: "clip.mp4", size: 1234, last_modified: 5678 };
const settings = {
  schema_version: 1,
  detector: "content",
  analysis_fps: 8,
  max_dimension: 640,
  detector_config: { detector: "content", threshold: 27 },
};

function fakeElement(overrides = {}) {
  return Object.assign(
    {
      style: {},
      dataset: {},
      classList: { add() {} },
      children: [],
      value: "1",
      min: "1",
      max: "8",
      parentElement: { clientWidth: 800 },
      textContent: "",
      addEventListener() {},
      replaceChildren(...children) {
        this.children = children;
      },
      append(...children) {
        this.children.push(...children);
      },
      closest() {
        return null;
      },
    },
    overrides,
  );
}

globalThis.document = {
  createElement() {
    return fakeElement();
  },
};

function createWorkspace(output = baselineOutput) {
  const video = {
    duration: 3,
    currentTime: 0,
    paused: true,
    pause() {
      this.paused = true;
    },
  };
  const workspace = createReviewWorkspace({
    video,
    timelineTrack: fakeElement(),
    timelineZoom: fakeElement({ value: "1" }),
    timelineStatus: fakeElement(),
    reviewStatus: fakeElement(),
    compareStatus: fakeElement(),
    formatTime: (value) => String(value),
  });
  workspace.load({ output: structuredClone(output), fps: 8, duration: video.duration });
  return { workspace, video };
}

function createReviewedSession() {
  const { workspace } = createWorkspace();
  workspace.seekBoundary(1);
  workspace.seekBoundary(1);
  workspace.acceptSelectedBoundary();
  const session = workspace.sessionArtifact({ media, settings });
  assert.deepEqual(session.review.decisions, [
    { sample: 18, media_time_seconds: 2.05, action: "accept" },
  ]);
  return session;
}

function currentIdentity(output = baselineOutput) {
  const { workspace } = createWorkspace(output);
  return {
    media: structuredClone(media),
    settings: structuredClone(settings),
    detector_snapshot: workspace.sessionArtifact({ media: null, settings: null }).detector_snapshot,
  };
}

test("detector snapshots match independent of object key order", () => {
  const left = currentIdentity().detector_snapshot;
  const right = {
    presented_samples: structuredClone(left.presented_samples),
    boundary_review: structuredClone(left.boundary_review),
    detection: structuredClone(left.detection),
    boundaries: left.boundaries.map(({ sample, media_time_seconds }) => ({
      media_time_seconds,
      sample,
    })),
    scenes: left.scenes.map(({ start, end }) => ({ end, start })),
  };

  assert.equal(detectorSnapshotsMatch(left, right), true);
});

test("detector snapshots reject a changed scene boundary", () => {
  const left = currentIdentity().detector_snapshot;
  const changed = structuredClone(left);
  changed.scenes[0].end = 13;
  changed.scenes[1].start = 13;
  changed.boundaries[0].sample = 13;

  assert.equal(detectorSnapshotsMatch(left, changed), false);
});

test("detector snapshots reject a changed boundary media time", () => {
  const left = currentIdentity().detector_snapshot;
  const changed = structuredClone(left);
  changed.boundaries[0].media_time_seconds = 1.5;

  assert.equal(detectorSnapshotsMatch(left, changed), false);
});

test("detector snapshots reject a changed review candidate", () => {
  const left = currentIdentity().detector_snapshot;
  const changed = structuredClone(left);
  changed.boundary_review.candidates[1].status = "accepted";

  assert.equal(detectorSnapshotsMatch(left, changed), false);
});

test("detector snapshots reject changed detection stats", () => {
  const left = currentIdentity().detector_snapshot;
  const changed = structuredClone(left);
  changed.detection.stats.rows[2].score = 3.2;

  assert.equal(detectorSnapshotsMatch(left, changed), false);
});

test("detector snapshots reject changed non-boundary presentation timing", () => {
  const left = currentIdentity().detector_snapshot;
  const changed = structuredClone(left);
  changed.presented_samples[2].media_time_seconds = 2.15;

  assert.equal(detectorSnapshotsMatch(left, changed), false);
});

test("detector snapshots reject missing complete-output identity", () => {
  const left = currentIdentity().detector_snapshot;
  const incomplete = structuredClone(left);
  delete incomplete.detection;

  assert.equal(detectorSnapshotsMatch(left, incomplete), false);
});

test("workspace session exports complete detector and presentation identity", () => {
  const snapshot = currentIdentity().detector_snapshot;

  assert.deepEqual(snapshot.detection, baselineOutput.detection);
  assert.deepEqual(snapshot.boundary_review, baselineOutput.boundary_review);
  assert.deepEqual(snapshot.presented_samples, baselineOutput.presented_samples);
});

test("public workspace restores decisions only for an exact completed run", () => {
  const imported = createReviewedSession();
  const { workspace } = createWorkspace();
  const restoration = reviewRestorationResult(imported, currentIdentity());

  assert.equal(restoration.restore, true);
  workspace.loadReviewDecisions(restoration.decisions);
  assert.deepEqual(workspace.reviewArtifact().decisions, imported.review.decisions);
});

test("public workspace keeps decisions detached when detector stats change", () => {
  const imported = createReviewedSession();
  const changedOutput = structuredClone(baselineOutput);
  changedOutput.detection.stats.rows[2].score = 3.2;
  const { workspace } = createWorkspace(changedOutput);
  const restoration = reviewRestorationResult(imported, currentIdentity(changedOutput));

  assert.deepEqual(restoration, {
    restore: false,
    mismatch: "detector output",
    decisions: null,
  });
  assert.deepEqual(workspace.reviewArtifact().decisions, []);
});

test("public workspace keeps decisions detached when non-boundary timing changes", () => {
  const imported = createReviewedSession();
  const changedOutput = structuredClone(baselineOutput);
  changedOutput.presented_samples[2].media_time_seconds = 2.15;
  const { workspace } = createWorkspace(changedOutput);
  const restoration = reviewRestorationResult(imported, currentIdentity(changedOutput));

  assert.deepEqual(restoration, {
    restore: false,
    mismatch: "detector output",
    decisions: null,
  });
  assert.deepEqual(workspace.reviewArtifact().decisions, []);
});

test("review restoration fails closed for changed media or settings", () => {
  const imported = createReviewedSession();
  const changedMedia = currentIdentity();
  changedMedia.media.size += 1;
  assert.deepEqual(reviewRestorationResult(imported, changedMedia), {
    restore: false,
    mismatch: "media fingerprint",
    decisions: null,
  });

  const changedSettings = currentIdentity();
  changedSettings.settings.detector_config.threshold = 28;
  assert.deepEqual(reviewRestorationResult(imported, changedSettings), {
    restore: false,
    mismatch: "detector or sampling settings",
    decisions: null,
  });
});

test("review restoration fails closed for malformed review decisions", () => {
  const imported = createReviewedSession();
  delete imported.review.decisions;

  assert.deepEqual(reviewRestorationResult(imported, currentIdentity()), {
    restore: false,
    mismatch: "review session",
    decisions: null,
  });
});

test("detector snapshots fail closed when either snapshot is missing", () => {
  const snapshot = currentIdentity().detector_snapshot;
  assert.equal(detectorSnapshotsMatch(null, snapshot), false);
  assert.equal(detectorSnapshotsMatch(snapshot, undefined), false);
});
