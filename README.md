# scenedetect-rs

`scenedetect-rs` is a Rust reimplementation and expansion of PySceneDetect.
The native workflow uses reusable Detection Stats as the canonical result of scene detection, while PySceneDetect-compatible commands remain available as a parity seam.

The repository is set up for test-driven agent work. Agents should read [`AGENTS.md`](AGENTS.md), start from a public-interface behavior test, and finish with:

```sh
bun run agent:check
```

Project site: [`https://moritzbrantner.github.io/scenedetect-rs/`](https://moritzbrantner.github.io/scenedetect-rs/)

## Current Scope

- Rust CLI binary: `scenedetect-rs`
- Native Detectors: Content, Adaptive, Threshold, Histogram, and perceptual Hash
- Primary reusable artifact: sibling `*.scenedetect.json` Detection Stats
- Frame acquisition: `ffmpeg` subprocess adapter
- Oracle parity: PySceneDetect v0.7 through a `uv`-managed Python 3.12 environment
- Config-driven parity: `tests/parity/cases.toml` tracks required Parity Cases and Expected Gaps
- Report-only benchmarks: optional `hyperfine` comparisons for Reference Oracle and Candidate commands

## Quick Start

Run a Detector once to create Detection Stats next to the input video:

```sh
scenedetect-rs detect content -i input.mp4
scenedetect-rs detect adaptive -i input.mp4
scenedetect-rs detect threshold -i input.mp4
scenedetect-rs detect hist -i input.mp4
scenedetect-rs detect hash -i input.mp4
```

For `input.mp4`, detection writes `input.scenedetect.json`. Render commands derive outputs from that artifact without decoding the video again:

```sh
scenedetect-rs render scenes -i input.mp4
scenedetect-rs render stats -i input.mp4 --csv
scenedetect-rs render html -i input.mp4
```

Content Detection Stats additionally support score-ranked Boundary Candidate review:

```sh
scenedetect-rs render boundaries -i input.mp4
```

Inspect an artifact without decoding video:

```sh
scenedetect-rs inspect -i input.mp4
scenedetect-rs inspect -i input.mp4 --json
scenedetect-rs inspect -i input.scenedetect.json --json
```

Inspection reports the Detector and configuration, input provenance, frame rate and frame count, Scene Boundary count, and resolved Detection Stats path. JSON output is intended for agents and tools.

Detection progress is interactive by default and can be controlled explicitly:

```sh
scenedetect-rs detect content -i input.mp4 --progress always
scenedetect-rs detect adaptive -i input.mp4 --progress never
```

Derived files use the input stem, for example `input.scenes.csv`, `input.stats.csv`, `input.boundaries.csv`, and `input.scenes.html`.

## PySceneDetect Compatibility

Legacy PySceneDetect-compatible commands remain available for parity work. Global options must appear before the Detector command, and `list-scenes` options must appear after `list-scenes`:

```sh
scenedetect-rs -i input.mp4 --output out detect-content list-scenes
scenedetect-rs -i input.mp4 --stats stats.csv detect-content list-scenes --no-output-file
scenedetect-rs -i input.mp4 --min-scene-len 100 detect-content --min-scene-len 1 list-scenes
scenedetect-rs -i input.mp4 detect-adaptive --threshold 3 --min-content-val 15 list-scenes
scenedetect-rs -i input.mp4 detect-threshold --threshold 12 list-scenes
scenedetect-rs -i input.mp4 detect-hist --threshold 0.05 --bins 256 list-scenes
scenedetect-rs -i input.mp4 detect-hash --threshold 0.395 --size 16 --lowpass 2 list-scenes
scenedetect-rs -i input.mp4 --output out detect-content list-scenes --format json
scenedetect-rs -i input.mp4 --output out detect-content list-scenes --format ndjson
scenedetect-rs -i input.mp4 --output out detect-content export-html
```

CSV remains the default `list-scenes` format and the format used for PySceneDetect parity. JSON writes a complete Scene List document. NDJSON writes one Scene Span event per line for downstream pipelines and agents. `export-html` writes a self-contained Scene List report.

Legacy file-writing Scene List commands create a hidden validated Scene List Artifact under the effective output directory and reuse matching rendered outputs by default. `--force` bypasses reuse and recomputes. `--scene-list-artifact` exposes the canonical legacy Scene List Artifact at a visible path when needed for parity work.

Flexible PySceneDetect command ordering, such as placing global options after `detect-content`, is out of initial scope and fails with clap's command-order error.

## Development

```sh
bun install
bun run tdd:check
```

Focused commands:

```sh
cargo test -p scenedetect-core
cargo test -p scenedetect-cli --test cli
cargo test -p scenedetect-cli --test native_inspect
tests/parity/run-all.sh
```

Optional CLI benchmark report:

```sh
tests/benchmarks/run-hyperfine.sh --generated-only
```

To update the Published Benchmark Snapshot used by the project site, run the benchmark suite locally and convert the ignored `hyperfine` output into committed site data:

```sh
tests/benchmarks/run-hyperfine.sh --include-real
python3 scripts/update-site-benchmarks.py
python3 scripts/check-site.py
```

Benchmark execution is local and report-only. It does not run in CI, the GitHub Pages workflow, `tdd:check`, or `agent:check`.
