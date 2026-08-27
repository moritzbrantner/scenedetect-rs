#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import json
import os
import re
import shutil
import subprocess
import sys
import time
import tomllib
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT_DIR = Path(__file__).resolve().parents[2]
QUALITY_DIR = ROOT_DIR / "tests" / "quality"
DEFAULT_MANIFEST = QUALITY_DIR / "corpus.local.toml"
DEFAULT_OUTPUT_DIR = QUALITY_DIR / "output"
DEFAULT_REPORT = DEFAULT_OUTPUT_DIR / "report.json"
SCENES_FILENAME = "scenes.csv"
DETECTORS = {"content", "adaptive", "threshold", "hist", "hash"}
SAFE_CASE_ID = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")
ORACLE_DETECTORS = {
    "content": "detect-content",
    "adaptive": "detect-adaptive",
    "threshold": "detect-threshold",
    "hist": "detect-hist",
    "hash": "detect-hash",
}


class ConfigError(Exception):
    pass


def load_config(path: Path) -> dict[str, Any]:
    try:
        with path.open("rb") as file:
            config = tomllib.load(file)
    except FileNotFoundError as error:
        raise ConfigError(
            f"quality manifest does not exist: {path}; copy corpus.example.toml to corpus.local.toml"
        ) from error
    except tomllib.TOMLDecodeError as error:
        raise ConfigError(f"invalid TOML in {path}: {error}") from error

    oracle = config.get("oracle", {})
    if not isinstance(oracle, dict):
        raise ConfigError("[oracle] must be a table")
    oracle.setdefault("package", "scenedetect-headless==0.7")
    oracle.setdefault("python", "3.12")

    quality = config.get("quality", {})
    if not isinstance(quality, dict):
        raise ConfigError("[quality] must be a table")
    default_tolerance = quality.get("tolerance_frames", 1)
    if not isinstance(default_tolerance, int) or default_tolerance < 0:
        raise ConfigError("quality.tolerance_frames must be a non-negative integer")

    cases = config.get("cases")
    if not isinstance(cases, list) or not cases:
        raise ConfigError("manifest must contain at least one [[cases]] entry")

    seen_ids: set[str] = set()
    for index, case in enumerate(cases, start=1):
        if not isinstance(case, dict):
            raise ConfigError(f"case {index} must be a table")
        case_id = require_string(case, "id", index)
        if not SAFE_CASE_ID.fullmatch(case_id):
            raise ConfigError(
                f"case id {case_id!r} must be a single safe path component "
                "using only letters, digits, dot, underscore, and hyphen"
            )
        if case_id in seen_ids:
            raise ConfigError(f"duplicate case id: {case_id}")
        seen_ids.add(case_id)
        require_string(case, "video", index)
        detector = require_string(case, "detector", index)
        if detector not in DETECTORS:
            raise ConfigError(f"case {case_id} has unsupported detector {detector!r}")
        if "threshold" not in case or not isinstance(case["threshold"], (int, float)):
            raise ConfigError(f"case {case_id} must define numeric threshold")
        min_scene_len = case.get("min_scene_len", "15")
        if not isinstance(min_scene_len, (str, int)):
            raise ConfigError(f"case {case_id} min_scene_len must be a string or integer")
        case["min_scene_len"] = str(min_scene_len)
        tolerance = case.get("tolerance_frames", default_tolerance)
        if not isinstance(tolerance, int) or tolerance < 0:
            raise ConfigError(f"case {case_id} tolerance_frames must be non-negative")
        case["tolerance_frames"] = tolerance
        for key in ("args", "oracle_args", "candidate_args"):
            values = case.get(key, [])
            if not isinstance(values, list) or not all(isinstance(value, str) for value in values):
                raise ConfigError(f"case {case_id} {key} must be an array of strings")
        enabled = case.get("enabled", True)
        if not isinstance(enabled, bool):
            raise ConfigError(f"case {case_id} enabled must be boolean")

    config["oracle"] = oracle
    config["quality"] = quality
    return config


