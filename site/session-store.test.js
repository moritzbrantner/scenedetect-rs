import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { detectorSnapshotsMatch, reviewRestorationResult } from "./session-store.js";

const baseline = {
  scenes: [
    { start: 0, end: 12 },
    { start: 12, end: 24 },
  ],
  boundaries: [{ sample: 12, media_time_seconds: 1.375 }],
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
const decisions = [
  { sample: 18, media_time_seconds: 2.05, action: "accept" },
];

function importedSession() {
  return {
    schema_version: 1,
    media: structuredClone(media),
    settings: structuredClone(settings),
    detector_snapshot: structuredClone(baseline),
    review: { decisions: structuredClone(decisions) },
  };
}

function currentIdentity() {
  return {
    media: structuredClone(media),
    settings: structuredClone(settings),
    detector_snapshot: structuredClone(baseline),
  };
}

test("detector snapshots match independent of object key order", () => {
  const equivalent = JSON.parse(JSON.stringify(baseline));
  equivalent.boundaries = [{ media_time_seconds: 1.375, sample: 12 }];
  equivalent.scenes = [
    { end: 12, start: 0 },
    { end: 24, start: 12 },
  ];

  assert.equal(detectorSnapshotsMatch(baseline, equivalent), true);
});

test("detector snapshots reject a changed scene boundary", () => {
  const changed = structuredClone(baseline);
  changed.scenes[0].end = 13;
  changed.scenes[1].start = 13;
  changed.boundaries[0].sample = 13;

  assert.equal(detectorSnapshotsMatch(baseline, changed), false);
});

test("detector snapshots reject a changed boundary media time", () => {
  const changed = structuredClone(baseline);
  changed.boundaries[0].media_time_seconds = 1.5;

  assert.equal(detectorSnapshotsMatch(baseline, changed), false);
});

test("detector snapshots reject a changed review candidate", () => {
  const changed = structuredClone(baseline);
  changed.boundary_review.candidates[1].status = "accepted";

  assert.equal(detectorSnapshotsMatch(baseline, changed), false);
});

test("detector snapshots reject changed detection stats", () => {
  const changed = structuredClone(baseline);
  changed.detection.stats.rows[2].score = 3.2;

  assert.equal(detectorSnapshotsMatch(baseline, changed), false);
});

test("detector snapshots reject changed non-boundary presentation timing", () => {
  const changed = structuredClone(baseline);
  changed.presented_samples[2].media_time_seconds = 2.15;

  assert.equal(detectorSnapshotsMatch(baseline, changed), false);
});

test("detector snapshots reject missing complete-output identity", () => {
  const incomplete = structuredClone(baseline);
  delete incomplete.detection;

  assert.equal(detectorSnapshotsMatch(baseline, incomplete), false);
});

test("review restoration releases decisions only for an exact run identity", () => {
  assert.deepEqual(reviewRestorationResult(importedSession(), currentIdentity()), {
    restore: true,
    mismatch: null,
    decisions,
  });
});

test("review restoration fails closed for changed detector output", () => {
  const current = currentIdentity();
  current.detector_snapshot.detection.stats.rows[2].score = 3.2;

  assert.deepEqual(reviewRestorationResult(importedSession(), current), {
    restore: false,
    mismatch: "detector output",
    decisions: null,
  });
});

test("review restoration fails closed for changed presentation timing", () => {
  const current = currentIdentity();
  current.detector_snapshot.presented_samples[2].media_time_seconds = 2.15;

  assert.deepEqual(reviewRestorationResult(importedSession(), current), {
    restore: false,
    mismatch: "detector output",
    decisions: null,
  });
});

test("review restoration fails closed for changed media or settings", () => {
  const changedMedia = currentIdentity();
  changedMedia.media.size += 1;
  assert.equal(reviewRestorationResult(importedSession(), changedMedia).restore, false);

  const changedSettings = currentIdentity();
  changedSettings.settings.detector_config.threshold = 28;
  assert.equal(reviewRestorationResult(importedSession(), changedSettings).restore, false);
});

test("review restoration fails closed for malformed review decisions", () => {
  const imported = importedSession();
  delete imported.review.decisions;

  assert.deepEqual(reviewRestorationResult(imported, currentIdentity()), {
    restore: false,
    mismatch: "review session",
    decisions: null,
  });
});

test("review workspace snapshots complete detector and presentation output", async () => {
  const source = await readFile(new URL("./review-workspace.js", import.meta.url), "utf8");

  assert.match(source, /detection:\s*output\?\.detection/u);
  assert.match(source, /boundary_review:\s*output\?\.boundary_review/u);
  assert.match(source, /presented_samples:\s*presentedSamples/u);
});

test("public workbench restoration path uses the tested fail-closed gate", async () => {
  const source = await readFile(new URL("./workbench.js", import.meta.url), "utf8");

  assert.match(
    source,
    /const restoration = reviewRestorationResult[\s\S]*if \(restoration\.restore\)[\s\S]*loadReviewDecisions\(restoration\.decisions\)/u,
  );
});

test("detector snapshots fail closed when either snapshot is missing", () => {
  assert.equal(detectorSnapshotsMatch(null, baseline), false);
  assert.equal(detectorSnapshotsMatch(baseline, undefined), false);
});
