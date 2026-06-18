# PySceneDetect v0.7 Capability Matrix

Required rows run through `tests/parity/run-all.sh`. Expected Gaps are recorded
in `tests/parity/cases.toml` but do not fail required parity until the Candidate
implements the matching Detector and CLI surface.

| PySceneDetect command | Candidate status | Parity status | Notes |
| --- | --- | --- | --- |
| `detect-content` | Supported | Required | Covers hard Scene Boundary detection and Timecode min-scene-len parsing. |
| `detect-adaptive` | Supported | Required | Covers adaptive detection on deterministic generated motion-like cuts. |
| `detect-threshold` | Supported | Required | Covers fade-return Scene List parity with a two-frame tolerance for current placement drift. |
| `detect-hist` | Expected Gap | Skipped | Candidate has no histogram Detector or `detect-hist` CLI command yet. |
| `detect-hash` | Expected Gap | Skipped | Candidate has no perceptual hash Detector or `detect-hash` CLI command yet. |

Known follow-up gaps:

- The close-cut min-scene-len fixture exposes a Detector semantic gap: the
  Reference Oracle suppresses both close Scene Boundaries, while the current
  Candidate may emit the later boundary.
- Threshold placement currently needs a wider tolerance than content/adaptive
  cases because the Candidate places the Scene Boundary later on the generated
  fade-return fixture.
