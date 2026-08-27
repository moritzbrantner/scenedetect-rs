# Scene-quality loop

This directory contains a local, report-only loop for finding meaningful differences between `scenedetect-rs` and the locked PySceneDetect v0.7 Reference Oracle.

There are two corpus modes:

- `corpus.generated.toml`: committed deterministic stress cases. Use this when an agent needs an always-available next quality problem.
- `corpus.local.toml`: ignored developer-owned real videos. Use this to extend the same loop with private material without committing media or machine-specific paths.

## Reference semantics

Quality comparison uses the locked PySceneDetect v0.7 **detector API**, not the PySceneDetect command-line parser. `reference_oracle.py` maps the five canonical Detector configurations explicitly and writes zero-based JSON Scene spans for the quality runner.

This boundary is intentional. `tests/parity/` remains responsible for PySceneDetect-compatible CLI behavior, including CLI-specific units and quirks. `tests/quality/` compares the detector algorithms using semantic configuration values. For example, Threshold `fade_bias = 0.25` means the API value `0.25` on both Reference and Candidate rather than routing the Reference through the v0.7 CLI percentage seam.

## Generated discovery corpus

Run the full deterministic discovery corpus:

```sh
bun run quality:generated
```

The command regenerates the tiny ignored FFmpeg fixtures, builds the Candidate, runs Reference Oracle and Candidate, and writes the structured report under `tests/quality/output/`.

A single case or Detector remains a valid small iteration:

```sh
bun run quality:generated -- --case generated-content-color-only
bun run quality:generated -- --detector adaptive
```

Generated quality is deliberately report-only. `agent:check` validates the committed manifest and runner tests but does not require every generated discovery result to be identical or make subjective quality a blocking gate.

## Progressive quality iteration

Use the quality report as a queue, not as a request for a broad rewrite:

1. Run the generated corpus, or one local real-video case.
2. Inspect `worst_divergences` in `tests/quality/output/report.json`.
3. Select one highest-ranked actionable false positive, false negative, or frame delta.
4. Reproduce that one case with the report's `reproduction_command` and inspect its Detection Stats.
5. When the divergence can be represented deterministically, add one public parity/core/CLI regression through the caller-facing interface.
6. Confirm RED when practical, make the smallest Detector or Frame Source change needed for GREEN, and rerun the focused case.
7. Run the relevant parity slice and `bun run agent:check` before merge.
8. Remove the fixed finding from special handling by relying on its normal regression coverage, then select the next divergence.

Do not add new stress families merely to grow the corpus. Add one only when the current corpus no longer exposes useful behavior or a real-video finding needs a deterministic reproduction.

## Local real-video setup

Copy the example manifest and edit only the local copy:

```sh
cp tests/quality/corpus.example.toml tests/quality/corpus.local.toml
```

Point one or more `[[cases]]` entries at real videos and set `enabled = true`. Relative video paths are resolved from the manifest directory. The local manifest, per-case work directories, Detection Stats, and generated reports are ignored by Git.

Run one local video and Detector:

```sh
python3 tests/quality/run.py --case my-video-content
```

Run every enabled local case:

```sh
python3 tests/quality/run.py
```

Filter by Detector or record optional report-only timing:

```sh
python3 tests/quality/run.py --detector adaptive
python3 tests/quality/run.py --timing
```

Validate a manifest without building or running either implementation:

```sh
python3 tests/quality/run.py --validate-only
python3 tests/quality/run.py --manifest tests/quality/corpus.generated.toml --validate-only
```

By default the structured report is written to `tests/quality/output/report.json`. It contains aggregate matched Scene Boundaries, false positives, false negatives, frame deltas, and a ranked `worst_divergences` list with Timecodes, Detector configuration, and a reproduction command.

## Isolation

The Candidate runs through the native Detection Stats workflow. Each source video is symlinked into an ignored per-case work directory, so `*.scenedetect.json` artifacts and rendered Scene Lists do not modify or replace artifacts next to the original media.

Runtime timing is intentionally separate from correctness and quality scoring. It is included only with `--timing` and never becomes a blocking CI signal.

No real media is committed or downloaded by this loop. Generated fixtures are tiny deterministic videos produced locally by FFmpeg and remain ignored by Git.
