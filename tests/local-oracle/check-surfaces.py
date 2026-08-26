#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import importlib.util
import io
import json
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[2]
FORMAT_CHECKER = ROOT_DIR / "tests" / "local-oracle" / "check.py"


def load_format_checker() -> Any:
    spec = importlib.util.spec_from_file_location("local_oracle_check", FORMAT_CHECKER)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load local oracle checker from {FORMAT_CHECKER}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


CHECK = load_format_checker()
ORACLE = CHECK.ORACLE


def accepted_boundaries_from_json(text: str, case_id: str, surface: str) -> list[int]:
    try:
        document = json.loads(text)
    except json.JSONDecodeError as error:
        raise ORACLE.OracleError(
            f"{case_id}: {surface}: invalid JSON: {error}"
        ) from error
    if not isinstance(document, dict):
        raise ORACLE.OracleError(f"{case_id}: {surface}: root must be an object")
    candidates = document.get("boundary_candidates")
    if not isinstance(candidates, list):
        raise ORACLE.OracleError(
            f"{case_id}: {surface}: boundary_candidates must be an array"
        )

    accepted: list[int] = []
    for index, candidate in enumerate(candidates, start=1):
        if not isinstance(candidate, dict):
            raise ORACLE.OracleError(
                f"{case_id}: {surface}: candidate {index} must be an object"
            )
        if candidate.get("status") != "accepted":
            continue
        frame = candidate.get("boundary_frame_index")
        if not isinstance(frame, int):
            raise ORACLE.OracleError(
                f"{case_id}: {surface}: candidate {index} boundary_frame_index "
                "must be an integer"
            )
        accepted.append(frame)
    return accepted


def accepted_boundaries_from_csv(text: str, case_id: str, surface: str) -> list[int]:
    try:
        rows = list(csv.DictReader(io.StringIO(text)))
    except csv.Error as error:
        raise ORACLE.OracleError(f"{case_id}: {surface}: invalid CSV: {error}") from error
    accepted: list[int] = []
    for index, row in enumerate(rows, start=1):
        if row.get("Status") != "accepted":
            continue
        raw = row.get("Boundary Frame Index")
        try:
            accepted.append(int(raw))
        except (TypeError, ValueError) as error:
            raise ORACLE.OracleError(
                f"{case_id}: {surface}: candidate {index} Boundary Frame Index "
                f"must be an integer, observed {raw!r}"
            ) from error
    return accepted


def expected_boundaries(golden: dict[str, Any]) -> list[int]:
    return [scene["start"] for scene in golden["scenes"][1:]]


def compare_boundaries(
    case: dict[str, Any], expected: list[int], observed: list[int], surface: str
) -> None:
    if len(expected) != len(observed):
        raise ORACLE.OracleError(
            f"{case['id']}: {surface}: accepted boundary count differs: "
            f"golden={len(expected)} observed={len(observed)}"
        )
    tolerance = case["tolerance_frames"]
    for index, (reference, candidate) in enumerate(zip(expected, observed), start=1):
        delta = abs(reference - candidate)
        if delta > tolerance:
            raise ORACLE.OracleError(
                f"{case['id']}: {surface}: boundary {index} differs by {delta} frames: "
                f"golden={reference} observed={candidate} tolerance={tolerance}"
            )


def html_scene_count(text: str, case_id: str, surface: str) -> int:
    if "<!doctype html>" not in text.lower() or "<table>" not in text.lower():
        raise ORACLE.OracleError(
            f"{case_id}: {surface}: expected self-contained Scene List HTML"
        )
    rows = text.lower().count("<tr>") - 1
    if rows < 0:
        raise ORACLE.OracleError(f"{case_id}: {surface}: Scene List table is missing")
    return rows


def legacy_base_command(case: dict[str, Any], candidate_bin: Path) -> list[str]:
    video = ORACLE.resolve_path(case["video"])
    return [
        str(candidate_bin),
        "-i",
        str(video),
        "-m",
        case["min_scene_len"],
        case["detector"],
        *ORACLE.detector_args(case, "candidate_args"),
    ]


