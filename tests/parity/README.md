# PySceneDetect Parity

The parity suite runs PySceneDetect as the Reference Oracle and `scenedetect-rs`
as the Candidate on generated tiny videos. It normalizes scene-list CSV files
into frame ranges and compares each required Parity Case with the
case-specific tolerance in `cases.toml`.

`cases.toml` is the source of truth for required cases and Expected Gaps. Rows
with `status = "required"` run in CI through `run-all.sh`. Rows with
`status = "expected-gap"` are reported and skipped until the Candidate exposes
the matching Detector and CLI command.

Each Parity Case defines its video path, Detector, `threshold`,
`min_scene_len`, `tolerance_frames`, and any extra detector-specific `args`.
The runner passes `threshold` explicitly to both Reference Oracle and Candidate
commands.

Scene-list CSV normalization requires a source because PySceneDetect reports
one-based frame columns while `scenedetect-rs` reports zero-based frame columns.

Run:

```sh
tests/parity/run-all.sh
```

Run one configured case:

```sh
tests/parity/run-case.sh content-hard-cut
```

Validate the configuration without running PySceneDetect:

```sh
tests/parity/run.py --validate-only
```

See `capability-matrix.md` for the PySceneDetect v0.7 Detector coverage
tracked by this suite.
