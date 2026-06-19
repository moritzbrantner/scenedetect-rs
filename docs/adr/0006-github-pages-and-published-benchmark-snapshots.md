# GitHub Pages And Published Benchmark Snapshots

The project uses a dependency-free static site in `site/` and deploys it with a
GitHub Pages Actions workflow. The site is a project showcase, not an
application build, so plain HTML, CSS, JavaScript, and committed assets keep the
deployment small and easy to inspect.

Benchmark timing is published as a committed Published Benchmark Snapshot at
`site/data/benchmarks.json`. The snapshot is derived from local `hyperfine`
output and can include generated and optional real-video Benchmark Cases, but
the Pages workflow does not run benchmarks. Timing is machine-sensitive, and
real-video media is local and uncommitted, so CI and Pages deployment only
validate the static site and snapshot shape.

`agent:check` remains a correctness gate. Benchmarks stay report-only evidence
that maintainers refresh deliberately when they want to publish new performance
numbers.
