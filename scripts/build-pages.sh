#!/usr/bin/env bash
# Builds the GitHub Pages artifact (`site/`) from committed sources only (#152).
#
# The only generated file allowed in the artifact is the browser WebAssembly
# module built from the committed Rust crates with `--locked`. Any other file in
# site/ that is untracked, ignored or modified relative to HEAD is a hidden local
# input and fails the build, so CI and a fresh clone publish the same artifact.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WASM_OUT="site/wasm/scenedetect_wasm.wasm"
cd "$ROOT_DIR"

# The WASM module is generated from the Rust sources, so the whole tree (not
# only site/) must match HEAD before building: no modified tracked files and no
# untracked, non-ignored files. Ignored build output (target/, node_modules/)
# is not a source.
if ! git diff --quiet HEAD; then
  echo "Working tree differs from HEAD; the Pages build needs committed sources:" >&2
  git diff --stat HEAD >&2
  exit 1
fi
untracked="$(git ls-files --others --exclude-standard)"
if [ -n "$untracked" ]; then
  echo "Untracked files would feed the Pages build:" >&2
  echo "$untracked" >&2
  exit 1
fi

# Build into a fresh target directory so no ignored, pre-existing Cargo output
# (a stale or modified artifact that Cargo would report as fresh) can become an
# input of the published module.
BUILD_TARGET="$(mktemp -d)"
trap 'rm -rf "$BUILD_TARGET"' EXIT
CARGO_TARGET_DIR="$BUILD_TARGET" cargo build --locked -p scenedetect-wasm --target wasm32-unknown-unknown --release
mkdir -p site/wasm
cp "$BUILD_TARGET/wasm32-unknown-unknown/release/scenedetect_wasm.wasm" "$WASM_OUT"
test -s "$WASM_OUT"
echo "SceneDetect browser WASM bytes: $(wc -c < "$WASM_OUT")"

# Untracked files, including ignored ones, other than the built WASM module.
extra="$(git ls-files --others -- site | grep -vxF "$WASM_OUT" || true)"
if [ -n "$extra" ]; then
  echo "Pages artifact contains files that are not committed:" >&2
  echo "$extra" >&2
  exit 1
fi
if ! git diff --quiet HEAD -- site; then
  echo "Pages artifact differs from the committed site/ sources:" >&2
  git diff --stat HEAD -- site >&2
  exit 1
fi
echo "Pages artifact: $(git ls-files -- site | wc -l) committed files + $WASM_OUT"
