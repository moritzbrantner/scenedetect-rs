#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
UV_BIN="${UV_BIN:-uv}"
UV_VERSION="${UV_VERSION:-0.11.21}"
UV_INSTALL_URL="${UV_INSTALL_URL:-https://astral.sh/uv/${UV_VERSION}/install.sh}"
PYSCENEDETECT_ORACLE_PYTHON="${PYSCENEDETECT_ORACLE_PYTHON:-3.12}"
PYSCENEDETECT_ORACLE_PACKAGE="${PYSCENEDETECT_ORACLE_PACKAGE:-scenedetect-headless==0.7}"

export UV_CACHE_DIR="${UV_CACHE_DIR:-$ROOT_DIR/.tools/uv-cache}"
export UV_EXCLUDE_NEWER="${UV_EXCLUDE_NEWER:-2026-06-19T00:00:00Z}"
export UV_PYTHON_INSTALL_DIR="${UV_PYTHON_INSTALL_DIR:-$ROOT_DIR/.tools/python}"

if ! command -v "$UV_BIN" >/dev/null 2>&1; then
  echo "local oracle prerequisite missing: uv ($UV_BIN)" >&2
  if ! command -v curl >/dev/null 2>&1; then
    echo "uv is required for the PySceneDetect oracle and curl is unavailable for the pinned bootstrap." >&2
    echo "Install uv or curl, then rerun the oracle setup." >&2
    exit 127
  fi
  echo "Bootstrapping pinned uv $UV_VERSION into $ROOT_DIR/.tools/uv." >&2
  mkdir -p "$ROOT_DIR/.tools/uv"
  curl --proto '=https' --tlsv1.2 -LsSf "$UV_INSTALL_URL" \
    | env UV_INSTALL_DIR="$ROOT_DIR/.tools/uv" sh >&2
  UV_BIN="$ROOT_DIR/.tools/uv/uv"
fi

if [[ ! -x "$UV_BIN" ]] && ! command -v "$UV_BIN" >/dev/null 2>&1; then
  echo "local oracle prerequisite failed: uv is still unavailable after setup." >&2
  exit 127
fi

"$UV_BIN" --version >&2
"$UV_BIN" python install "$PYSCENEDETECT_ORACLE_PYTHON" --managed-python --no-bin >&2
"$UV_BIN" run \
  --managed-python \
  --python "$PYSCENEDETECT_ORACLE_PYTHON" \
  --with "$PYSCENEDETECT_ORACLE_PACKAGE" \
  -- \
  scenedetect version >&2

echo "$UV_BIN"
