#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

printf '[{"start": 0, "end": 10}]\n' >"$TMP_DIR/reference.json"
printf '[{"start": 0}]\n' >"$TMP_DIR/bad-candidate.json"

set +e
OUTPUT="$(
  "$ROOT_DIR/tests/parity/run.py" \
    --compare-json content-hard-cut \
    "$TMP_DIR/reference.json" \
    "$TMP_DIR/bad-candidate.json" 2>&1
)"
STATUS=$?
set -e

if [[ "$STATUS" -eq 0 ]]; then
  echo "expected bad candidate JSON comparison to fail" >&2
  exit 1
fi

if [[ "$OUTPUT" != *"content-hard-cut: candidate scene 1 field end"* ]]; then
  echo "expected error to include case id, source, scene, and field" >&2
  echo "$OUTPUT" >&2
  exit 1
fi

echo "runner ok: bad candidate JSON reports case id and field"

cat >"$TMP_DIR/oracle.csv" <<'CSV'
Scene Number,Start Frame,End Frame
1,1,10
CSV

cat >"$TMP_DIR/candidate.csv" <<'CSV'
Scene Number,Start Frame,End Frame
1,0,10
CSV

"$ROOT_DIR/tests/parity/normalize-scenes.py" \
  --source oracle \
  "$TMP_DIR/oracle.csv" >"$TMP_DIR/oracle-normalized.json"
"$ROOT_DIR/tests/parity/normalize-scenes.py" \
  --source candidate \
  "$TMP_DIR/candidate.csv" >"$TMP_DIR/candidate-normalized.json"

python3 - "$TMP_DIR/oracle-normalized.json" "$TMP_DIR/candidate-normalized.json" <<'PY'
import json
import sys
from pathlib import Path

oracle = json.loads(Path(sys.argv[1]).read_text())
candidate = json.loads(Path(sys.argv[2]).read_text())

if oracle != [{"start": 0, "end": 10}]:
    raise SystemExit(f"oracle normalization mismatch: {oracle!r}")
if candidate != [{"start": 0, "end": 10}]:
    raise SystemExit(f"candidate normalization mismatch: {candidate!r}")
PY

echo "normalizer ok: oracle and candidate frame bases are explicit"
