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

Focused commands:

```sh
cargo test -p scenedetect-core <test_name>
cargo test -p scenedetect-cli --test cli <test_name>
cargo test -p scenedetect-ffmpeg <test_name>
tests/parity/run-all.sh
bun run tdd:check
```

Coding agents should follow `AGENTS.md`. An `agent-ready` issue must include
acceptance criteria and verification commands before an agent starts.

Final agent handoff requires:

```sh
bun run agent:check
```
