#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

"$ROOT_DIR/scripts/generate-fixtures.sh" >/dev/null
exec python3 "$ROOT_DIR/tests/quality/run.py" \
  --manifest "$ROOT_DIR/tests/quality/corpus.generated.toml" \
  "$@"