def require_string(case: dict[str, Any], key: str, index: int) -> str:
    value = case.get(key)
    if not isinstance(value, str) or not value:
        raise ConfigError(f"case {index} must define non-empty {key}")
    return value


def resolve_video(manifest: Path, value: str) -> Path:
    path = Path(os.path.expanduser(value))
    if not path.is_absolute():
        path = manifest.parent / path
    return path.resolve()


def run(cmd: list[str], *, cwd: Path = ROOT_DIR) -> None:
    subprocess.run(cmd, cwd=cwd, check=True)


def capture(cmd: list[str], *, cwd: Path = ROOT_DIR) -> str:
    return subprocess.run(
        cmd,
        cwd=cwd,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    ).stdout.strip()


def prepare_candidate() -> Path:
    configured = os.environ.get("CANDIDATE_BIN")
    if configured:
        path = Path(configured).expanduser().resolve()
        if not path.exists():
            raise FileNotFoundError(f"CANDIDATE_BIN does not exist: {path}")
        return path
    run(["cargo", "build", "-p", "scenedetect-cli"])
    return ROOT_DIR / "target" / "debug" / "scenedetect-rs"


def prepare_oracle() -> str:
    return capture([str(ROOT_DIR / "scripts" / "setup-python-oracle.sh")])


def case_args(case: dict[str, Any], key: str) -> list[str]:
    values = case.get(key)
    if values:
        return list(values)
    return list(case.get("args", []))


def normalized_oracle_scenes(csv_path: Path) -> list[dict[str, int]]:
    lines = csv_path.read_text().splitlines()
    header_index = next(
        (
            index
            for index, line in enumerate(lines)
            if "scene number" in line.lower() and "start frame" in line.lower()
        ),
        None,
    )
    if header_index is None:
        raise AssertionError(f"could not find Scene List header in {csv_path}")
    reader = csv.DictReader(lines[header_index:])
    scenes: list[dict[str, int]] = []
    for row in reader:
        normalized = {
            key.strip().lower().replace(" ", "_"): value for key, value in row.items()
        }
        try:
            start = int(normalized["start_frame"]) - 1
            end = int(normalized["end_frame"])
        except (KeyError, TypeError, ValueError):
            continue
        scenes.append({"start": start, "end": end})
    return scenes


def native_scenes(path: Path) -> list[dict[str, int]]:
    payload = json.loads(path.read_text())
    scenes = payload.get("scenes") if isinstance(payload, dict) else None
    if not isinstance(scenes, list):
        raise AssertionError(f"native Scene List is invalid: {path}")
    return [
        {"start": int(scene["start_frame"]) - 1, "end": int(scene["end_frame"])}
        for scene in scenes
    ]


def boundaries(scenes: list[dict[str, int]]) -> list[int]:
    return [scene["start"] for scene in scenes[1:]]


