# Source-first `scenedetect-core` consumer contract

Status: accepted

`scenedetect-core` is the canonical Rust scene-detection owner for source-first
cross-repository development. Consumers should compose the existing typed seam
instead of depending on CLI orchestration:

1. implement `FrameSource` at the consumer/backend boundary;
2. run `detect_content_stats` for the current native content workflow;
3. retain `ContentDetectionStats` as the reusable typed result; and
4. derive Scene Lists and Boundary Candidate review from those Detection Stats.

This contract deliberately keeps decoding and application-specific media
metadata outside the core crate. Transport-neutral timing attached to an
individual decoded Frame is part of the Frame Source seam: timing-aware sources
may provide presentation time and duration as integer ticks plus a rational time
base, while legacy sources may continue to provide plain Frames with timing left
unknown. Core detection consumes the richer seam but keeps Scene Boundary
semantics frame-index based in this compatibility slice.

This also lets a consumer stream decoded frames through the detector once rather
than retaining a video and repeatedly rerunning prefix detection.

## Compatibility

`FrameSource::next_frame` remains the required legacy method. The richer
timing-aware method has a default implementation that wraps a plain Frame, so
existing custom Frame Sources remain source-compatible. New backend adapters may
override the richer method without forcing presentation timing into the `Frame`
struct itself.

`ContentDetectionStats` and its nested public serde types are part of the Rust
consumer contract. A committed prior-shape JSON fixture must continue to
deserialize and derive the same Scene List and accepted Boundary Candidates for
compatible 0.1.x changes. Intentional incompatible serialized changes require a
separate compatibility decision rather than silent field drift.

The native CLI's visible `<stem>.scenedetect.json` file is a different boundary.
That document owns file/source provenance such as path, byte length, modified
time, dimensions, detector identity, and its explicit `schema_version`. Its
versioning and file lifecycle stay in `scenedetect-cli`; Rust consumers do not
need to reconstruct CLI-owned source metadata merely to call `scenedetect-core`.
Frame presentation timing is not added to that persisted document by the initial
Frame Source timing slice; persisting a richer Scene Timeline requires a separate
versioned compatibility decision.

## Development and distribution

Ordinary multi-repository development may consume an exact reviewed
`scenedetect-rs` source revision through local-only source mode. Publication is a
later distribution concern and is not required before testing a consumer change.
Committed consumer manifests retain their registry coordinate; local sibling
paths and moving Git dependencies are not part of the distribution contract.

The first design client is `visual-analysis`. If a whole-source adapter can
implement `FrameSource` and produce Detection Stats, Scene Lists, and Boundary
review in one pass, no additional core orchestration API is justified. A future
incremental push API should be added only for a demonstrated frame-by-frame
consumer that cannot use the whole-source seam without semantic or performance
loss.
