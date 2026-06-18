#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ -n "${MOONLIGHT_BIN:-}" ]]; then
  exec "$MOONLIGHT_BIN" "$@"
fi

if [[ -f "$ROOT_DIR/../moonlight/Cargo.toml" ]]; then
  exec cargo run --manifest-path "$ROOT_DIR/../moonlight/Cargo.toml" -p moonlight-cli --bin moonlight -- "$@"
fi

if command -v moonlight >/dev/null 2>&1; then
  exec moonlight "$@"
fi

exec bunx @moritzbrantner/moonlight "$@"