def match_boundaries(
    reference: list[int], candidate: list[int], tolerance: int
) -> tuple[list[dict[str, int]], list[int], list[int]]:
    # Boundaries are ordered by frame. Dynamic programming over ordered
    # prefixes maximizes one-to-one match cardinality first, then minimizes
    # total absolute frame delta.
    match_action = 1
    skip_reference_action = 2
    skip_candidate_action = 3
    action_priority = {
        match_action: 2,
        skip_reference_action: 1,
        skip_candidate_action: 0,
    }

    def better(
        score: tuple[int, int],
        action: int,
        best_score: tuple[int, int],
        best_action: int,
    ) -> bool:
        if score[0] != best_score[0]:
            return score[0] > best_score[0]
        if score[1] != best_score[1]:
            return score[1] < best_score[1]
        return action_priority[action] > action_priority[best_action]

    width = len(candidate) + 1
    back = bytearray((len(reference) + 1) * width)
    for column in range(1, width):
        back[column] = skip_candidate_action

    previous = [(0, 0)] * width
    for row, reference_frame in enumerate(reference, start=1):
        current = [(0, 0)] * width
        back[row * width] = skip_reference_action
        for column, candidate_frame in enumerate(candidate, start=1):
            best_score = previous[column]
            best_action = skip_reference_action

            left_score = current[column - 1]
            if better(left_score, skip_candidate_action, best_score, best_action):
                best_score = left_score
                best_action = skip_candidate_action

            delta = abs(candidate_frame - reference_frame)
            if delta <= tolerance:
                diagonal = previous[column - 1]
                match_score = (diagonal[0] + 1, diagonal[1] + delta)
                if better(match_score, match_action, best_score, best_action):
                    best_score = match_score
                    best_action = match_action

            current[column] = best_score
            back[row * width + column] = best_action
        previous = current

    matched_pairs: list[tuple[int, int]] = []
    row = len(reference)
    column = len(candidate)
    while row > 0 or column > 0:
        action = back[row * width + column]
        if action == match_action:
            matched_pairs.append((row - 1, column - 1))
            row -= 1
            column -= 1
        elif action == skip_reference_action:
            row -= 1
        elif action == skip_candidate_action:
            column -= 1
        else:
            raise AssertionError("invalid boundary matching backpointer")
    matched_pairs.reverse()

    matched_reference = {reference_index for reference_index, _ in matched_pairs}
    matched_candidate = {candidate_index for _, candidate_index in matched_pairs}
    matches = []
    for reference_index, candidate_index in matched_pairs:
        reference_frame = reference[reference_index]
        candidate_frame = candidate[candidate_index]
        matches.append(
            {
                "reference_frame": reference_frame,
                "candidate_frame": candidate_frame,
                "delta_frames": candidate_frame - reference_frame,
                "absolute_delta_frames": abs(candidate_frame - reference_frame),
            }
        )
    false_positives = [
        frame for index, frame in enumerate(candidate) if index not in matched_candidate
    ]
    false_negatives = [
        frame for index, frame in enumerate(reference) if index not in matched_reference
    ]
    return matches, false_positives, false_negatives


def timecode(frame: int, frame_rate: float) -> str:
    if frame_rate <= 0:
        return str(frame)
    milliseconds = round(frame * 1000 / frame_rate)
    hours, remainder = divmod(milliseconds, 3_600_000)
    minutes, remainder = divmod(remainder, 60_000)
    seconds, milliseconds = divmod(remainder, 1000)
    return f"{hours:02}:{minutes:02}:{seconds:02}.{milliseconds:03}"


def reproduction_command(case: dict[str, Any], video: Path) -> str:
    detector = case["detector"]
    args = [
        "scenedetect-rs",
        "detect",
        detector,
        "-i",
        str(video),
        "--threshold",
        str(case["threshold"]),
        *case_args(case, "candidate_args"),
        "--min-scene-len",
        case["min_scene_len"],
        "--progress",
        "never",
        "--force",
    ]
    return " ".join(shell_quote(value) for value in args)


def shell_quote(value: str) -> str:
    if value and all(character.isalnum() or character in "-._/:" for character in value):
        return value
    return "'" + value.replace("'", "'\\''") + "'"


