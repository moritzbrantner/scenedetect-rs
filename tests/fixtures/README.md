# Fixtures

Generated fixtures live in `tests/fixtures/generated/` and are intentionally
ignored by git. Create them with:

```sh
scripts/generate-fixtures.sh
```

The fixtures are tiny deterministic videos produced by `ffmpeg` filters.
They cover hard Scene Boundaries, adaptive fast-motion-like changes, threshold
fade-return behavior, content threshold sensitivity, and close cuts for
min-scene-len work. `hash-pattern-cut.mkv` uses a structural pattern change
because uniform luma changes do not reliably trigger perceptual hash detection.

Generated fixtures:

- `content-hard-cut.mkv`: black-to-white hard Scene Boundary.
- `content-threshold-gray-cut.mkv`: black-to-gray content change that appears
  at a lower content threshold and is suppressed at a higher threshold.
- `threshold-fade-return.mkv`: threshold/fade-oriented luminance transition.
- `adaptive-fast-motion.mkv`: rapid contrast changes for adaptive detector work.
- `min-scene-len-close-cuts.mkv`: close content Scene Boundaries for
  min-scene-len work.
- `hash-pattern-cut.mkv`: black-to-testsrc structural Scene Boundary for
  perceptual hash detection.
