# Local PySceneDetect Oracle

The local oracle is an optional maintainer workflow for comparing `scenedetect-rs`
with the pinned PySceneDetect Reference Oracle. It is intentionally separate from
`agent:check`: generated media, Python oracle output, Candidate output, local
goldens, and benchmark timing are machine-local artifacts rather than required CI
evidence.

## Prerequisites

- `ffmpeg` on `PATH` for deterministic fixture generation and Candidate decoding.
- `uv` for the pinned Python 3.12 / `scenedetect-headless==0.7` Reference Oracle.
  `scripts/setup-python-oracle.sh` uses an installed `uv` when available and can
  bootstrap the pinned repository-local copy with `curl` when it is missing.
- Rust/Cargo for building the Candidate CLI.

Missing prerequisites fail with a named tool and remediation. The oracle setup
prints an explicit `uv`-missing message before attempting the pinned bootstrap.

## Refresh goldens

Refresh ignored Reference Oracle goldens deliberately when the oracle contract or
deterministic fixtures change:

```sh
bun run oracle:refresh
```

Refresh a single Parity Case with:

```sh
bun run oracle:refresh -- --case content-hard-cut
```

Goldens record the oracle package, Python version, case id, Detector command and
arguments, `min-scene-len`, fixture identity, and fixture content hash. Stale
metadata or fixture content therefore fails closed instead of silently becoming a
new baseline.

## Check Candidate behavior

Check every required Detector case across CSV, JSON, and NDJSON Scene List public
CLI formats:

```sh
bun run oracle:check
```

The comparison normalizes PySceneDetect/Candidate frame bases and uses the
configured per-case tolerance, currently one frame for the required cases.
Failures identify the case, source/format, scene, field, observed values, and
tolerance where applicable.

Check the broader public output surfaces with the representative content case:

```sh
bun run oracle:check-surfaces
```

That command checks Boundary Candidate CSV/JSON, legacy HTML, Scene List artifact
reuse, native Detection Stats, and native scene/stats/boundary/HTML renders. These
outputs are checked against the same local golden rather than introducing another
truth source.

## Verify goldens

Verify stored local goldens against a fresh Reference Oracle run without changing
them:

```sh
bun run oracle:verify
```

Use this before trusting an older local golden set after changing Python, fixtures,
or oracle setup.

## Ignored artifact policy

The following stay untracked and may be deleted/recreated at any time:

- `tests/local-oracle/goldens/`
- `tests/local-oracle/output/`
- generated fixture videos
- Reference Oracle outputs
- Candidate outputs
- benchmark generated videos and timing reports

Only the harnesses, configuration, validation logic, and documentation are
committed. Do not promote local timing or golden files into `agent:check`.
