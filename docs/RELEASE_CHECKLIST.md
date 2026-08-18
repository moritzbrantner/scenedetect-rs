# Release checklist

For the issue #72 `scenedetect-core` 0.1.0 release:

1. Keep Detector source, tests, CLI, FFmpeg, fixtures, and behavior unchanged.
2. Commit the release controls as the immutable source commit, then add only
   `releases/scenedetect-core-0.1.0.toml` in the control commit.
3. Run only clean-state inspection, `git diff --check`, locked Cargo metadata,
   the exact manifest validator, one locked `scenedetect-core` crates.io
   package, and registry/tag absence or exact-state checks.
4. Record unit, parity, workspace, Clippy, documentation, consumer, build, and
   broad package suites as skipped; never describe them as passing evidence.
5. Keep issue #72 open and unapproved while another release owns crates.io.
6. Before publication, bind the exact control SHA and manifest SHA-256 in issue
   #72 and apply `release:approved` only for the active receipt-gated attempt.
7. Publish only `scenedetect-core` 0.1.0. Verify its non-yanked registry checksum
   before creating `scenedetect-core-v0.1.0` at the immutable source commit and
   the matching GitHub Release.
8. Stop on any mismatch or partial failure. Resume idempotently; never
   overwrite, delete, or automatically yank a published version.
9. Treat registry-only consumer migration and source removal from
   `rust-packages` as separately authorized restructuring work.
