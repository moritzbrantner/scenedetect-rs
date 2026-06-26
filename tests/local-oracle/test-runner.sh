#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

set +e
OUTPUT="$(
  "$ROOT_DIR/tests/local-oracle/run.py" \
    check-golden \
    --case content-hard-cut \
    --golden-dir "$TMP_DIR/missing-goldens" \
    --scenes-json "$TMP_DIR/scenes.json" 2>&1
)"
STATUS=$?
set -e

if [[ "$STATUS" -eq 0 ]]; then
  echo "expected missing golden comparison to fail" >&2
  exit 1
fi

if [[ "$OUTPUT" != *"content-hard-cut: missing golden"* ]]; then
  echo "expected missing golden error to include case id and remediation" >&2
  echo "$OUTPUT" >&2
  exit 1
fi

echo "local oracle ok: missing golden reports case id"

mkdir -p "$TMP_DIR/goldens"
printf 'not json\n' >"$TMP_DIR/goldens/content-hard-cut.json"
printf '[{"start": 0, "end": 10}]\n' >"$TMP_DIR/scenes.json"

set +e
OUTPUT="$(
  "$ROOT_DIR/tests/local-oracle/run.py" \
    check-golden \
    --case content-hard-cut \
    --golden-dir "$TMP_DIR/goldens" \
    --scenes-json "$TMP_DIR/scenes.json" 2>&1
)"
STATUS=$?
set -e

if [[ "$STATUS" -eq 0 ]]; then
  echo "expected malformed golden comparison to fail" >&2
  exit 1
fi

if [[ "$OUTPUT" != *"content-hard-cut: malformed golden"* ]]; then
  echo "expected malformed golden error to include case id" >&2
  echo "$OUTPUT" >&2
  exit 1
fi

echo "local oracle ok: malformed golden reports case id"

cat >"$TMP_DIR/goldens/content-hard-cut.json" <<JSON
{
  "metadata": {
    "schema_version": 1,
    "oracle_package": "scenedetect-headless==0.6",
    "python": "3.12",
    "python_version": "3.12.0",
    "case_id": "content-hard-cut",
    "detector_command": "detect-content",
    "detector_args": ["--threshold", "20"],
    "min-scene-len": "1",
    "fixture_identity": "tests/fixtures/generated/content-hard-cut.mkv",
    "fixture_content_hash": "sha256:test"
  },
  "scenes": [{"start": 0, "end": 10}]
}
JSON

set +e
OUTPUT="$(
  "$ROOT_DIR/tests/local-oracle/run.py" \
    check-golden \
    --case content-hard-cut \
    --golden-dir "$TMP_DIR/goldens" \
    --scenes-json "$TMP_DIR/scenes.json" \
    --skip-fixture-hash-check 2>&1
)"
STATUS=$?
set -e

if [[ "$STATUS" -eq 0 ]]; then
  echo "expected stale golden comparison to fail" >&2
  exit 1
fi

if [[ "$OUTPUT" != *"content-hard-cut: stale golden: metadata.oracle_package"* ]]; then
  echo "expected stale golden error to include case id and metadata field" >&2
  echo "$OUTPUT" >&2
  exit 1
fi

echo "local oracle ok: stale metadata reports field"

cat >"$TMP_DIR/goldens/content-hard-cut.json" <<JSON
{
  "metadata": {
    "schema_version": 1,
    "oracle_package": "scenedetect-headless==0.7",
    "python": "3.12",
    "python_version": "3.12.0",
    "case_id": "content-hard-cut",
    "detector_command": "detect-content",
    "detector_args": ["--threshold", "20"],
    "min-scene-len": "1",
    "fixture_identity": "tests/fixtures/generated/content-hard-cut.mkv",
    "fixture_content_hash": "sha256:test"
  },
  "scenes": [{"start": 0, "end": 8}]
}
JSON

set +e
OUTPUT="$(
  "$ROOT_DIR/tests/local-oracle/run.py" \
    check-golden \
    --case content-hard-cut \
    --golden-dir "$TMP_DIR/goldens" \
    --scenes-json "$TMP_DIR/scenes.json" \
    --skip-metadata-check 2>&1
)"
STATUS=$?
set -e

if [[ "$STATUS" -eq 0 ]]; then
  echo "expected mismatched golden comparison to fail" >&2
  exit 1
fi

if [[ "$OUTPUT" != *"content-hard-cut: scene 1 end differs"* ]]; then
  echo "expected mismatch error to include case id, scene, and field" >&2
  echo "$OUTPUT" >&2
  exit 1
fi

echo "local oracle ok: mismatch reports scene field"
