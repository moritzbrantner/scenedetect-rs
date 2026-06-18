#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[2]
PARITY_DIR = ROOT_DIR / "tests" / "parity"
CONFIG_PATH = PARITY_DIR / "cases.toml"
OUTPUT_DIR = PARITY_DIR / "output"
SCENES_FILENAME = "scenes.csv"


class ConfigError(Exception):
    pass


def load_config(path: Path) -> dict[str, Any]:
    try:
        with path.open("rb") as file:
            config = tomllib.load(file)
    except tomllib.TOMLDecodeError as error:
        raise ConfigError(f"invalid TOML in {path}: {error}") from error

    oracle = config.get("oracle")
    if not isinstance(oracle, dict):
        raise ConfigError("missing [oracle] table")
    require_string(oracle, "package", "[oracle]")
    require_string(oracle, "python", "[oracle]")

    cases = config.get("cases")
    if not isinstance(cases, list) or not cases:
        raise ConfigError("missing [[cases]] entries")

    seen_ids: set[str] = set()
    for index, case in enumerate(cases, start=1):
        if not isinstance(case, dict):
            raise ConfigError(f"case {index} must be a table")
        validate_case(case, index)
        case_id = case["id"]
        if case_id in seen_ids:
            raise ConfigError(f"duplicate case id: {case_id}")
        seen_ids.add(case_id)

    return config


def validate_case(case: dict[str, Any], index: int) -> None:
    label = f"case {index}"
    for key in ("id", "status", "video", "detector", "min_scene_len"):
        require_string(case, key, label)

    status = case["status"]
    if status not in {"required", "expected-gap"}:
        raise ConfigError(f"{label} has invalid status {status!r}")

    args = case.get("args")
    if not isinstance(args, list) or not all(isinstance(value, str) for value in args):
        raise ConfigError(f"{label} must define args as a string array")

    for override_key in ("oracle_args", "candidate_args"):
        if override_key in case:
            override = case[override_key]
            if not isinstance(override, list) or not all(
                isinstance(value, str) for value in override
            ):
                raise ConfigError(f"{label} {override_key} must be a string array")

    tolerance = case.get("tolerance_frames")
    if not isinstance(tolerance, int) or tolerance < 0:
        raise ConfigError(f"{label} must define non-negative tolerance_frames")

    reason = case.get("reason")
    if status == "expected-gap" and not isinstance(reason, str):
        raise ConfigError(f"{label} expected-gap must include a reason")


def require_string(table: dict[str, Any], key: str, label: str) -> None:
    if not isinstance(table.get(key), str) or table[key] == "":
        raise ConfigError(f"{label} must define non-empty {key}")


def run(cmd: list[str], *, cwd: Path = ROOT_DIR) -> None:
    subprocess.run(cmd, cwd=cwd, check=True)


def capture(cmd: list[str], *, cwd: Path = ROOT_DIR) -> str:
    result = subprocess.run(
        cmd,
        cwd=cwd,
        check=True,
        text=True,
        stdout=subprocess.PIPE,
    )
    return result.stdout.strip()


def resolve_path(path: str) -> Path:
    value = Path(path)
    if value.is_absolute():
        return value
    return ROOT_DIR / value


def normalize(csv_path: Path, json_path: Path) -> list[dict[str, int]]:
    with json_path.open("w") as file:
        subprocess.run(
            [sys.executable, str(PARITY_DIR / "normalize-scenes.py"), str(csv_path)],
            cwd=ROOT_DIR,
            check=True,
            stdout=file,
        )
    with json_path.open() as file:
        return json.load(file)


def compare_scenes(
    case: dict[str, Any],
    reference: list[dict[str, int]],
    candidate: list[dict[str, int]],
) -> None:
    tolerance = case["tolerance_frames"]
    case_id = case["id"]

    if len(reference) != len(candidate):
        raise AssertionError(
            f"{case_id}: scene count differs: "
            f"reference={len(reference)} candidate={len(candidate)}"
        )

    for index, (ref, cand) in enumerate(zip(reference, candidate), start=1):
        for field in ("start", "end"):
            delta = abs(ref[field] - cand[field])
            if delta > tolerance:
                raise AssertionError(
                    f"{case_id}: scene {index} {field} differs by {delta} frames: "
                    f"reference={ref[field]} candidate={cand[field]} "
                    f"tolerance={tolerance}"
                )


