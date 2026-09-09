#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
"$ROOT_DIR/scripts/generate-fixtures.sh" >/dev/null
MANIFEST="$ROOT_DIR/tests/quality/edge-threshold-sweep-temp.toml"
{
  printf '[oracle]\npackage = "scenedetect-headless==0.7"\npython = "3.12"\n\n[quality]\ntolerance_frames = 0\n\n'
  for threshold in 1 2 3 4 5 7.5 10 12.5 15 17.5 20 22.5 25 30 35 40 50 60 70 80 90 100 120 140 160 180 200; do
    id="edge-only-${threshold/./-}"
    cat <<EOF
[[cases]]
id = "$id"
video = "../fixtures/generated/content-edge-only-cut.mkv"
detector = "content"
threshold = $threshold
min_scene_len = "1"
args = ["--weights", "0", "0", "0", "1"]

EOF
  done
} > "$MANIFEST"
python3 "$ROOT_DIR/tests/quality/run.py" --manifest "$MANIFEST" --report "$ROOT_DIR/tests/quality/output/edge-sweep.json" --limit-worst 50
cat "$ROOT_DIR/tests/quality/output/edge-sweep.json"
