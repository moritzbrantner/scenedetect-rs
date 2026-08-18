# Project invariants

## INV-001 — The selected package metadata is exact

- Requirement: `scenedetect-core` is version 0.1.0 and every dependency is an external crates.io dependency.
- Forbidden behavior: a workspace/path/Git dependency, an additional release package, or a detector/API behavior change.
- Authority/source: issue:#72
- Affected surfaces: Cargo.toml, Cargo.lock, crates/scenedetect-core/Cargo.toml
- Compatibility promise: Release preparation does not change Detector behavior or the public scene contract.
- Required evidence: static
- Sensitivity: optional
- Risk dimensions: security=covered:INV-002; recovery=covered:INV-002; persistence=not-applicable:no-storage-change; concurrency=not-applicable:no-concurrency-change; migration=covered:INV-002; partial-failure=covered:INV-002; operational=covered:INV-003

## INV-002 — The release contract authorizes only scenedetect-core

- Requirement: The checked manifest binds destination issue #72, its exact source commit, and only `scenedetect-core` 0.1.0.
- Forbidden behavior: extra packages, another repository or issue, source/control drift outside the manifest, publication without an exact approved issue, or a tag that does not resolve to the immutable source commit.
- Authority/source: issue:#71
- Affected surfaces: .agent-loop.toml, releases/scenedetect-core-0.1.0.toml, scripts/check_release_plan.py, scripts/publish_release.py
- Compatibility promise: CLI, FFmpeg, Detector, fixture, and test surfaces remain outside this release.
- Required evidence: contract
- Sensitivity: optional
- Risk dimensions: security=covered:INV-002; recovery=covered:INV-002; persistence=not-applicable:no-storage-change; concurrency=not-applicable:publication-is-serialized; migration=covered:INV-002; partial-failure=covered:INV-002; operational=covered:INV-003

## INV-003 — Only scenedetect-core enters the archive gate

- Requirement: The structural archive gate runs `cargo package --locked --registry crates-io` for only `scenedetect-core`.
- Forbidden behavior: packaging the workspace, running or claiming behavioral evidence, selecting another registry, or passing a local patch to publication.
- Authority/source: issue:#72
- Affected surfaces: .agent-loop.toml, Cargo.toml, Cargo.lock, crates/scenedetect-core/Cargo.toml
- Compatibility promise: Archive preparation is side-effect free for crates.io and changes only ignored build output.
- Required evidence: integration
- Sensitivity: optional
- Risk dimensions: security=covered:INV-002; recovery=covered:INV-002; persistence=not-applicable:no-registry-write; concurrency=not-applicable:no-publication-effect; migration=covered:INV-002; partial-failure=covered:INV-003; operational=covered:INV-003
