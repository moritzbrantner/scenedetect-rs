#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

"$ROOT_DIR/scripts/generate-fixtures.sh" >/dev/null

"$ROOT_DIR/tests/parity/run-case.sh" \
  "content-hard-cut" \
  "tests/fixtures/generated/content-hard-cut.mkv" \
  "detect-content" \
  "20"
