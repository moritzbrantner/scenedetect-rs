#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import subprocess
import tempfile
from pathlib import Path
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[1]
DEFAULT_INPUT = ROOT_DIR / "tests" / "quality" / "output" / "report.json"
DEFAULT_OUTPUT = ROOT_DIR / "site" / "data" / "quality.json"
DEFAULT_COMMAND = "bun run quality:generated"
DEFAULT_MANIFEST = "tests/quality/corpus.generated.toml"
DEFAULT_ORACLE = "scenedetect-headless==0.7"


class QualitySnapshotError(Exception):
    pass


def load_json(path: Path) -> dict[str, Any]:
    if not path.exists():
        raise QualitySnapshotError(f"quality report not found: {path}")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise QualitySnapshotError(f"invalid quality JSON: {error}") from error
    if not isinstance(value, dict) or value.get("schema_version") != 1:
        raise QualitySnapshotError("quality report must use schema_version 1")
    return value


def git_candidate_ref() -> str:
    try:
        return subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=ROOT_DIR,
            check=True,
            text=True,
            stdout=subprocess.PIPE,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def sanitize_configuration(value: Any) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise QualitySnapshotError("quality case configuration must be an object")
    return {
        "threshold": value.get("threshold"),
        "min_scene_len": value.get("min_scene_len"),
        "args": value.get("args", []),
        "tolerance_frames": value.get("tolerance_frames"),
    }


def reproduction_command(case_id: str) -> str:
    return f"bun run quality:generated -- --case {case_id}"


def sanitize_case(case: Any) -> dict[str, Any]:
    if not isinstance(case, dict):
        raise QualitySnapshotError("quality cases must be objects")
    case_id = case.get("id")
    detector = case.get("detector")
    if not isinstance(case_id, str) or not case_id:
        raise QualitySnapshotError("quality case is missing id")
    if not isinstance(detector, str) or not detector:
        raise QualitySnapshotError(f"quality case {case_id} is missing detector")
    return {
        "id": case_id,
        "detector": detector,
        "configuration": sanitize_configuration(case.get("configuration")),
        "reproduction_command": reproduction_command(case_id),
        "reference_boundary_count": case.get("reference_boundary_count"),
        "candidate_boundary_count": case.get("candidate_boundary_count"),
        "matched_boundary_count": case.get("matched_boundary_count"),
        "false_positive_count": case.get("false_positive_count"),
        "false_negative_count": case.get("false_negative_count"),
        "max_absolute_delta_frames": case.get("max_absolute_delta_frames"),
        "mean_absolute_delta_frames": case.get("mean_absolute_delta_frames"),
        "matches": case.get("matches", []),
        "false_positives": case.get("false_positives", []),
        "false_negatives": case.get("false_negatives", []),
    }


def sanitize_divergence(item: Any) -> dict[str, Any]:
    if not isinstance(item, dict):
        raise QualitySnapshotError("quality divergences must be objects")
    case_id = item.get("case")
    if not isinstance(case_id, str) or not case_id:
        raise QualitySnapshotError("quality divergence is missing case")
    result = {
        key: item[key]
        for key in (
            "case",
            "detector",
            "kind",
            "frame",
            "timecode",
            "candidate_frame",
            "delta_frames",
            "absolute_delta_frames",
            "severity",
            "configuration",
        )
        if key in item
    }
    result["reproduction_command"] = reproduction_command(case_id)
    return result


def build_snapshot(
    report: dict[str, Any],
    *,
    candidate_ref: str,
    machine_label: str,
    command: str,
) -> dict[str, Any]:
    totals = report.get("totals")
    cases = report.get("cases")
    divergences = report.get("worst_divergences")
    if not isinstance(totals, dict):
        raise QualitySnapshotError("quality report is missing totals")
    if not isinstance(cases, list):
        raise QualitySnapshotError("quality report is missing cases")
    if not isinstance(divergences, list):
        raise QualitySnapshotError("quality report is missing worst_divergences")

    return {
        "schema_version": 1,
        "generated_at": report.get("generated_at"),
        "candidate_ref": candidate_ref,
        "reference_oracle": DEFAULT_ORACLE,
        "source": {
            "command": command,
            "manifest": DEFAULT_MANIFEST,
            "machine_label": machine_label,
            "report_only": True,
        },
        "totals": totals,
        "worst_divergences": [sanitize_divergence(item) for item in divergences],
        "cases": [sanitize_case(case) for case in cases],
    }


def write_snapshot(snapshot: dict[str, Any], path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(snapshot, indent=2, sort_keys=False) + "\n", encoding="utf-8")


def fixture_report() -> dict[str, Any]:
    return {
        "schema_version": 1,
        "generated_at": "2026-09-11T00:00:00+00:00",
        "totals": {
            "cases": 1,
            "reference_boundaries": 1,
            "candidate_boundaries": 1,
            "matched_boundaries": 1,
            "false_positives": 0,
            "false_negatives": 0,
        },
        "worst_divergences": [],
        "cases": [
            {
                "id": "fixture-content",
                "video": "/private/runner/path/video.mkv",
                "detector": "content",
                "configuration": {
                    "threshold": 20.0,
                    "min_scene_len": "1",
                    "args": [],
                    "tolerance_frames": 1,
                },
                "reproduction_command": "scenedetect-rs detect content -i /private/runner/path/video.mkv",
                "reference_boundary_count": 1,
                "candidate_boundary_count": 1,
                "matched_boundary_count": 1,
                "false_positive_count": 0,
                "false_negative_count": 0,
                "matches": [
                    {
                        "reference_frame": 10,
                        "candidate_frame": 10,
                        "delta_frames": 0,
                        "absolute_delta_frames": 0,
                        "reference_timecode": "00:00:01.000",
                        "candidate_timecode": "00:00:01.000",
                    }
                ],
                "false_positives": [],
                "false_negatives": [],
                "max_absolute_delta_frames": 0,
                "mean_absolute_delta_frames": 0.0,
                "detection_stats": "/private/runner/path/source.scenedetect.json",
            }
        ],
    }


def check_fixture() -> None:
    snapshot = build_snapshot(
        fixture_report(),
        candidate_ref="fixture-ref",
        machine_label="fixture-runner",
        command=DEFAULT_COMMAND,
    )
    encoded = json.dumps(snapshot)
    assert "/private/runner/path" not in encoded
    assert snapshot["cases"][0]["reproduction_command"] == (
        "bun run quality:generated -- --case fixture-content"
    )
    assert snapshot["source"]["report_only"] is True

    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "quality.json"
        write_snapshot(snapshot, path)
        loaded = json.loads(path.read_text(encoding="utf-8"))
        assert loaded == snapshot


def main() -> int:
    parser = argparse.ArgumentParser(description="Publish a sanitized generated-quality snapshot.")
    parser.add_argument("--input", type=Path, default=DEFAULT_INPUT)
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--candidate-ref", default=None)
    parser.add_argument("--machine-label", default="local")
    parser.add_argument("--command", default=DEFAULT_COMMAND)
    parser.add_argument("--check-fixture", action="store_true")
    args = parser.parse_args()

    try:
        if args.check_fixture:
            check_fixture()
            print("quality snapshot fixture check ok")
            return 0
        snapshot = build_snapshot(
            load_json(args.input),
            candidate_ref=args.candidate_ref or git_candidate_ref(),
            machine_label=args.machine_label,
            command=args.command,
        )
        write_snapshot(snapshot, args.output)
        print(f"wrote quality snapshot: {args.output}")
        return 0
    except (AssertionError, QualitySnapshotError) as error:
        print(f"quality snapshot error: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
