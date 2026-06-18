#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 4 ]]; then
  echo "usage: run-case.sh <case-id> <video> <detector> <threshold>" >&2
  exit 2
fi

CASE_ID="$1"
VIDEO="$2"
DETECTOR="$3"
THRESHOLD="$4"

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="$ROOT_DIR/tests/parity/output/$CASE_ID"
mkdir -p "$OUT_DIR/reference" "$OUT_DIR/candidate"

UV_BIN="$("$ROOT_DIR/scripts/setup-python-oracle.sh")"
CANDIDATE_BIN="${CANDIDATE_BIN:-$ROOT_DIR/target/debug/scenedetect-rs}"

cargo build -p scenedetect-cli >/dev/null

"$UV_BIN" run --python 3.12 --with scenedetect-headless==0.7 -- \
  scenedetect -i "$ROOT_DIR/$VIDEO" "$DETECTOR" --threshold "$THRESHOLD" --min-scene-len 1 \
  list-scenes --output "$OUT_DIR/reference" --filename scenes.csv

"$CANDIDATE_BIN" -i "$ROOT_DIR/$VIDEO" -m 1 "$DETECTOR" --threshold "$THRESHOLD" \
  list-scenes --output "$OUT_DIR/candidate" --filename scenes.csv --quiet

"$ROOT_DIR/tests/parity/normalize-scenes.py" "$OUT_DIR/reference/scenes.csv" >"$OUT_DIR/reference.json"
"$ROOT_DIR/tests/parity/normalize-scenes.py" "$OUT_DIR/candidate/scenes.csv" >"$OUT_DIR/candidate.json"

python3 - "$OUT_DIR/reference.json" "$OUT_DIR/candidate.json" <<'PY'
import json
import sys

reference = json.load(open(sys.argv[1]))
candidate = json.load(open(sys.argv[2]))
tolerance = 1

if len(reference) != len(candidate):
    raise SystemExit(f"scene count differs: reference={len(reference)} candidate={len(candidate)}")

for index, (ref, cand) in enumerate(zip(reference, candidate), start=1):
    for field in ("start", "end"):
        delta = abs(ref[field] - cand[field])
        if delta > tolerance:
            raise SystemExit(
                f"scene {index} {field} differs by {delta} frames: "
                f"reference={ref[field]} candidate={cand[field]}"
            )

print(f"parity ok: {len(reference)} scenes")
PY
