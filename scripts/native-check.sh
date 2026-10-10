#!/usr/bin/env bash
# Native fresh-clone path: Rust toolchain + ffmpeg only. No Python, uv, Bun
# packages or PySceneDetect oracle are needed. Builds the workspace, runs every
# Rust test (ffmpeg-backed tests must run, not skip), and runs native Content
# detection on a generated hard-cut fixture.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

for tool in cargo ffmpeg ffprobe; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "native-check: $tool is required on PATH" >&2
    exit 1
  fi
done

cargo build --locked -p scenedetect-cli
cargo test --locked --workspace

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT
VIDEO="$WORK_DIR/native-hard-cut.mkv"
ffmpeg -y -v error \
  -f lavfi -i color=c=black:s=64x64:d=0.5:r=10 \
  -f lavfi -i color=c=white:s=64x64:d=0.5:r=10 \
  -filter_complex "[0:v][1:v]concat=n=2:v=1:a=0" \
  -c:v ffv1 \
  "$VIDEO"

target/debug/scenedetect-rs detect content -i "$VIDEO" --threshold 20 --min-scene-len 1

STATS="$WORK_DIR/native-hard-cut.scenedetect.json"
if [ ! -s "$STATS" ]; then
  echo "native-check: detection stats were not written to $STATS" >&2
  exit 1
fi
ACCEPTED="$(grep -o '"decision": *"accepted"' "$STATS" | wc -l)"
if [ "$ACCEPTED" -ne 1 ]; then
  echo "native-check: expected exactly one accepted Scene Boundary on the hard cut, found $ACCEPTED" >&2
  exit 1
fi
echo "native-check: native build, tests and hard-cut detection passed"
