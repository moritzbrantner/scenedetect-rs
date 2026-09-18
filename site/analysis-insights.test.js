import assert from "node:assert/strict";
import test from "node:test";

import {
  buildScoreSeries,
  detectorMetricSpec,
  maximumSeriesScore,
  previewCandidateFrames,
  scenePreviewSamples,
} from "./analysis-insights.js";

function output(rows) {
  return {
    detection: {
      stats: { rows },
    },
  };
}

test("content preview uses Rust content_val and skips frame zero", () => {
  const settings = {
    detector: "content",
    detector_config: { threshold: 20 },
  };
  const { series, spec } = buildScoreSeries(
    output([
      { frame: 0, metrics: { content_val: 100 } },
      { frame: 1, metrics: { content_val: 22 } },
      { frame: 2, metrics: { content_val: 10 } },
    ]),
    settings,
  );

  assert.equal(spec.threshold, 20);
  assert.deepEqual(previewCandidateFrames(series, 20), [1]);
});

test("adaptive preview preserves the minimum-content noise floor", () => {
  const settings = {
    detector: "adaptive",
    detector_config: { threshold: 3, min_content_val: 15 },
  };
  const { series } = buildScoreSeries(
    output([
      { frame: 1, metrics: { adaptive_ratio: 8, content_val: 10 } },
      { frame: 2, metrics: { adaptive_ratio: 4, content_val: 20 } },
      { frame: 3, metrics: { adaptive_ratio: 2, content_val: 30 } },
    ]),
    settings,
  );

  assert.deepEqual(previewCandidateFrames(series, 3), [2]);
});

test("histogram preview converts correlation to detector distance", () => {
  const settings = {
    detector: "histogram",
    detector_config: { threshold: 0.05 },
  };
  const { series } = buildScoreSeries(
    output([
      { frame: 0, metrics: { "hist_diff [bins=256]": 0 } },
      { frame: 1, metrics: { "hist_diff [bins=256]": 0.9 } },
      { frame: 2, metrics: { "hist_diff [bins=256]": 0.99 } },
    ]),
    settings,
  );

  assert.equal(series[0].score, 0);
  assert.ok(Math.abs(series[1].score - 0.1) < 1e-12);
  assert.deepEqual(previewCandidateFrames(series, 0.05), [1]);
});

test("hash preview uses hash distance", () => {
  const settings = {
    detector: "hash",
    detector_config: { threshold: 0.4 },
  };
  const { series } = buildScoreSeries(
    output([
      { frame: 1, metrics: { "hash_dist [size=16 lowpass=2]": 0.45 } },
      { frame: 2, metrics: { "hash_dist [size=16 lowpass=2]": 0.2 } },
    ]),
    settings,
  );

  assert.deepEqual(previewCandidateFrames(series, 0.4), [1]);
});

test("stateful fade detector explicitly disables threshold-only boundary preview", () => {
  const spec = detectorMetricSpec({
    detector: "threshold",
    detector_config: { threshold: 12 },
  });

  assert.equal(spec.tunable, false);
  assert.equal(spec.label, "Average luminance");
});

test("maximum score stays bounded at the 200000-sample browser cap", () => {
  const series = Array.from({ length: 200_000 }, (_, index) => ({
    score: index === 199_999 ? 123.5 : 1,
  }));

  assert.equal(maximumSeriesScore(series), 123.5);
});

test("scene preview samples span the scene without crossing its exclusive end", () => {
  assert.deepEqual(scenePreviewSamples({ start: 10, end: 20 }, 3), [10, 15, 19]);
  assert.deepEqual(scenePreviewSamples({ start: 4, end: 5 }, 3), [4]);
  assert.deepEqual(scenePreviewSamples({ start: 8, end: 8 }, 3), [8]);
});
