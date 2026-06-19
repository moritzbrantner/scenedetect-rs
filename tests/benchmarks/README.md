# CLI Benchmarks

Benchmarks are report-only. They compare end-to-end Reference Oracle and
Candidate CLI commands, including frame decode, Detector execution, and
scene-list CSV output. They are not part of `agent:check`.

Prerequisites:

- `ffmpeg`
- `uv` through `scripts/setup-python-oracle.sh`
- `hyperfine` on `PATH`

Run generated-only Benchmark Cases:

```sh
tests/benchmarks/run-hyperfine.sh --generated-only
```

Include optional real-video Benchmark Cases:

```sh
tests/benchmarks/run-hyperfine.sh --include-real
```

The optional real-video corpus defaults to:

```sh
../native-whisperx/reference/Shrek Retold - Full Movie [pM70TROZQsI].webm
```

Override it with:

```sh
BENCH_REAL_SOURCE=/path/to/video.webm tests/benchmarks/run-hyperfine.sh --include-real
```

Generated Benchmark Corpus clips are written to `tests/benchmarks/generated/`.
Reports are written to:

- `tests/benchmarks/results/cli.json`
- `tests/benchmarks/results/cli.md`

Both directories are ignored by git.

## Published Benchmark Snapshot

The project site publishes a curated `site/data/benchmarks.json` snapshot. That
file is committed, but it is derived from ignored local `hyperfine` output.

Refresh it after a local benchmark run:

```sh
tests/benchmarks/run-hyperfine.sh --include-real
python3 scripts/update-site-benchmarks.py
python3 scripts/check-site.py
```

Benchmark timing remains report-only. It does not run in CI, the GitHub Pages
workflow, `tdd:check`, or `agent:check`.