def run_case(
    case: dict[str, Any],
    manifest: Path,
    output_dir: Path,
    candidate_bin: Path,
    uv_bin: str,
    oracle: dict[str, Any],
    timing: bool,
) -> dict[str, Any]:
    case_id = case["id"]
    video = resolve_video(manifest, case["video"])
    if not video.exists():
        raise FileNotFoundError(f"{case_id}: video does not exist: {video}")

    case_dir = output_dir / "cases" / case_id
    shutil.rmtree(case_dir, ignore_errors=True)
    reference_dir = case_dir / "reference"
    reference_dir.mkdir(parents=True)
    link = case_dir / f"source{video.suffix or '.video'}"
    link.symlink_to(video)

    oracle_cmd = [
        uv_bin,
        "run",
        "--python",
        oracle["python"],
        "--with",
        oracle["package"],
        "--",
        "scenedetect",
        "-i",
        str(link),
        ORACLE_DETECTORS[case["detector"]],
        "--threshold",
        str(case["threshold"]),
        *case_args(case, "oracle_args"),
        "--min-scene-len",
        case["min_scene_len"],
        "list-scenes",
        "--output",
        str(reference_dir),
        "--filename",
        SCENES_FILENAME,
    ]
    started = time.perf_counter()
    run(oracle_cmd)
    oracle_seconds = time.perf_counter() - started

    candidate_cmd = [
        str(candidate_bin),
        "detect",
        case["detector"],
        "-i",
        str(link),
        "--threshold",
        str(case["threshold"]),
        *case_args(case, "candidate_args"),
        "--min-scene-len",
        case["min_scene_len"],
        "--progress",
        "never",
        "--quiet",
        "--force",
    ]
    started = time.perf_counter()
    run(candidate_cmd)
    candidate_seconds = time.perf_counter() - started

    candidate_json = case_dir / "candidate-scenes.json"
    run(
        [
            str(candidate_bin),
            "render",
            "scenes",
            "-i",
            str(link),
            "--format",
            "json",
            "--output",
            str(candidate_json),
        ]
    )

    stats_path = case_dir / "source.scenedetect.json"
    stats = json.loads(stats_path.read_text())
    frame_rate = float(stats["input"]["frame_rate"])
    reference = boundaries(normalized_oracle_scenes(reference_dir / SCENES_FILENAME))
    candidate = boundaries(native_scenes(candidate_json))
    matches, false_positives, false_negatives = match_boundaries(
        reference, candidate, case["tolerance_frames"]
    )

    for match in matches:
        match["reference_timecode"] = timecode(match["reference_frame"], frame_rate)
        match["candidate_timecode"] = timecode(match["candidate_frame"], frame_rate)

    result: dict[str, Any] = {
        "id": case_id,
        "video": str(video),
        "detector": case["detector"],
        "configuration": {
            "threshold": case["threshold"],
            "min_scene_len": case["min_scene_len"],
            "args": case_args(case, "candidate_args"),
            "tolerance_frames": case["tolerance_frames"],
        },
        "reproduction_command": reproduction_command(case, video),
        "reference_boundary_count": len(reference),
        "candidate_boundary_count": len(candidate),
        "matched_boundary_count": len(matches),
        "false_positive_count": len(false_positives),
        "false_negative_count": len(false_negatives),
        "matches": matches,
        "false_positives": [
            {"frame": frame, "timecode": timecode(frame, frame_rate)}
            for frame in false_positives
        ],
        "false_negatives": [
            {"frame": frame, "timecode": timecode(frame, frame_rate)}
            for frame in false_negatives
        ],
        "max_absolute_delta_frames": max(
            (match["absolute_delta_frames"] for match in matches), default=0
        ),
        "mean_absolute_delta_frames": (
            sum(match["absolute_delta_frames"] for match in matches) / len(matches)
            if matches
            else 0.0
        ),
        "detection_stats": str(stats_path),
    }
    if timing:
        result["timing_seconds"] = {
            "oracle": oracle_seconds,
            "candidate": candidate_seconds,
        }
    return result


