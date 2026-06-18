#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: run-case.sh <case-id>" >&2
  exit 2
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

"$ROOT_DIR/scripts/generate-fixtures.sh" >/dev/null
"$ROOT_DIR/tests/parity/run.py" --case "$1"
