import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { detectorSnapshotsMatch } from "./session-store.js";

const baseline = {
  scenes: [
    { start: 0, end: 12 },
    { start: 12, end: 24 },
  ],
  boundaries: [{ sample: 12, media_time_seconds: 1.375 }],
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
    ],
  },
};

test("detector snapshots match independent of object key order", () => {
  const equivalent = {
    boundary_review: {
      candidates: [
        {
          threshold_distance: 15.5,
          score: 42.5,
          status: "accepted",
          frame: 12,
        },
      ],
      score_metric: "content_val",
      detector: "content",
    },
    boundaries: [{ media_time_seconds: 1.375, sample: 12 }],
    scenes: [
      { end: 12, start: 0 },
      { end: 24, start: 12 },
    ],
  };

  assert.equal(detectorSnapshotsMatch(baseline, equivalent), true);
});

test("detector snapshots reject a changed scene boundary", () => {
  const changed = structuredClone(baseline);
  changed.scenes[0].end = 13;
  changed.scenes[1].start = 13;
  changed.boundaries[0].sample = 13;

  assert.equal(detectorSnapshotsMatch(baseline, changed), false);
});

test("detector snapshots reject a changed presented media time", () => {
  const changed = structuredClone(baseline);
  changed.boundaries[0].media_time_seconds = 1.5;

  assert.equal(detectorSnapshotsMatch(baseline, changed), false);
});

test("detector snapshots reject a changed review candidate", () => {
  const changed = structuredClone(baseline);
  changed.boundary_review.candidates[0].status = "near_miss";

  assert.equal(detectorSnapshotsMatch(baseline, changed), false);
});

test("detector snapshots reject missing review-candidate identity", () => {
  const incomplete = structuredClone(baseline);
  delete incomplete.boundary_review;

  assert.equal(detectorSnapshotsMatch(baseline, incomplete), false);
});

test("review workspace snapshots include Rust boundary-review output", async () => {
  const source = await readFile(new URL("./review-workspace.js", import.meta.url), "utf8");

  assert.match(source, /boundary_review:\s*output\.boundary_review/u);
});

test("detector snapshots fail closed when either snapshot is missing", () => {
  assert.equal(detectorSnapshotsMatch(null, baseline), false);
  assert.equal(detectorSnapshotsMatch(baseline, undefined), false);
});
