#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
"$ROOT_DIR/scripts/generate-fixtures.sh" >/dev/null
MANIFEST="$ROOT_DIR/tests/quality/hash-threshold-sweep-temp.toml"
{
  printf '[oracle]\npackage = "scenedetect-headless==0.7"\npython = "3.12"\n\n[quality]\ntolerance_frames = 0\n\n'
  for threshold in 0.10 0.15 0.20 0.25 0.30 0.35 0.40 0.45 0.50 0.55 0.60 0.65 0.70 0.75 0.80 0.85 0.90; do
    id="hash-mirror-${threshold/./-}"
    cat <<EOF
[[cases]]
id = "$id"
video = "../fixtures/generated/hash-pattern-mirror.mkv"
detector = "hash"
threshold = $threshold
min_scene_len = "1"
args = ["--size", "16", "--lowpass", "2"]

EOF
  done
} > "$MANIFEST"
python3 "$ROOT_DIR/tests/quality/run.py" --manifest "$MANIFEST" --report "$ROOT_DIR/tests/quality/output/hash-sweep.json" --limit-worst 50
cat "$ROOT_DIR/tests/quality/output/hash-sweep.json"
