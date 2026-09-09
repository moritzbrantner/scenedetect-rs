#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
"$ROOT_DIR/scripts/generate-fixtures.sh" >/dev/null
MANIFEST="$ROOT_DIR/tests/quality/min-scene-sweep-temp.toml"
{
  printf '[oracle]\npackage = "scenedetect-headless==0.7"\npython = "3.12"\n\n[quality]\ntolerance_frames = 0\n\n'
  for min_scene_len in 1 2 3 4 5 6 7 8 9 10; do
    cat <<EOF
[[cases]]
id = "content-close-cuts-min-$min_scene_len"
video = "../fixtures/generated/min-scene-len-close-cuts.mkv"
detector = "content"
threshold = 20
min_scene_len = "$min_scene_len"
args = []

EOF
  done
} > "$MANIFEST"
python3 "$ROOT_DIR/tests/quality/run.py" --manifest "$MANIFEST" --report "$ROOT_DIR/tests/quality/output/min-scene-sweep.json" --limit-worst 50
cat "$ROOT_DIR/tests/quality/output/min-scene-sweep.json"
