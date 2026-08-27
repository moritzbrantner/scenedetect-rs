# Fixtures

Generated fixtures live in `tests/fixtures/generated/` and are intentionally
ignored by git. Create them with:

```sh
scripts/generate-fixtures.sh
```

The fixtures are tiny deterministic videos produced by `ffmpeg` filters. The
basic cases back normal parity tests; the stress cases give the report-only
quality loop an always-available discovery corpus.

Generated fixtures:

- `content-hard-cut.mkv`: black-to-white hard Scene Boundary.
- `content-threshold-gray-cut.mkv`: black-to-gray content change that appears
  at a lower content threshold and is suppressed at a higher threshold.
- `content-color-only-cut.mkv`: similar-luma red-to-green structural color
  change that stresses Content hue/saturation behavior and Histogram luma
  stability.
- `adaptive-fast-motion.mkv`: rapid contrast changes for Adaptive Detector work.
- `adaptive-single-flash.mkv`: one-frame white flash surrounded by black Frames
  for Adaptive windowing stress.
- `threshold-fade-return.mkv`: step-wise threshold/fade-oriented luminance
  transition.
- `threshold-gradual-fade.mkv`: gradual white-to-black fade, short black hold,
  and fade-in for Threshold midpoint/fade-bias stress.
- `min-scene-len-close-cuts.mkv`: close Content Scene Boundaries for
  min-scene-len work.
- `hash-pattern-cut.mkv`: black-to-testsrc structural Scene Boundary for
  Perceptual Hash detection.
- `hash-pattern-mirror.mkv`: test pattern to mirrored test pattern, a subtler
  structural Perceptual Hash change.

Do not commit the generated media. Once a quality finding becomes a fixed
behavioral contract, keep the generating recipe and normal parity/regression
test rather than checking in the video artifact.
