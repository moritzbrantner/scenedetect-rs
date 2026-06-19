# scenedetect-rs

`scenedetect-rs` is a Rust reimplementation and expansion of PySceneDetect.
The first milestone focuses on CLI-compatible scene detection for
`detect-content`, `detect-adaptive`, `detect-threshold`, scene-list CSV output,
and detector stats CSV output.

The repository is set up for test-driven agent work. Agents should read
[`AGENTS.md`](AGENTS.md), start from a public-interface behavior test, and finish
with:

```sh
bun run agent:check
```

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
```

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
