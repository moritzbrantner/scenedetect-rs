#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[2]
LOCAL_ORACLE_DIR = ROOT_DIR / "tests" / "local-oracle"
DEFAULT_GOLDEN_DIR = LOCAL_ORACLE_DIR / "goldens"
DEFAULT_OUTPUT_DIR = LOCAL_ORACLE_DIR / "output"
SCENES_FILENAME = "scenes.csv"
GOLDEN_SCHEMA_VERSION = 1


def load_parity_module() -> Any:
    path = ROOT_DIR / "tests" / "parity" / "run.py"
    spec = importlib.util.spec_from_file_location("parity_run", path)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load parity runner from {path}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


PARITY = load_parity_module()


class OracleError(Exception):
    pass


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


def resolve_path(path: str | Path) -> Path:
    value = Path(path)
    if value.is_absolute():
        return value
    return ROOT_DIR / value


def selected_required_cases(
    config: dict[str, Any], case_id: str | None
) -> list[dict[str, Any]]:
    cases = PARITY.selected_cases(config["cases"], case_id)
    return [case for case in cases if case["status"] == "required"]


def case_golden_path(golden_dir: Path, case: dict[str, Any]) -> Path:
    return golden_dir / f"{case['id']}.json"


def detector_args(case: dict[str, Any], override_key: str) -> list[str]:
    return PARITY.detector_args(case, override_key)


