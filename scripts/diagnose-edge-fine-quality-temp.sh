#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
"$ROOT_DIR/scripts/generate-fixtures.sh" >/dev/null
MANIFEST="$ROOT_DIR/tests/quality/edge-fine-sweep-temp.toml"
{
  printf '[oracle]\npackage = "scenedetect-headless==0.7"\npython = "3.12"\n\n[quality]\ntolerance_frames = 0\n\n'
  for threshold in $(seq 35 0.1 40); do
    id="edge-fine-${threshold/./-}"
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
python3 "$ROOT_DIR/tests/quality/run.py" --manifest "$MANIFEST" --report "$ROOT_DIR/tests/quality/output/edge-fine-sweep.json" --limit-worst 100
cat "$ROOT_DIR/tests/quality/output/edge-fine-sweep.json"