def aggregate(cases: list[dict[str, Any]], limit: int) -> dict[str, Any]:
    totals = {
        "cases": len(cases),
        "reference_boundaries": sum(case["reference_boundary_count"] for case in cases),
        "candidate_boundaries": sum(case["candidate_boundary_count"] for case in cases),
        "matched_boundaries": sum(case["matched_boundary_count"] for case in cases),
        "false_positives": sum(case["false_positive_count"] for case in cases),
        "false_negatives": sum(case["false_negative_count"] for case in cases),
    }
    divergences: list[dict[str, Any]] = []
    for case in cases:
        common = {
            "case": case["id"],
            "video": case["video"],
            "detector": case["detector"],
            "configuration": case["configuration"],
            "reproduction_command": case["reproduction_command"],
        }
        for item in case["false_negatives"]:
            divergences.append({**common, "kind": "false_negative", **item, "severity": 2})
        for item in case["false_positives"]:
            divergences.append({**common, "kind": "false_positive", **item, "severity": 2})
        for match in case["matches"]:
            if match["absolute_delta_frames"]:
                divergences.append(
                    {
                        **common,
                        "kind": "frame_delta",
                        "frame": match["reference_frame"],
                        "timecode": match["reference_timecode"],
                        "candidate_frame": match["candidate_frame"],
                        "delta_frames": match["delta_frames"],
                        "absolute_delta_frames": match["absolute_delta_frames"],
                        "severity": 1,
                    }
                )
    divergences.sort(
        key=lambda item: (
            item["severity"],
            item.get("absolute_delta_frames", 0),
        ),
        reverse=True,
    )
    return {"totals": totals, "worst_divergences": divergences[:limit]}


def select_cases(
    cases: list[dict[str, Any]], case_ids: set[str], detectors: set[str]
) -> list[dict[str, Any]]:
    selected = [case for case in cases if case.get("enabled", True)]
    if case_ids:
        unknown = case_ids - {case["id"] for case in cases}
        if unknown:
            raise ConfigError(f"unknown case id(s): {', '.join(sorted(unknown))}")
        selected = [case for case in selected if case["id"] in case_ids]
    if detectors:
        unknown = detectors - DETECTORS
        if unknown:
            raise ConfigError(f"unknown detector(s): {', '.join(sorted(unknown))}")
        selected = [case for case in selected if case["detector"] in detectors]
    return selected


def main() -> int:
    parser = argparse.ArgumentParser(description="Evaluate scene detection quality on local real videos.")
    parser.add_argument("--manifest", type=Path, default=DEFAULT_MANIFEST)
    parser.add_argument("--report", type=Path, default=DEFAULT_REPORT)
    parser.add_argument("--case", action="append", default=[], help="run one case id; repeatable")
    parser.add_argument("--detector", action="append", default=[], help="run one Detector; repeatable")
    parser.add_argument("--timing", action="store_true", help="record report-only runtime timings")
    parser.add_argument("--validate-only", action="store_true")
    parser.add_argument("--limit-worst", type=int, default=20)
    args = parser.parse_args()

    try:
        manifest = args.manifest.expanduser().resolve()
        config = load_config(manifest)
        cases = select_cases(config["cases"], set(args.case), set(args.detector))
    except ConfigError as error:
        print(f"quality config error: {error}", file=sys.stderr)
        return 2

    if args.validate_only:
        print(f"quality config ok: {len(config['cases'])} configured, {len(cases)} enabled/selected")
        return 0
    if not cases:
        print("no enabled quality cases selected", file=sys.stderr)
        return 2
    if args.limit_worst < 1:
        print("--limit-worst must be at least 1", file=sys.stderr)
        return 2

    output_dir = args.report.expanduser().resolve().parent
    output_dir.mkdir(parents=True, exist_ok=True)
    try:
        candidate_bin = prepare_candidate()
        uv_bin = prepare_oracle()
        results = [
            run_case(
                case,
                manifest,
                output_dir,
                candidate_bin,
                uv_bin,
                config["oracle"],
                args.timing,
            )
            for case in cases
        ]
    except (AssertionError, FileNotFoundError, OSError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        print(f"quality evaluation failed: {error}", file=sys.stderr)
        return 1

    summary = aggregate(results, args.limit_worst)
    report = {
        "schema_version": 1,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "manifest": str(manifest),
        "timing_is_report_only": args.timing,
        **summary,
        "cases": results,
    }
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    totals = summary["totals"]
    print(
        f"quality report: {args.report}: {totals['cases']} cases, "
        f"{totals['matched_boundaries']} matched, {totals['false_positives']} false positives, "
        f"{totals['false_negatives']} false negatives"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
