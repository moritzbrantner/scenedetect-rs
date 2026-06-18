#!/usr/bin/env bash
set -euo pipefail

MODE="${1:---generated-only}"
ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT_DIR="$ROOT_DIR/tests/benchmarks/generated"
mkdir -p "$OUT_DIR"

ffmpeg -y -v error \
  -f lavfi -i color=c=black:s=320x180:d=5:r=24 \
  -f lavfi -i color=c=white:s=320x180:d=5:r=24 \
  -f lavfi -i color=c=black:s=320x180:d=5:r=24 \
  -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0" \
  -c:v ffv1 \
  "$OUT_DIR/generated-hard-cuts.mkv"

ffmpeg -y -v error \
  -f lavfi -i color=c=black:s=320x180:d=4:r=24 \
  -f lavfi -i color=c=gray:s=320x180:d=4:r=24 \
  -f lavfi -i color=c=white:s=320x180:d=4:r=24 \
  -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0" \
  -c:v ffv1 \
  "$OUT_DIR/generated-fade.mkv"

if [[ "$MODE" != "--include-real" ]]; then
  echo "$OUT_DIR"
  exit 0
fi

REAL_SOURCE="${BENCH_REAL_SOURCE:-$ROOT_DIR/../native-whisperx/reference/Shrek Retold - Full Movie [pM70TROZQsI].webm}"

if [[ ! -f "$REAL_SOURCE" ]]; then
  echo "benchmark real source not found, skipping real clips: $REAL_SOURCE" >&2
  echo "$OUT_DIR"
  exit 0
fi

generate_real_clip() {
  local start="$1"
  local filename="$2"

  ffmpeg -y -v error \
    -ss "$start" \
    -i "$REAL_SOURCE" \
    -t 30 \
    -vf "scale=640:-2" \
    -an \
    -c:v ffv1 \
    "$OUT_DIR/$filename"
}

generate_real_clip "00:01:00" "shrek-retold-early-30s.mkv"
generate_real_clip "00:45:00" "shrek-retold-middle-30s.mkv"
generate_real_clip "01:20:00" "shrek-retold-late-30s.mkv"

echo "$OUT_DIR"
