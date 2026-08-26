# CLI Benchmarks

Benchmarks are report-only. They compare end-to-end Reference Oracle and
Candidate CLI commands, including frame decode, Detector execution, and
Scene List CSV output. Timing never blocks CI, `tdd:check`, or `agent:check`.

The generated Benchmark Corpus covers the five supported Detector families:
`detect-content`, `detect-adaptive`, `detect-threshold`, `detect-hist`, and
`detect-hash`. Optional real-video cases currently exercise content detection.

## Prerequisites

- `ffmpeg` on `PATH` for generated benchmark media.
- `uv` for the pinned Python 3.12 / `scenedetect-headless==0.7` Reference Oracle.
  `scripts/setup-python-oracle.sh` uses an installed `uv` or bootstraps the pinned
  local copy with `curl`; missing `uv` is reported explicitly before bootstrap.
- `hyperfine` on `PATH` for timing.
- Rust/Cargo for the release Candidate build.

`run-hyperfine.sh` fails early with a named `ffmpeg` or `hyperfine` prerequisite.
Oracle setup similarly names missing `uv` and explains the pinned bootstrap path.

Validate benchmark configuration without running timing:

```sh
bun run benchmark:validate
```

Run generated-only Benchmark Cases:

```sh
bun run benchmark:generated
```

Include optional real-video Benchmark Cases:

```sh
bun run benchmark:real
```

The equivalent direct commands remain:

```sh
tests/benchmarks/run-hyperfine.sh --generated-only
tests/benchmarks/run-hyperfine.sh --include-real
```

The optional real-video corpus defaults to:

```sh
../native-whisperx/reference/Shrek Retold - Full Movie [pM70TROZQsI].webm
```

Override it with:

```sh
BENCH_REAL_SOURCE=/path/to/video.webm bun run benchmark:real
```

Generated Benchmark Corpus clips are written to `tests/benchmarks/generated/`.
Reports are written to:

- `tests/benchmarks/results/cli.json`
- `tests/benchmarks/results/cli.md`

Both directories are ignored by git. Generated media and timing results are local
artifacts, not source or release evidence.

## Published Benchmark Snapshot

The project site publishes a curated `site/data/benchmarks.json` snapshot. That
file is committed, but it is derived from ignored local `hyperfine` output.

Refresh it only after a deliberate local benchmark run:

```sh
bun run benchmark:real
python3 scripts/update-site-benchmarks.py
python3 scripts/check-site.py
```

The published snapshot is informational. Benchmark timing remains report-only and
is never added to the correctness gates.
