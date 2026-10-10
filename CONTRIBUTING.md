# Contributing

## TDD Workflow

Changes should start with one behavior test through the public interface. Avoid
writing a batch of imagined tests up front; use a vertical red/green slice and
let each passing slice teach the next one.

Use the project vocabulary from `CONTEXT.md` in test names and assertions:
Frame Source, Frame, Timecode, Detector, Scene Boundary, Scene Span, Scene List,
Detection Stats, Reference Oracle, and Candidate.

Keep tests at the interface callers actually use:

| Behavior             | Test location                                  | Interface                         |
| -------------------- | ---------------------------------------------- | --------------------------------- |
| Core scene model     | `crates/scenedetect-core/src/**/tests.rs`      | public core functions and types   |
| Detector behavior    | `crates/scenedetect-core/src/**/tests.rs`      | `detect_scenes`                   |
| CLI behavior         | `crates/scenedetect-cli/tests/cli.rs`          | `scenedetect-rs` binary           |
| Frame source         | `crates/scenedetect-ffmpeg/tests/*.rs`         | `FfmpegFrameSource`               |
| PySceneDetect parity | `tests/parity/**`                              | reference and candidate CLIs      |

Mock only system boundaries. Rust tests should prefer real temp dirs, real
command invocation, generated tiny videos, and test support helpers over
mocking internal modules.

Use this loop:

1. `RED`: add one failing behavior test.
2. `GREEN`: implement the smallest change that passes that test.
3. `REFACTOR`: clean up only after the test suite is green.

Native path first: a fresh clone builds and tests with only Rust and `ffmpeg`
(`scripts/native-check.sh`, also `bun run native:check`). The parity, oracle,
quality and site tools below are optional for local work, but remain part of
the `agent:check` handoff gate.

Focused commands:

```sh
cargo test -p scenedetect-core <test_name>
cargo test -p scenedetect-cli --test cli <test_name>
cargo test -p scenedetect-ffmpeg <test_name>
tests/parity/run-all.sh
bun run tdd:check
```

Optional performance reporting:

```sh
tests/benchmarks/run-hyperfine.sh --generated-only
```

Benchmark reports are useful when changing Frame Source, Detector, or CLI
throughput, but they are report-only and are not part of `agent:check`.

Coding agents should follow `AGENTS.md`.

Final agent handoff requires:

```sh
bun run agent:check
```

The `Agent check` job in `.github/workflows/ci.yml` runs exactly this command
on every pull-request head and on `main`, so hosted evidence and the local
handoff gate are the same contract (format, Clippy, WebAssembly build, Rust
tests, local oracle, PySceneDetect parity, quality validation and site checks).
`Dependency security policy` runs alongside it. `Performance evidence`
(Moonlight session profile) stays report-only for timing.
