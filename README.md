# scenedetect-rs

`scenedetect-rs` is a Rust reimplementation and expansion of PySceneDetect.
The first milestone focuses on CLI-compatible scene detection for
`detect-content`, `detect-adaptive`, `detect-threshold`, `detect-hist`,
`detect-hash`, scene-list CSV output, and detector stats CSV output.

The repository is set up for test-driven agent work. Agents should read
[`AGENTS.md`](AGENTS.md), start from a public-interface behavior test, and finish
with:

```sh
bun run agent:check
```

Project site:
[`https://moritzbrantner.github.io/scenedetect-rs/`](https://moritzbrantner.github.io/scenedetect-rs/)

## Current Scope

- Rust CLI binary: `scenedetect-rs`
- Frame acquisition: `ffmpeg` subprocess adapter
- Oracle parity: PySceneDetect v0.7 through a `uv`-managed Python 3.12
  environment
- Config-driven parity: `tests/parity/cases.toml` tracks required Parity Cases
  and Expected Gaps for PySceneDetect v0.7 Detector coverage
- Report-only benchmarks: optional `hyperfine` comparisons for end-to-end
  Reference Oracle and Candidate CLI commands

## CLI Examples

Global options must appear before the Detector command, and `list-scenes`
options must appear after `list-scenes`:

```sh
scenedetect-rs -i input.mp4 --output out detect-content list-scenes
scenedetect-rs -i input.mp4 --stats stats.csv detect-content list-scenes --no-output-file
scenedetect-rs -i input.mp4 --min-scene-len 100 detect-content --min-scene-len 1 list-scenes
scenedetect-rs -i input.mp4 detect-hist --threshold 0.05 --bins 256 list-scenes
scenedetect-rs -i input.mp4 detect-hash --threshold 0.395 --size 16 --lowpass 2 list-scenes
scenedetect-rs -i input.mp4 --output out detect-content list-scenes --format json
scenedetect-rs -i input.mp4 detect-content list-scenes --format json --no-output-file
scenedetect-rs -i input.mp4 --output out detect-content list-scenes --format ndjson
scenedetect-rs -i input.mp4 detect-content list-scenes --format ndjson --no-output-file
```

CSV remains the default `list-scenes` format and is the format used for
PySceneDetect parity. JSON writes a complete Scene List document to
`scenes.json` by default. NDJSON writes one Scene Span event per line to
`scenes.ndjson` by default, or to stdout with `--no-output-file`, for downstream
pipeline and agent workflows.

Flexible PySceneDetect command ordering, such as placing global options after
`detect-content`, is out of initial scope and fails with clap's command-order
error.

## Development

```sh
bun install
bun run tdd:check
```

Focused commands:

```sh
cargo test -p scenedetect-core
cargo test -p scenedetect-cli --test cli
tests/parity/run-all.sh
```

Optional CLI benchmark report:

```sh
tests/benchmarks/run-hyperfine.sh --generated-only
```

To update the Published Benchmark Snapshot used by the project site, run the
benchmark suite locally and convert the ignored `hyperfine` output into the
committed site data:

```sh
tests/benchmarks/run-hyperfine.sh --include-real
python3 scripts/update-site-benchmarks.py
python3 scripts/check-site.py
```

Benchmark execution is local and report-only. It does not run in CI, the GitHub
Pages workflow, `tdd:check`, or `agent:check`.
