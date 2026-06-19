# PySceneDetect v0.7 Capability Matrix

Required rows run through `tests/parity/run-all.sh`. Expected Gaps are recorded
in `tests/parity/cases.toml` when present but do not fail required parity until
the Candidate implements the matching Detector and CLI surface.

| PySceneDetect command | Candidate status | Parity status | Notes |
| --- | --- | --- | --- |
| `detect-content` | Supported | Required | Covers hard Scene Boundary detection, threshold sensitivity, luma-only mode, and min-scene-len suppression. |
| `detect-adaptive` | Supported | Required | Covers adaptive detection on deterministic generated motion-like cuts. |
| `detect-threshold` | Supported | Required | Covers fade-return Scene List parity. |
| `detect-hist` | Supported | Required | Covers Histogram Detector parity on a hard luma Scene Boundary. |
| `detect-hash` | Supported | Required | Covers Perceptual Hash Detector parity on a structural pattern Scene Boundary. |
