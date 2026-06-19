# Fixtures

Generated fixtures live in `tests/fixtures/generated/` and are intentionally
ignored by git. Create them with:

```sh
scripts/generate-fixtures.sh
```

The fixtures are tiny deterministic videos produced by `ffmpeg` filters.
They cover hard Scene Boundaries, adaptive fast-motion-like changes, threshold
fade-return behavior, and close cuts for min-scene-len follow-up work.

Generated fixtures:

- `content-hard-cut.mkv`: black-to-white hard Scene Boundary.
- `threshold-fade-return.mkv`: threshold/fade-oriented luminance transition.
- `adaptive-fast-motion.mkv`: rapid contrast changes for adaptive detector work.
- `min-scene-len-close-cuts.mkv`: close Scene Boundaries for min-scene-len work.
