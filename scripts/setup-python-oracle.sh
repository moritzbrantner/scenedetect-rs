#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UV_BIN="${UV_BIN:-uv}"

if ! command -v "$UV_BIN" >/dev/null 2>&1; then
  mkdir -p "$ROOT_DIR/.tools/uv"
  curl -LsSf https://astral.sh/uv/install.sh | env UV_INSTALL_DIR="$ROOT_DIR/.tools/uv" sh >/dev/null
  UV_BIN="$ROOT_DIR/.tools/uv/uv"
fi

"$UV_BIN" python install 3.12 >/dev/null 2>&1
"$UV_BIN" run --python 3.12 --with scenedetect-headless==0.7 -- scenedetect version >/dev/null 2>&1

echo "$UV_BIN"
