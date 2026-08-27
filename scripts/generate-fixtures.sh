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

# Similar-luma colors force Content to rely on chroma/hue behavior instead of
# a trivial luminance jump. Histogram uses the same fixture as a negative/stress
# case because its luma distribution should remain comparatively stable.
ffmpeg -y -v error \
  -f lavfi -i color=c=0xff0000:s=64x64:d=0.5:r=10 \
  -f lavfi -i color=c=0x008200:s=64x64:d=0.5:r=10 \
  -filter_complex "[0:v][1:v]concat=n=2:v=1:a=0,format=rgb24" \
  -c:v ffv1 \
  "$OUT_DIR/content-color-only-cut.mkv"

ffmpeg -y -v error \
  -f lavfi -i color=c=black:s=64x64:d=0.4:r=10 \
  -f lavfi -i color=c=white:s=64x64:d=0.2:r=10 \
  -f lavfi -i color=c=black:s=64x64:d=0.4:r=10 \
  -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0" \
  -c:v ffv1 \
  "$OUT_DIR/adaptive-fast-motion.mkv"

# One-frame flash stresses Adaptive windowing and the surrounding Content values.
ffmpeg -y -v error \
  -f lavfi -i color=c=black:s=64x64:d=0.4:r=10 \
  -f lavfi -i color=c=white:s=64x64:d=0.1:r=10 \
  -f lavfi -i color=c=black:s=64x64:d=0.5:r=10 \
  -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0" \
  -c:v ffv1 \
  "$OUT_DIR/adaptive-single-flash.mkv"

ffmpeg -y -v error \
  -f lavfi -i color=c=black:s=64x64:d=0.4:r=10 \
  -f lavfi -i color=c=gray:s=64x64:d=0.2:r=10 \
  -f lavfi -i color=c=white:s=64x64:d=0.4:r=10 \
  -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0" \
  -c:v ffv1 \
  "$OUT_DIR/threshold-fade-return.mkv"

# Gradual fade-out, short black hold, then fade-in exercises Threshold midpoint
# and fade-bias behavior rather than only step-wise luminance transitions.
ffmpeg -y -v error \
  -f lavfi -i color=c=white:s=64x64:d=0.8:r=10 \
  -f lavfi -i color=c=black:s=64x64:d=0.2:r=10 \
  -f lavfi -i color=c=white:s=64x64:d=0.8:r=10 \
  -filter_complex "[0:v]fade=t=out:st=0.2:d=0.5[f0];[2:v]fade=t=in:st=0:d=0.5[f2];[f0][1:v][f2]concat=n=3:v=1:a=0,format=rgb24" \
  -c:v ffv1 \
  "$OUT_DIR/threshold-gradual-fade.mkv"

ffmpeg -y -v error \
  -f lavfi -i color=c=black:s=64x64:d=0.2:r=10 \
  -f lavfi -i color=c=white:s=64x64:d=0.2:r=10 \
  -f lavfi -i color=c=black:s=64x64:d=0.6:r=10 \
  -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0" \
  -c:v ffv1 \
  "$OUT_DIR/min-scene-len-close-cuts.mkv"

ffmpeg -y -v error \
  -f lavfi -i color=c=black:s=64x64:d=0.5:r=10 \
  -f lavfi -i testsrc=size=64x64:rate=10:duration=0.5 \
  -filter_complex "[0:v][1:v]concat=n=2:v=1:a=0,format=rgb24" \
  -c:v ffv1 \
  "$OUT_DIR/hash-pattern-cut.mkv"

# A mirrored structural change is subtler than black -> pattern and stresses
# perceptual hash sensitivity without changing dimensions or frame cadence.
ffmpeg -y -v error \
  -f lavfi -i testsrc=size=64x64:rate=10:duration=0.5 \
  -f lavfi -i testsrc=size=64x64:rate=10:duration=0.5 \
  -filter_complex "[1:v]hflip[mirrored];[0:v][mirrored]concat=n=2:v=1:a=0,format=rgb24" \
  -c:v ffv1 \
  "$OUT_DIR/hash-pattern-mirror.mkv"

echo "$OUT_DIR"