def check_boundary_review(
    case: dict[str, Any], candidate_bin: Path, golden: dict[str, Any]
) -> None:
    expected = expected_boundaries(golden)
    base = legacy_base_command(case, candidate_bin)
    csv_text = ORACLE.capture(
        [*base, "list-boundaries", "--format", "csv", "--no-output-file"]
    )
    json_text = ORACLE.capture(
        [*base, "list-boundaries", "--format", "json", "--no-output-file"]
    )
    compare_boundaries(
        case,
        expected,
        accepted_boundaries_from_csv(csv_text, case["id"], "boundary-review-csv"),
        "boundary-review-csv",
    )
    compare_boundaries(
        case,
        expected,
        accepted_boundaries_from_json(json_text, case["id"], "boundary-review-json"),
        "boundary-review-json",
    )


def check_legacy_html(
    case: dict[str, Any], candidate_bin: Path, golden: dict[str, Any]
) -> None:
    text = ORACLE.capture(
        [*legacy_base_command(case, candidate_bin), "export-html", "--no-output-file"]
    )
    observed = html_scene_count(text, case["id"], "legacy-html")
    expected = len(golden["scenes"])
    if observed != expected:
        raise ORACLE.OracleError(
            f"{case['id']}: legacy-html: scene row count differs: "
            f"golden={expected} observed={observed}"
        )


def check_artifact_reuse(
    case: dict[str, Any], candidate_bin: Path, output_dir: Path
) -> None:
    reuse_dir = output_dir / case["id"] / "reuse"
    shutil.rmtree(reuse_dir, ignore_errors=True)
    reuse_dir.mkdir(parents=True)
    command = [
        *legacy_base_command(case, candidate_bin),
        "list-scenes",
        "--output",
        str(reuse_dir),
        "--filename",
        "reuse.csv",
    ]
    subprocess.run(command, cwd=ROOT_DIR, check=True, text=True, capture_output=True)
    second = subprocess.run(
        command, cwd=ROOT_DIR, check=True, text=True, capture_output=True
    )
    if "reusing Scene List output:" not in second.stderr:
        raise ORACLE.OracleError(
            f"{case['id']}: artifact-reuse: second identical public command did not "
            "report reusable Scene List output"
        )


def detection_stats_path(video: Path) -> Path:
    return video.with_name(f"{video.stem}.scenedetect.json")


def validate_detection_stats(
    path: Path, case: dict[str, Any], golden: dict[str, Any]
) -> None:
    try:
        document = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise ORACLE.OracleError(
            f"{case['id']}: detection-stats: failed to read {path}: {error}"
        ) from error
    if document.get("schema_version") != 1 or document.get("kind") != "detection_stats":
        raise ORACLE.OracleError(
            f"{case['id']}: detection-stats: expected schema_version=1 kind=detection_stats"
        )
    detector = document.get("detector")
    if not isinstance(detector, dict) or detector.get("name") != "content":
        raise ORACLE.OracleError(
            f"{case['id']}: detection-stats: expected content detector provenance"
        )
    rows = document.get("rows")
    if not isinstance(rows, list) or not rows:
        raise ORACLE.OracleError(f"{case['id']}: detection-stats: rows are missing")
    accepted = [
        row.get("frame")
        for row in rows
        if isinstance(row, dict) and row.get("decision") == "accepted"
    ]
    if not all(isinstance(frame, int) for frame in accepted):
        raise ORACLE.OracleError(
            f"{case['id']}: detection-stats: accepted frame must be an integer"
        )
    compare_boundaries(case, expected_boundaries(golden), accepted, "detection-stats")


def validate_stats_csv(path: Path, case_id: str) -> None:
    with path.open(newline="") as file:
        rows = list(csv.reader(file))
    if len(rows) < 2 or not rows[0] or rows[0][0] != "Frame Number":
        raise ORACLE.OracleError(
            f"{case_id}: native-render-stats: expected Frame Number CSV with data rows"
        )


