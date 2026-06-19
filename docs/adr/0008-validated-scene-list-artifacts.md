# Validated Scene List Artifacts

`scenedetect-rs` reuses completed Scene List work through validated Scene List
Artifacts and render manifests instead of raw file-exists checks. The artifact
records detection provenance, so commands can skip duplicate detection only
when the input video fingerprint, Detector configuration, effective frame rate,
and Detection Options match.

We keep Detection Stats separate from this reuse path because Detection Stats
are per-frame explanatory metrics, not the canonical Scene List that downstream
renderers such as `list-scenes` and `export-html` need.
