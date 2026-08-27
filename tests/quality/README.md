# Real-video quality loop

This directory contains a local, non-CI loop for finding meaningful differences between `scenedetect-rs` and the locked PySceneDetect v0.7 Reference Oracle on developer-owned media.

## Setup

Copy the example manifest and edit only the local copy:

```sh
cp tests/quality/corpus.example.toml tests/quality/corpus.local.toml
```

Point one or more `[[cases]]` entries at real videos and set `enabled = true`. Relative video paths are resolved from the manifest directory. The local manifest, per-case work directories, Detection Stats, and generated reports are ignored by Git.

## Run

A single video and Detector is a valid run:

```sh
python3 tests/quality/run.py --case my-video-content
```

Run every enabled case:

```sh
python3 tests/quality/run.py
```

Filter by Detector or record optional report-only timing:

```sh
python3 tests/quality/run.py --detector adaptive
python3 tests/quality/run.py --timing
```

Validate the manifest without building or running either implementation:

```sh
python3 tests/quality/run.py --validate-only
```

By default the structured report is written to `tests/quality/output/report.json`. It contains aggregate matched boundaries, false positives, false negatives, frame deltas, and a ranked `worst_divergences` list with timecodes, Detector configuration, and a reproduction command.

## Isolation

The Candidate runs through the native Detection Stats workflow. Each source video is symlinked into an ignored per-case work directory, so `*.scenedetect.json` artifacts and rendered Scene Lists do not modify or replace artifacts next to the original media.

Runtime timing is intentionally separate from correctness and quality scoring. It is included only with `--timing` and never becomes a blocking CI signal.

No real media is committed or downloaded by this loop.
