# Canonical scene capability boundary

Status: accepted

`scenedetect-rs` is the sole canonical owner of scene-boundary detection
algorithms, Detection Stats, Scene List and Boundary Candidate derivation, and
exact scene timeline projection. It stays a focused scene capability.

The machine-readable contract is
`docs/ownership/scenedetect-rs-boundary.json`; `scripts/check-scene-boundary.py`
enforces it as part of `bun run tdd:check` (and therefore the hosted
`Agent check`):

- No crate depends on generic visual-analysis packages
  (`moenarch-image-analysis-*`, `moenarch-video-analysis-*`,
  `moenarch-vision-*`) or on downstream corpus/product repositories. Scene
  algorithms are never copied into those repositories either; they consume
  `scenedetect-core`.
- Generic exact rational media time belongs to `moenarch-media-core` in
  `moenarch-foundation`; scene timeline semantics stay here (#129 delegates the
  arithmetic).
- Committed manifests do not reach sibling checkouts through path
  dependencies or local Bun specifiers, so a fresh clone resolves on its own.
- The supported cross-domain integration path is the `scenedetect-core`
  consumer seam of ADR 0010 (`FrameSource`, `detect_content_stats`,
  `ContentDetectionStats`), used by `visual-analysis`.

The only transitional exception is the unpublished browser adapter
`scenedetect-wasm`, which consumes the published
`moenarch-image-analysis-processing` capability for the Pages lab's similarity
view instead of reimplementing it. The check requires it to stay
`publish = false` and limits it to that package.