def prepare_candidate(config: dict[str, Any]) -> Path:
    configured = os.environ.get("CANDIDATE_BIN")
    if configured:
        return resolve_path(configured)

    run(["cargo", "build", "-p", "scenedetect-cli"])
    return ROOT_DIR / "target" / "debug" / "scenedetect-rs"


def prepare_oracle(config: dict[str, Any]) -> str:
    uv_bin = capture([str(ROOT_DIR / "scripts" / "setup-python-oracle.sh")])
    return uv_bin


def run_required_case(
    case: dict[str, Any],
    config: dict[str, Any],
    uv_bin: str,
    candidate_bin: Path,
) -> None:
    case_id = case["id"]
    video = resolve_path(case["video"])
    if not video.exists():
        raise FileNotFoundError(f"{case_id}: video does not exist: {video}")

    case_dir = OUTPUT_DIR / case_id
    reference_dir = case_dir / "reference"
    candidate_dir = case_dir / "candidate"
    shutil.rmtree(case_dir, ignore_errors=True)
    reference_dir.mkdir(parents=True)
    candidate_dir.mkdir(parents=True)

    package = config["oracle"]["package"]
    python = config["oracle"]["python"]
    oracle_args = case.get("oracle_args", case["args"])
    candidate_args = case.get("candidate_args", case["args"])

    run(
        [
            uv_bin,
            "run",
            "--python",
            python,
            "--with",
            package,
            "--",
            "scenedetect",
            "-i",
            str(video),
            case["detector"],
            *oracle_args,
            "--min-scene-len",
            case["min_scene_len"],
            "list-scenes",
            "--output",
            str(reference_dir),
            "--filename",
            SCENES_FILENAME,
        ]
    )

    run(
        [
            str(candidate_bin),
            "-i",
            str(video),
            "-m",
            case["min_scene_len"],
            case["detector"],
            *candidate_args,
            "list-scenes",
            "--output",
            str(candidate_dir),
            "--filename",
            SCENES_FILENAME,
            "--quiet",
        ]
    )

    reference = normalize(
        reference_dir / SCENES_FILENAME, case_dir / "reference.json"
    )
    candidate = normalize(
        candidate_dir / SCENES_FILENAME, case_dir / "candidate.json"
    )
    compare_scenes(case, reference, candidate)
    print(f"parity ok: {case_id}: {len(reference)} scenes")


def selected_cases(cases: list[dict[str, Any]], case_id: str | None) -> list[dict[str, Any]]:
    if case_id is None:
        return cases
    matches = [case for case in cases if case["id"] == case_id]
    if not matches:
        raise ConfigError(f"unknown case id: {case_id}")
    return matches


def main() -> int:
    parser = argparse.ArgumentParser(description="Run PySceneDetect parity cases.")
    parser.add_argument("--case", dest="case_id", help="run one configured case id")
    parser.add_argument(
        "--validate-only",
        action="store_true",
        help="validate cases.toml without running parity",
    )
    args = parser.parse_args()

    try:
        config = load_config(CONFIG_PATH)
        cases = selected_cases(config["cases"], args.case_id)
    except ConfigError as error:
        print(f"config error: {error}", file=sys.stderr)
        return 2

    if args.validate_only:
        print(f"parity config ok: {len(config['cases'])} cases")
        return 0

    required = [case for case in cases if case["status"] == "required"]
    expected_gaps = [case for case in cases if case["status"] == "expected-gap"]

    for case in expected_gaps:
        print(
            f"expected gap: {case['id']}: {case['detector']}: {case['reason']}"
        )

    if not required:
        print("no required parity cases selected")
        return 0

    try:
        candidate_bin = prepare_candidate(config)
        uv_bin = prepare_oracle(config)
        for case in required:
            run_required_case(case, config, uv_bin, candidate_bin)
    except (AssertionError, FileNotFoundError, subprocess.CalledProcessError) as error:
        print(f"parity failed: {error}", file=sys.stderr)
        return 1

    print(
        f"required parity ok: {len(required)} run, "
        f"{len(expected_gaps)} expected gaps skipped"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
