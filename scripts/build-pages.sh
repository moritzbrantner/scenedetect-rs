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

cargo build --locked -p scenedetect-wasm --target wasm32-unknown-unknown --release
mkdir -p site/wasm
cp target/wasm32-unknown-unknown/release/scenedetect_wasm.wasm "$WASM_OUT"
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
