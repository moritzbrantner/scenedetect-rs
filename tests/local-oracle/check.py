#!/usr/bin/env python3
from __future__ import annotations

import argparse
import importlib.util
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[2]
ORACLE_RUNNER = ROOT_DIR / "tests" / "local-oracle" / "run.py"


def load_oracle_module() -> Any:
    spec = importlib.util.spec_from_file_location("local_oracle_run", ORACLE_RUNNER)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load local oracle runner from {ORACLE_RUNNER}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


ORACLE = load_oracle_module()


def normalize_scene_export(
    scene: Any, case_id: str, source: str, scene_number: int
) -> dict[str, int]:
    if not isinstance(scene, dict):
        raise ORACLE.OracleError(
            f"{case_id}: {source} scene {scene_number} field scene must be an object"
        )

    start_frame = scene.get("start_frame")
    end_frame = scene.get("end_frame")
    if not isinstance(start_frame, int):
        raise ORACLE.OracleError(
            f"{case_id}: {source} scene {scene_number} field start_frame must be an integer"
        )
    if start_frame < 1:
        raise ORACLE.OracleError(
            f"{case_id}: {source} scene {scene_number} field start_frame must be one-based"
        )
    if not isinstance(end_frame, int):
        raise ORACLE.OracleError(
            f"{case_id}: {source} scene {scene_number} field end_frame must be an integer"
        )

    return {"start": start_frame - 1, "end": end_frame}


def parse_json_scene_list(text: str, case_id: str) -> list[dict[str, int]]:
    source = "candidate-json"
    try:
        document = json.loads(text)
    except json.JSONDecodeError as error:
        raise ORACLE.OracleError(f"{case_id}: {source} JSON is invalid: {error}") from error

    if not isinstance(document, dict):
        raise ORACLE.OracleError(f"{case_id}: {source} root must be an object")
    scenes = document.get("scenes")
    if not isinstance(scenes, list):
        raise ORACLE.OracleError(f"{case_id}: {source} field scenes must be an array")

    scene_count = document.get("scene_count")
    if not isinstance(scene_count, int):
        raise ORACLE.OracleError(
            f"{case_id}: {source} field scene_count must be an integer"
        )
    if scene_count != len(scenes):
        raise ORACLE.OracleError(
            f"{case_id}: {source} field scene_count={scene_count} does not match "
            f"scenes={len(scenes)}"
        )

    return [
        normalize_scene_export(scene, case_id, source, index)
        for index, scene in enumerate(scenes, start=1)
    ]


def parse_ndjson_scene_list(text: str, case_id: str) -> list[dict[str, int]]:
    source = "candidate-ndjson"
    scenes: list[dict[str, int]] = []
    for index, line in enumerate(text.splitlines(), start=1):
        if not line.strip():
            continue
        try:
            event = json.loads(line)
        except json.JSONDecodeError as error:
            raise ORACLE.OracleError(
                f"{case_id}: {source} line {index} JSON is invalid: {error}"
            ) from error
        if not isinstance(event, dict):
            raise ORACLE.OracleError(
                f"{case_id}: {source} line {index} event must be an object"
            )
        if event.get("event") != "scene":
            raise ORACLE.OracleError(
                f"{case_id}: {source} line {index} field event expected 'scene', "
                f"observed {event.get('event')!r}"
            )
        scenes.append(normalize_scene_export(event, case_id, source, len(scenes) + 1))
    return scenes


def candidate_stdout(
    case: dict[str, Any], candidate_bin: Path, output_format: str
) -> str:
    video = ORACLE.resolve_path(case["video"])
    if not video.exists():
        raise ORACLE.OracleError(f"{case['id']}: fixture does not exist: {video}")

    return ORACLE.capture(
        [
            str(candidate_bin),
            "-i",
            str(video),
            "-m",
            case["min_scene_len"],
            case["detector"],
            *ORACLE.detector_args(case, "candidate_args"),
            "list-scenes",
            "--format",
            output_format,
            "--no-output-file",
        ]
    )


def run_candidate_formats(
    case: dict[str, Any], candidate_bin: Path, output_dir: Path
) -> dict[str, list[dict[str, int]]]:
    return {
        "csv": ORACLE.run_candidate_case(case, candidate_bin, output_dir),
        "json": parse_json_scene_list(
            candidate_stdout(case, candidate_bin, "json"), case["id"]
        ),
        "ndjson": parse_ndjson_scene_list(
            candidate_stdout(case, candidate_bin, "ndjson"), case["id"]
        ),
    }


def command_check(args: argparse.Namespace, config: dict[str, Any]) -> int:
    ORACLE.ensure_generated_fixtures(config)
    cases = ORACLE.selected_required_cases(config, args.case_id)
    if not cases:
        print("local oracle check: no required cases selected")
        return 0

    golden_dir = ORACLE.resolve_path(args.golden_dir)
    output_dir = ORACLE.resolve_path(args.output_dir)
    candidate_bin = ORACLE.prepare_candidate(config)

    for case in cases:
        golden = ORACLE.load_golden(config, case, golden_dir)
        outputs = run_candidate_formats(case, candidate_bin, output_dir)
        for output_format, candidate in outputs.items():
            ORACLE.compare_scenes(
                case,
                golden["scenes"],
                candidate,
                reference_label="golden",
                observed_label=f"candidate-{output_format}",
            )
            print(
                f"local oracle check ok: {case['id']}: {output_format}: "
                f"{len(candidate)} scenes"
            )

    print(f"local oracle candidate check ok: {len(cases)} cases x 3 formats")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Check Candidate Scene List formats against local PySceneDetect goldens."
    )
    ORACLE.add_shared_run_args(parser)
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
