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
- Exact timing projection: versioned `*.timeline.json` Scene Timeline with rational Media Time endpoints
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

For `input.mp4`, detection writes `input.scenedetect.json`. Frame-index render commands derive outputs from that artifact without decoding the video again:

```sh
scenedetect-rs render scenes -i input.mp4
scenedetect-rs render stats -i input.mp4 --csv
scenedetect-rs render html -i input.mp4
```

Render an exact Scene Timeline when downstream tooling needs presentation timing rather than average-frame-rate timecodes:

```sh
scenedetect-rs render timeline -i input.mp4
```

This writes `input.timeline.json`. The Scene Timeline preserves the canonical Scene Span frame boundaries from Detection Stats and attaches exact integer-tick/rational-time-base Media Time endpoints. It explicitly re-reads the video through the timing-aware Frame Source, so this render path may decode/probe media again. Rendering it never rewrites Detection Stats.

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

Derived files use the input stem, for example `input.scenes.csv`, `input.stats.csv`, `input.boundaries.csv`, `input.scenes.html`, and `input.timeline.json`.

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

### Native path (fresh clone)

A fresh clone needs only a Rust toolchain plus `ffmpeg`/`ffprobe` on `PATH`. No Python, uv, Bun packages or PySceneDetect are required:

```sh
cargo build --locked -p scenedetect-cli
cargo test --locked --workspace
scripts/native-check.sh   # build, all Rust tests, and native Content detection on a generated hard cut
```

The `Native fresh clone` CI job runs `scripts/native-check.sh` on a clean checkout inside a separately started minimal Rust container (no runner mounts) without Python, pip, uv, Node.js, npm or Bun, so a hidden dependency on any of them fails the job.

### Full handoff gate (optional tooling)

PySceneDetect parity, the local oracle, quality corpus checks and site checks need Bun, Python and the uv-managed PySceneDetect oracle (`scripts/setup-python-oracle.sh`). They stay in the handoff gate that `AGENTS.md` requires:

```sh
bun install
bun run tdd:check   # = bun run agent:check
bun run tdd:parity  # PySceneDetect parity on its own
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

## Vocabulary

Scene detection identifies boundaries in decoded video and emits scene spans
with enough stats to explain and reproduce those boundaries.

### Terms

**Frame Source**:
A producer of decoded video frames and timing metadata.
_Avoid_: Video backend

**Frame**:
One decoded RGB image at a known frame index.

**Media Time**:
An exact presentation position or duration represented as integer ticks plus a
rational time base. Use this for backend/container timing, including VFR data.
_Avoid_: Timecode, floating-point timestamp

**Timecode**:
A user-facing representation of a frame position or duration.
_Avoid_: Timestamp

**Detector**:
A strategy that scores frames and proposes scene boundaries.
_Avoid_: Algorithm

**Histogram Detector**:
A Detector that compares adjacent Frame luma histograms to propose Scene
Boundaries.
_Avoid_: Hist algorithm

**Perceptual Hash Detector**:
A Detector that compares adjacent Frame perceptual hashes to propose Scene
Boundaries.
_Avoid_: Hash algorithm

**Histogram Correlation Score**:
A Detection Stats value describing adjacent luma histogram similarity.
_Avoid_: Histogram distance

**Hash Distance**:
A Detection Stats value describing normalized adjacent perceptual hash
difference.
_Avoid_: Raw hamming distance

**Scene Boundary**:
A frame position where one scene ends and another begins.
_Avoid_: Cut

**Boundary Candidate**:
A frame position considered during review as a possible Scene Boundary,
including accepted, suppressed, and near-threshold positions.
_Avoid_: Candidate, split

**Boundary Score**:
A Detector-specific numeric value used to rank Boundary Candidates for review.
_Avoid_: Score

**Scene Span**:
A contiguous range of frames belonging to one scene.
_Avoid_: Segment

**Scene List**:
Ordered Scene Spans derived from Detection Stats.

**Scene Timeline**:
A versioned exact-media-time projection of a Scene List. It preserves canonical
frame boundaries while attaching optional rational Media Time endpoints from a
timing-aware Frame Source. It is derived output, not Detector truth.
_Avoid_: Timed Detection Stats

**Scene List Artifact**:
Legacy reusable representation of a Scene List plus detection provenance.
_Avoid_: Cache file

**Reusable Output**:
A rendered output file whose manifest proves it was produced from a matching
Scene List Artifact.
_Avoid_: Existing file

**Detection Stats**:
Canonical reusable record of Detector provenance, per-frame scores, decisions,
and metrics used to derive Scene Lists and review outputs.
_Avoid_: Metrics file

**Reference Oracle**:
PySceneDetect output used as the known-good comparison.
_Avoid_: Baseline script

**Candidate**:
The `scenedetect-rs` output being compared to the Reference Oracle.

**Parity Case**:
One Reference Oracle and Candidate comparison over a fixture, Detector options,
and tolerance.

**Capability Matrix**:
Supported and unsupported PySceneDetect behavior tracked by Detector and output
behavior.

**Expected Gap**:
Known PySceneDetect behavior that is documented but not yet required to pass.

**Benchmark Case**:
One report-only timing comparison over equivalent Reference Oracle and Candidate
commands.

**Benchmark Corpus**:
Generated and optional local real-video fixtures used for timing.

**Published Benchmark Snapshot**:
A committed point-in-time Benchmark Report derived from local Benchmark Cases
and published on the project site.
