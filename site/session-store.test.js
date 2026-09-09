import assert from "node:assert/strict";
import test from "node:test";

import { detectorSnapshotsMatch } from "./session-store.js";

const baseline = {
  scenes: [
    { start: 0, end: 12 },
    { start: 12, end: 24 },
  ],
  boundaries: [{ sample: 12, media_time_seconds: 1.375 }],
};

test("detector snapshots match independent of object key order", () => {
  const equivalent = {
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

test("detector snapshots fail closed when either snapshot is missing", () => {
  assert.equal(detectorSnapshotsMatch(null, baseline), false);
  assert.equal(detectorSnapshotsMatch(baseline, undefined), false);
});
