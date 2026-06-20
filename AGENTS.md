# Agent Operating Contract

## Mission

Implement the assigned goal until the acceptance criteria are met and required
checks are green, or stop only with a concrete blocker report.

## Required Reading

1. `CONTEXT.md`
2. `CONTRIBUTING.md`
3. The assigned GitHub issue
4. Any ADR touching the area being changed
5. Relevant tests near the target behavior

## Work Loop

1. Restate the goal and acceptance criteria.
2. Inspect the current implementation.
3. Add or update one behavior test through the public interface.
4. Run the focused test and confirm RED when practical.
5. Implement the smallest change needed for GREEN.
6. Run the focused test again.
7. Repeat until all acceptance criteria are covered.
8. Refactor only when the suite is green.
9. Run final verification.

## Test Placement

Use the test placement table in `CONTRIBUTING.md`. Tests verify behavior through
the public interface that callers use, not private implementation details.

## Verification Ladder

During development:

- Core slice: `cargo test -p scenedetect-core <test_name>`
- CLI slice: `cargo test -p scenedetect-cli --test cli <test_name>`
- FFmpeg slice: `cargo test -p scenedetect-ffmpeg <test_name>`
- Parity slice: `tests/parity/run-all.sh`

Before handoff:

- `bun run tdd:check`
- `bun run agent:check`

When branch or patch comparison is useful:

- `bun run agent:eval -- --candidate-ref <branch-or-sha>`
- `bun run agent:eval -- --candidate-patch <patch-file>`

## Definition Of Done

An agent is done only when:

- acceptance criteria are satisfied
- behavior tests exist or were deliberately deemed unnecessary
- `bun run agent:check` passes
- Moonlight eval passes when applicable
- PR description includes verification evidence

## Blocker Report

If blocked, report:

- exact goal and remaining acceptance criteria
- commands run
- failing output summary
- files inspected or changed
- concrete external decision or input needed
- safest next step

## Agent skills

### Issue tracker

Issues and PRDs are tracked in GitHub Issues for `moritzbrantner/scenedetect-rs`. See `docs/agents/issue-tracker.md`.

### Triage labels

The repo uses the default five-label triage vocabulary. See `docs/agents/triage-labels.md`.

### Domain docs

This is a single-context repo with root `CONTEXT.md` and root `docs/adr/`. See `docs/agents/domain.md`.

### Planning workflow

Substantial new work should be planned into GitHub PRD issues instead of implemented directly. See `docs/agents/planning-workflow.md`.
