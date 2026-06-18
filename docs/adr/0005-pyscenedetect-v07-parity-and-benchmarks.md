# PySceneDetect v0.7 Parity And Benchmarks

`scenedetect-rs` uses `scenedetect-headless==0.7` as the locked Reference
Oracle for required parity checks. Required Parity Cases use deterministic
generated fixtures so coding agents and CI compare the Candidate against a
stable target.

PySceneDetect v0.7 includes `detect-content`, `detect-adaptive`,
`detect-threshold`, `detect-hist`, and `detect-hash`. The current Candidate
supports content, adaptive, and threshold Detectors. Histogram and perceptual
hash behavior are tracked as Expected Gaps in the Capability Matrix until their
Rust Detector and CLI surfaces exist.

Benchmarks are report-only Benchmark Cases. They compare end-to-end CLI
commands, including frame decode, detection, and scene-list CSV output, but
they do not run in `agent:check`. Timing is machine-sensitive, so correctness
checks stay separate from performance reports.

Benchmark commands use `hyperfine` as an explicit prerequisite. The repository
does not install it automatically because benchmark tooling should not make
routine correctness checks slower or more surprising. Generated Benchmark
Corpus fixtures are always local artifacts. Optional real-video clips are
derived from a local Shrek Retold source when present and remain uncommitted.