def check_native_surfaces(
    case: dict[str, Any], candidate_bin: Path, output_dir: Path, golden: dict[str, Any]
) -> None:
    if case["detector"] != "detect-content":
        raise ORACLE.OracleError(
            f"{case['id']}: native-surfaces: native Detection Stats currently support content only"
        )

    native_dir = output_dir / case["id"] / "native"
    shutil.rmtree(native_dir, ignore_errors=True)
    native_dir.mkdir(parents=True)
    source = ORACLE.resolve_path(case["video"])
    video = native_dir / source.name
    shutil.copy2(source, video)

    ORACLE.run(
        [
            str(candidate_bin),
            "detect",
            "content",
            "-i",
            str(video),
            "-m",
            case["min_scene_len"],
            *ORACLE.detector_args(case, "candidate_args"),
            "--progress",
            "never",
            "--quiet",
        ]
    )
    stats_document = detection_stats_path(video)
    validate_detection_stats(stats_document, case, golden)

    scenes_path = native_dir / "native-scenes.csv"
    stats_csv_path = native_dir / "native-stats.csv"
    boundaries_path = native_dir / "native-boundaries.csv"
    html_path = native_dir / "native-scenes.html"

    ORACLE.run([str(candidate_bin), "render", "scenes", "-i", str(video), "-o", str(scenes_path)])
    ORACLE.run([str(candidate_bin), "render", "stats", "-i", str(video), "--csv", "-o", str(stats_csv_path)])
    ORACLE.run([str(candidate_bin), "render", "boundaries", "-i", str(video), "-o", str(boundaries_path)])
    ORACLE.run([str(candidate_bin), "render", "html", "-i", str(video), "-o", str(html_path)])

    native_scenes = ORACLE.PARITY.normalize(
        scenes_path,
        native_dir / "native-scenes.normalized.json",
        case_id=case["id"],
        source="candidate",
    )
    ORACLE.compare_scenes(
        case,
        golden["scenes"],
        native_scenes,
        reference_label="golden",
        observed_label="native-render-scenes",
    )
    validate_stats_csv(stats_csv_path, case["id"])
    compare_boundaries(
        case,
        expected_boundaries(golden),
        accepted_boundaries_from_csv(
            boundaries_path.read_text(), case["id"], "native-render-boundaries"
        ),
        "native-render-boundaries",
    )
    observed_html = html_scene_count(
        html_path.read_text(), case["id"], "native-render-html"
    )
    if observed_html != len(golden["scenes"]):
        raise ORACLE.OracleError(
            f"{case['id']}: native-render-html: scene row count differs: "
            f"golden={len(golden['scenes'])} observed={observed_html}"
        )


def command_check(args: argparse.Namespace, config: dict[str, Any]) -> int:
    ORACLE.ensure_generated_fixtures(config)
    cases = ORACLE.selected_required_cases(config, args.case_id)
    if len(cases) != 1:
        raise ORACLE.OracleError(
            "output-surface check requires one representative case; pass --case"
        )
    case = cases[0]
    golden = ORACLE.load_golden(config, case, ORACLE.resolve_path(args.golden_dir))
    candidate_bin = ORACLE.prepare_candidate(config)
    output_dir = ORACLE.resolve_path(args.output_dir)

    check_boundary_review(case, candidate_bin, golden)
    print(f"local oracle surface ok: {case['id']}: boundary review csv/json")
    check_legacy_html(case, candidate_bin, golden)
    print(f"local oracle surface ok: {case['id']}: legacy html")
    check_artifact_reuse(case, candidate_bin, output_dir)
    print(f"local oracle surface ok: {case['id']}: artifact reuse")
    check_native_surfaces(case, candidate_bin, output_dir, golden)
    print(f"local oracle surface ok: {case['id']}: Detection Stats and native renders")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Check public review, HTML, Detection Stats, render, and reuse surfaces."
    )
    parser.add_argument("--case", dest="case_id", default="content-hard-cut")
    parser.add_argument("--golden-dir", default=str(ORACLE.DEFAULT_GOLDEN_DIR))
    parser.add_argument("--output-dir", default=str(ORACLE.DEFAULT_OUTPUT_DIR))
    args = parser.parse_args()

    try:
        config = ORACLE.PARITY.load_config(ORACLE.PARITY.CONFIG_PATH)
        return command_check(args, config)
    except (
        ORACLE.PARITY.ConfigError,
        ORACLE.OracleError,
        subprocess.CalledProcessError,
    ) as error:
        print(f"local oracle failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
