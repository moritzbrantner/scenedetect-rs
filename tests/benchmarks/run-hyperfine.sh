#!/usr/bin/env bash
set -euo pipefail

MODE="${1:---generated-only}"

if [[ "$MODE" != "--generated-only" && "$MODE" != "--include-real" ]]; then
  echo "usage: run-hyperfine.sh [--generated-only|--include-real]" >&2
  exit 2
fi

if ! command -v hyperfine >/dev/null 2>&1; then
  echo "hyperfine is required for benchmarks." >&2
  echo "Install it with your system package manager or cargo install hyperfine --locked." >&2
  exit 127
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

"$ROOT_DIR/scripts/generate-benchmark-fixtures.sh" "$MODE" >/dev/null
"$ROOT_DIR/scripts/setup-python-oracle.sh" >/dev/null
cargo build -p scenedetect-cli --release >/dev/null

if [[ "$MODE" == "--include-real" ]]; then
  "$ROOT_DIR/tests/benchmarks/run.py" --include-real
else
  "$ROOT_DIR/tests/benchmarks/run.py"
fi
