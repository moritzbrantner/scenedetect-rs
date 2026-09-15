import assert from "node:assert/strict";
import test from "node:test";

import { createReviewWorkspace } from "./review-workspace.js";

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

class ReviewSelectionEvent {
  constructor(type, init = {}) {
    this.type = type;
    this.detail = init.detail;
  }
}

test("selected boundary exposes the previous presented sample and exact media times", () => {
  const originalCustomEvent = globalThis.CustomEvent;
  globalThis.CustomEvent = ReviewSelectionEvent;
  try {
    const events = [];
    const timelineTrack = fakeElement({
      dispatchEvent(event) {
        events.push(event);
        return true;
      },
    });
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
      timelineTrack,
      timelineZoom: fakeElement({ value: "1" }),
      timelineStatus: fakeElement(),
      reviewStatus: fakeElement(),
      compareStatus: fakeElement(),
      formatTime: String,
    });

    workspace.load({
      fps: 8,
      duration: video.duration,
      output: {
        detection: {
          scene_list: {
            scenes: [
              { start: 0, end: 12 },
              { start: 12, end: 24 },
            ],
          },
          stats: { rows: [] },
        },
        boundary_review: {
          candidates: [
            {
              frame: 12,
              status: "accepted",
              score: 42.5,
              threshold_distance: 15.5,
            },
          ],
        },
        presented_samples: [
          { sample: 0, media_time_seconds: 0 },
          { sample: 7, media_time_seconds: 0.81 },
          { sample: 12, media_time_seconds: 1.375 },
          { sample: 24, media_time_seconds: 3 },
        ],
      },
    });

    workspace.seekBoundary(1);
    assert.deepEqual(events.at(-1).detail, {
      type: "boundary",
      sample: 12,
      label: "Rust detector boundary",
      beforeSample: 7,
      beforeMediaTime: 0.81,
      afterSample: 12,
      afterMediaTime: 1.375,
      candidate: {
        status: "accepted",
        score: 42.5,
        threshold_distance: 15.5,
      },
    });

    workspace.acceptSelectedBoundary();
    assert.deepEqual(workspace.reviewArtifact().decisions, [
      { sample: 12, media_time_seconds: 1.375, action: "accept" },
    ]);
  } finally {
    globalThis.CustomEvent = originalCustomEvent;
  }
});