def fixture_hash(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as file:
        for chunk in iter(lambda: file.read(1024 * 1024), b""):
            digest.update(chunk)
    return f"sha256:{digest.hexdigest()}"


def expected_metadata(
    config: dict[str, Any],
    case: dict[str, Any],
    *,
    python_version: str | None = None,
    include_fixture_hash: bool = True,
) -> dict[str, Any]:
    video = resolve_path(case["video"])
    if include_fixture_hash and not video.exists():
        raise OracleError(f"{case['id']}: fixture does not exist: {video}")

    metadata = {
        "schema_version": GOLDEN_SCHEMA_VERSION,
        "oracle_package": config["oracle"]["package"],
        "python": config["oracle"]["python"],
        "case_id": case["id"],
        "detector_command": case["detector"],
        "detector_args": detector_args(case, "oracle_args"),
        "min-scene-len": case["min_scene_len"],
        "fixture_identity": case["video"],
    }
    if include_fixture_hash:
        metadata["fixture_content_hash"] = fixture_hash(video)
    if python_version is not None:
        metadata["python_version"] = python_version
    return metadata


def validate_metadata(
    config: dict[str, Any],
    case: dict[str, Any],
    metadata: Any,
    *,
    require_python_version: bool = True,
    skip_fixture_hash_check: bool = False,
) -> None:
    case_id = case["id"]
    if not isinstance(metadata, dict):
        raise OracleError(f"{case_id}: malformed golden: metadata must be an object")
    if require_python_version and not isinstance(metadata.get("python_version"), str):
        raise OracleError(
            f"{case_id}: stale golden: metadata.python_version is missing"
        )

    expected = expected_metadata(
        config,
        case,
        include_fixture_hash=not skip_fixture_hash_check,
    )
    for key, expected_value in expected.items():
        observed = metadata.get(key)
        if observed != expected_value:
            raise OracleError(
                f"{case_id}: stale golden: metadata.{key} expected "
                f"{expected_value!r}, observed {observed!r}; refresh with "
                "bun run oracle:refresh"
            )


def load_scene_json(path: Path, case_id: str, source: str) -> list[dict[str, int]]:
    try:
        return PARITY.load_scene_json(path, case_id, source)
    except json.JSONDecodeError as error:
        raise OracleError(f"{case_id}: {source} JSON is invalid: {error}") from error
    except AssertionError as error:
        raise OracleError(str(error)) from error


def load_golden(
    config: dict[str, Any],
    case: dict[str, Any],
    golden_dir: Path,
    *,
    skip_metadata_check: bool = False,
    skip_fixture_hash_check: bool = False,
) -> dict[str, Any]:
    case_id = case["id"]
    path = case_golden_path(golden_dir, case)
    if not path.exists():
        raise OracleError(
            f"{case_id}: missing golden: {path}; refresh with bun run oracle:refresh"
        )

    try:
        with path.open() as file:
            golden = json.load(file)
    except json.JSONDecodeError as error:
        raise OracleError(f"{case_id}: malformed golden: invalid JSON: {error}") from error

    if not isinstance(golden, dict):
        raise OracleError(f"{case_id}: malformed golden: root must be an object")

    scenes = golden.get("scenes")
    if not isinstance(scenes, list):
        raise OracleError(f"{case_id}: malformed golden: scenes must be an array")

    if not skip_metadata_check:
        validate_metadata(
            config,
            case,
            golden.get("metadata"),
            skip_fixture_hash_check=skip_fixture_hash_check,
        )

    # Reuse parity scene validation so failures name source, scene, and field.
    tmp_path = path.with_suffix(".validated-scenes.json")
    try:
        tmp_path.write_text(json.dumps(scenes))
        validated_scenes = load_scene_json(tmp_path, case_id, "golden")
    finally:
        tmp_path.unlink(missing_ok=True)

    return {"metadata": golden.get("metadata"), "scenes": validated_scenes}


def compare_scenes(
    case: dict[str, Any],
    reference: list[dict[str, int]],
    observed: list[dict[str, int]],
    *,
    reference_label: str,
    observed_label: str,
) -> None:
    tolerance = case["tolerance_frames"]
    case_id = case["id"]

    if len(reference) != len(observed):
        raise OracleError(
            f"{case_id}: scene count differs: "
            f"{reference_label}={len(reference)} {observed_label}={len(observed)}"
        )

    for index, (ref, obs) in enumerate(zip(reference, observed), start=1):
        for field in ("start", "end"):
            delta = abs(ref[field] - obs[field])
            if delta > tolerance:
                raise OracleError(
                    f"{case_id}: scene {index} {field} differs by {delta} frames: "
                    f"{reference_label}={ref[field]} {observed_label}={obs[field]} "
                    f"tolerance={tolerance}"
                )


def prepare_oracle(config: dict[str, Any]) -> str:
    os.environ.setdefault("PYSCENEDETECT_ORACLE_PYTHON", config["oracle"]["python"])
    os.environ.setdefault("PYSCENEDETECT_ORACLE_PACKAGE", config["oracle"]["package"])
    return capture([str(ROOT_DIR / "scripts" / "setup-python-oracle.sh")])


def prepare_candidate(config: dict[str, Any]) -> Path:
    return PARITY.prepare_candidate(config)


def run_oracle_case(
    config: dict[str, Any],
    case: dict[str, Any],
    uv_bin: str,
    output_dir: Path,
) -> list[dict[str, int]]:
    case_id = case["id"]
    video = resolve_path(case["video"])
    if not video.exists():
        raise OracleError(f"{case_id}: fixture does not exist: {video}")

    case_dir = output_dir / case_id / "oracle"
    shutil.rmtree(case_dir, ignore_errors=True)
    case_dir.mkdir(parents=True)

    run(
        [
            uv_bin,
            "run",
            "--python",
            config["oracle"]["python"],
            "--with",
            config["oracle"]["package"],
            "--",
            "scenedetect",
            "-i",
            str(video),
            case["detector"],
            *detector_args(case, "oracle_args"),
            "--min-scene-len",
            case["min_scene_len"],
            "list-scenes",
            "--output",
            str(case_dir),
            "--filename",
            SCENES_FILENAME,
        ]
    )

    json_path = output_dir / case_id / "oracle.json"
    return PARITY.normalize(
        case_dir / SCENES_FILENAME,
        json_path,
        case_id=case_id,
        source="oracle",
    )


def run_candidate_case(
    case: dict[str, Any],
    candidate_bin: Path,
    output_dir: Path,
) -> list[dict[str, int]]:
    case_id = case["id"]
    video = resolve_path(case["video"])
    if not video.exists():
        raise OracleError(f"{case_id}: fixture does not exist: {video}")

    case_dir = output_dir / case_id / "candidate"
    shutil.rmtree(case_dir, ignore_errors=True)
    case_dir.mkdir(parents=True)

    run(
        [
            str(candidate_bin),
            "-i",
            str(video),
            "-m",
            case["min_scene_len"],
            case["detector"],
            *detector_args(case, "candidate_args"),
            "list-scenes",
            "--output",
            str(case_dir),
            "--filename",
            SCENES_FILENAME,
            "--quiet",
        ]
    )

    json_path = output_dir / case_id / "candidate.json"
    return PARITY.normalize(
        case_dir / SCENES_FILENAME,
        json_path,
        case_id=case_id,
        source="candidate",
    )


def oracle_python_version(config: dict[str, Any], uv_bin: str) -> str:
    return capture(
        [
            uv_bin,
            "run",
            "--python",
            config["oracle"]["python"],
            "--with",
            config["oracle"]["package"],
            "--",
            "python",
            "-c",
            "import sys; print('.'.join(map(str, sys.version_info[:3])))",
        ]
    )


def write_golden(
    config: dict[str, Any],
    case: dict[str, Any],
    scenes: list[dict[str, int]],
    golden_dir: Path,
    python_version: str,
) -> Path:
    golden_dir.mkdir(parents=True, exist_ok=True)
    path = case_golden_path(golden_dir, case)
    metadata = expected_metadata(config, case, python_version=python_version)
    path.write_text(
        json.dumps(
            {"metadata": metadata, "scenes": scenes},
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )
    return path


def ensure_generated_fixtures(config: dict[str, Any]) -> None:
    missing = [
        case["video"]
        for case in config["cases"]
        if case["status"] == "required" and not resolve_path(case["video"]).exists()
    ]
    if missing:
        run([str(ROOT_DIR / "scripts" / "generate-fixtures.sh")])


def command_refresh(args: argparse.Namespace, config: dict[str, Any]) -> int:
    ensure_generated_fixtures(config)
    cases = selected_required_cases(config, args.case_id)
    if not cases:
        print("local oracle refresh: no required cases selected")
        return 0

    uv_bin = prepare_oracle(config)
    python_version = oracle_python_version(config, uv_bin)
    output_dir = resolve_path(args.output_dir)
    golden_dir = resolve_path(args.golden_dir)

    for case in cases:
        scenes = run_oracle_case(config, case, uv_bin, output_dir)
        path = write_golden(config, case, scenes, golden_dir, python_version)
        print(f"local oracle refreshed: {case['id']}: {path}")

    print(f"local oracle refresh ok: {len(cases)} goldens")
    return 0


def command_check(args: argparse.Namespace, config: dict[str, Any]) -> int:
    ensure_generated_fixtures(config)
    cases = selected_required_cases(config, args.case_id)
    if not cases:
        print("local oracle check: no required cases selected")
        return 0

    golden_dir = resolve_path(args.golden_dir)
    output_dir = resolve_path(args.output_dir)
    candidate_bin = prepare_candidate(config)

    for case in cases:
        golden = load_golden(config, case, golden_dir)
        candidate = run_candidate_case(case, candidate_bin, output_dir)
        compare_scenes(
            case,
            golden["scenes"],
            candidate,
            reference_label="golden",
            observed_label="candidate",
        )
        print(f"local oracle check ok: {case['id']}: {len(candidate)} scenes")

    print(f"local oracle candidate check ok: {len(cases)} cases")
    return 0


def command_verify(args: argparse.Namespace, config: dict[str, Any]) -> int:
    ensure_generated_fixtures(config)
    cases = selected_required_cases(config, args.case_id)
    if not cases:
        print("local oracle verify: no required cases selected")
        return 0

    uv_bin = prepare_oracle(config)
    golden_dir = resolve_path(args.golden_dir)
    output_dir = resolve_path(args.output_dir)

    for case in cases:
        golden = load_golden(config, case, golden_dir)
        fresh_oracle = run_oracle_case(config, case, uv_bin, output_dir)
        compare_scenes(
            case,
            golden["scenes"],
            fresh_oracle,
            reference_label="golden",
            observed_label="fresh-oracle",
        )
        print(f"local oracle verify ok: {case['id']}: {len(fresh_oracle)} scenes")

    print(f"local oracle golden verify ok: {len(cases)} cases")
    return 0


def command_check_golden(args: argparse.Namespace, config: dict[str, Any]) -> int:
    case = selected_required_cases(config, args.case_id)[0]
    golden = load_golden(
        config,
        case,
        resolve_path(args.golden_dir),
        skip_metadata_check=args.skip_metadata_check,
        skip_fixture_hash_check=args.skip_fixture_hash_check,
    )
    scenes = load_scene_json(resolve_path(args.scenes_json), case["id"], "observed")
    compare_scenes(
        case,
        golden["scenes"],
        scenes,
        reference_label="golden",
        observed_label="observed",
    )
    print(f"local oracle golden comparison ok: {case['id']}")
    return 0


def add_shared_run_args(parser: argparse.ArgumentParser) -> None:
    parser.add_argument("--case", dest="case_id", help="run one configured case id")
    parser.add_argument(
        "--golden-dir",
        default=str(DEFAULT_GOLDEN_DIR),
        help="directory for ignored local golden JSON files",
    )
    parser.add_argument(
        "--output-dir",
        default=str(DEFAULT_OUTPUT_DIR),
        help="directory for ignored local oracle/candidate outputs",
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Refresh and check ignored local PySceneDetect oracle goldens."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    refresh = subparsers.add_parser(
        "refresh", help="refresh local Reference Oracle goldens"
    )
    add_shared_run_args(refresh)

    check = subparsers.add_parser(
        "check", help="check Candidate outputs against local goldens"
    )
    add_shared_run_args(check)

    verify = subparsers.add_parser(
        "verify", help="verify local goldens against a fresh Reference Oracle run"
    )
    add_shared_run_args(verify)

    check_golden = subparsers.add_parser(
        "check-golden",
        help="compare a normalized scene JSON file to one local golden",
    )
    check_golden.add_argument("--case", dest="case_id", required=True)
    check_golden.add_argument("--golden-dir", required=True)
    check_golden.add_argument("--scenes-json", required=True)
    check_golden.add_argument(
        "--skip-metadata-check",
        action="store_true",
        help="test helper for isolating scene mismatch output",
    )
    check_golden.add_argument(
        "--skip-fixture-hash-check",
        action="store_true",
        help="test helper for isolating non-fixture stale metadata output",
    )

    args = parser.parse_args()

    try:
        config = PARITY.load_config(PARITY.CONFIG_PATH)
        if args.command == "refresh":
            return command_refresh(args, config)
        if args.command == "check":
            return command_check(args, config)
        if args.command == "verify":
            return command_verify(args, config)
        if args.command == "check-golden":
            return command_check_golden(args, config)
    except (PARITY.ConfigError, OracleError, subprocess.CalledProcessError) as error:
        print(f"local oracle failed: {error}", file=sys.stderr)
        return 1

    parser.error(f"unsupported command: {args.command}")
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
