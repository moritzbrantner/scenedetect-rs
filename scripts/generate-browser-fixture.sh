#!/usr/bin/env bash
# Regenerates the committed browser-acceptance fixture video (#152).
#
# Three solid-colour shots of 3.1 s each at 10 fps (64x36, VP9 in WebM), so
# hard Scene Boundaries fall at 3.1 s and 6.2 s. The cut times deliberately sit
# between the workbench's default 6 fps sampling instants, so the first sample
# showing each new shot is sample 19 (3.1667 s) and sample 38 (6.3333 s).
# The output is committed; regenerate only when the fixture contract changes
# and update tests/browser/fixtures/three-shot-cuts.expected.json with it.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT_DIR/tests/browser/fixtures/three-shot-cuts.webm"

ffmpeg -y -v error \
  -f lavfi -i color=c=0xd03020:s=64x36:d=3.1:r=10 \
  -f lavfi -i color=c=0x20a040:s=64x36:d=3.1:r=10 \
  -f lavfi -i color=c=0x2040d0:s=64x36:d=3.1:r=10 \
  -filter_complex "[0:v][1:v][2:v]concat=n=3:v=1:a=0,format=yuv420p" \
  -c:v libvpx-vp9 -b:v 0 -crf 40 -g 10 -row-mt 0 -threads 1 \
  -fflags +bitexact -flags:v +bitexact -map_metadata -1 \
  "$OUT"

echo "$OUT ($(wc -c < "$OUT") bytes)"
