#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
"$ROOT_DIR/scripts/generate-fixtures.sh" >/dev/null
MANIFEST="$ROOT_DIR/tests/quality/threshold-bias-sweep-temp.toml"
{
  printf '[oracle]\npackage = "scenedetect-headless==0.7"\npython = "3.12"\n\n[quality]\ntolerance_frames = 0\n\n'
  for bias in -1 -0.9 -0.75 -0.5 -0.25 0 0.25 0.5 0.75 0.9 1; do
    id="threshold-fade-bias-${bias/-/neg-}"
    id="${id/./-}"
    cat <<EOF
[[cases]]
id = "$id"
video = "../fixtures/generated/threshold-fade-return.mkv"
detector = "threshold"
threshold = 12
min_scene_len = "1"
args = ["--fade-bias", "$bias"]

EOF
  done
} > "$MANIFEST"
python3 "$ROOT_DIR/tests/quality/run.py" --manifest "$MANIFEST" --report "$ROOT_DIR/tests/quality/output/threshold-bias-sweep.json" --limit-worst 50
cat "$ROOT_DIR/tests/quality/output/threshold-bias-sweep.json"
