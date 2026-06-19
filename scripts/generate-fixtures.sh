#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/tests/fixtures/generated"
mkdir -p "$OUT_DIR"

ffmpeg -y -v error \
  -f lavfi -i color=c=black:s=64x64:d=0.5:r=10 \
  -f lavfi -i color=c=white:s=64x64:d=0.5:r=10 \
  -filter_complex "[0:v][1:v]concat=n=2:v=1:a=0" \
  -c:v ffv1 \
  "$OUT_DIR/content-hard-cut.mkv"

ffmpeg -y -v error \
  -f lavfi -i color=c=black:s=64x64:d=0.5:r=10 \
  -f lavfi -i color=c=gray:s=64x64:d=0.5:r=10 \
  -filter_complex "[0:v][1:v]concat=n=2:v=1:a=0" \
  -c:v ffv1 \
  "$OUT_DIR/content-threshold-gray-cut.mkv"

ffmpeg -y -v error \
  -f lavfi -i color=c=black:s=64x64:d=0.4:r=10 \
  -f lavfi -i color=c=white:s=64x64:d=0.2:r=10 \
  -f lavfi -i color=c=black:s=64x64:d=0.4:r=10 \
  -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0" \
  -c:v ffv1 \
  "$OUT_DIR/adaptive-fast-motion.mkv"

ffmpeg -y -v error \
  -f lavfi -i color=c=black:s=64x64:d=0.4:r=10 \
  -f lavfi -i color=c=gray:s=64x64:d=0.2:r=10 \
  -f lavfi -i color=c=white:s=64x64:d=0.4:r=10 \
  -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0" \
  -c:v ffv1 \
  "$OUT_DIR/threshold-fade-return.mkv"

ffmpeg -y -v error \
  -f lavfi -i color=c=black:s=64x64:d=0.2:r=10 \
  -f lavfi -i color=c=white:s=64x64:d=0.2:r=10 \
  -f lavfi -i color=c=black:s=64x64:d=0.6:r=10 \
  -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0" \
  -c:v ffv1 \
  "$OUT_DIR/min-scene-len-close-cuts.mkv"

echo "$OUT_DIR"
