# Bounded Frame Retention For Detection

`scenedetect-rs` detection consumes `Frame Source` values incrementally instead
of retaining every decoded `Frame` before Detector execution. The CLI uses the
streaming path by default, and Detection Stats are written through a sink when a
stats file is requested.

The collecting Rust APIs remain available for callers that want an in-memory
`DetectionResult`, but they are compatibility wrappers around the streaming
engine. This keeps existing callers working while making bounded raw-frame
retention the default implementation constraint.

Content and threshold Detectors need only the current Detector state plus, for
content, the previous `Frame`. Adaptive detection keeps a bounded local window
of content values so its `frame-window` behavior and edge-frame `0.0` adaptive
ratios remain compatible with existing parity expectations.
