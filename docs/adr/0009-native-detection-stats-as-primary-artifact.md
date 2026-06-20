# Native Detection Stats As Primary Artifact

Status: accepted; supersedes ADR-0008 for the native command workflow.

`scenedetect-rs` native commands use visible Detection Stats as the primary
reusable result because users need progress, explainability, and render outputs
that all come from the same per-frame record. Scene Lists, Boundary Candidate
review, CSV stats, and HTML reports are derived from Detection Stats; the older
Scene List Artifact path remains as legacy compatibility for PySceneDetect-style
commands until that surface is retired.
