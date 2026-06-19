#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import subprocess
import tempfile
import tomllib
from datetime import UTC, datetime
from pathlib import Path
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[1]
DEFAULT_HYPERFINE_JSON = ROOT_DIR / "tests" / "benchmarks" / "results" / "cli.json"
DEFAULT_BENCHMARK_CONFIG = ROOT_DIR / "tests" / "benchmarks" / "cases.toml"
DEFAULT_SITE_SNAPSHOT = ROOT_DIR / "site" / "data" / "benchmarks.json"
DEFAULT_COMMAND = "tests/benchmarks/run-hyperfine.sh --include-real"
DEFAULT_NOTES = "Real-video Benchmark Corpus is optional and media is not committed."


class SnapshotError(Exception):
    pass


def load_json(path: Path) -> dict[str, Any]:
    if not path.exists():
        raise SnapshotError(
            f"benchmark results not found: {path}\n"
            f"Run {DEFAULT_COMMAND} locally before updating the Published Benchmark Snapshot."
        )
    try:
        with path.open(encoding="utf-8") as file:
            data = json.load(file)
    except json.JSONDecodeError as error:
        raise SnapshotError(f"invalid JSON in {path}: {error}") from error
    if not isinstance(data, dict):
        raise SnapshotError(f"expected top-level JSON object in {path}")
    return data


def load_toml(path: Path) -> dict[str, Any]:
    if not path.exists():
        raise SnapshotError(f"benchmark config not found: {path}")
    try:
        with path.open("rb") as file:
            data = tomllib.load(file)
    except tomllib.TOMLDecodeError as error:
        raise SnapshotError(f"invalid TOML in {path}: {error}") from error
    if not isinstance(data, dict):
        raise SnapshotError(f"expected top-level TOML table in {path}")
    return data


def git_candidate_ref() -> str:
    try:
        revision = subprocess.run(
            ["git", "rev-parse", "--short", "HEAD"],
            cwd=ROOT_DIR,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
        ).stdout.strip()
        dirty = subprocess.run(
            ["git", "diff", "--quiet"],
            cwd=ROOT_DIR,
            check=False,
            stderr=subprocess.DEVNULL,
        ).returncode != 0
    except (OSError, subprocess.CalledProcessError):
        return "unknown"
    return f"{revision}-dirty" if dirty else revision


def require_number(value: Any, label: str) -> float:
    if not isinstance(value, int | float):
        raise SnapshotError(f"{label} must be a number")
    if value < 0:
        raise SnapshotError(f"{label} must be non-negative")
    return float(value)


def command_label(result: dict[str, Any], index: int) -> str:
    value = result.get("command_name", result.get("command"))
    if not isinstance(value, str) or value == "":
        raise SnapshotError(f"hyperfine result {index} is missing a command label")
    return value


def parse_hyperfine_pairs(data: dict[str, Any]) -> dict[str, dict[str, float]]:
    results = data.get("results")
    if not isinstance(results, list) or not results:
        raise SnapshotError("hyperfine JSON must contain a non-empty results array")

    pairs: dict[str, dict[str, float]] = {}
    for index, result in enumerate(results, start=1):
        if not isinstance(result, dict):
            raise SnapshotError(f"hyperfine result {index} must be an object")
        label = command_label(result, index)
        try:
            case_id, kind = label.rsplit(" ", 1)
        except ValueError as error:
            raise SnapshotError(
                f"hyperfine result label must end in ' reference' or ' candidate': {label}"
            ) from error
        if kind not in {"reference", "candidate"}:
            raise SnapshotError(
                f"hyperfine result label must end in ' reference' or ' candidate': {label}"
            )
        pairs.setdefault(case_id, {})
        if kind in pairs[case_id]:
            raise SnapshotError(f"duplicate {kind} result for benchmark case: {case_id}")
        pairs[case_id][kind] = require_number(result.get("mean"), f"{label} mean")

    missing = [
        f"{case_id} {kind}"
        for case_id, case_pairs in sorted(pairs.items())
        for kind in ("reference", "candidate")
        if kind not in case_pairs
    ]
    if missing:
        raise SnapshotError(f"missing paired hyperfine result(s): {', '.join(missing)}")
    return pairs


def case_metadata(config: dict[str, Any]) -> dict[str, dict[str, str]]:
    cases = config.get("cases")
    if not isinstance(cases, list):
        raise SnapshotError("benchmark config must contain [[cases]] entries")

    metadata: dict[str, dict[str, str]] = {}
    for index, case in enumerate(cases, start=1):
        if not isinstance(case, dict):
            raise SnapshotError(f"benchmark config case {index} must be a table")
        case_id = case.get("id")
        corpus = case.get("corpus")
        detector = case.get("detector")
        if not all(isinstance(value, str) and value for value in (case_id, corpus, detector)):
            raise SnapshotError(f"benchmark config case {index} must define id, corpus, detector")
        metadata[case_id] = {
            "corpus": corpus,
            "detector": detector,
        }
    return metadata


def snapshot_settings(config: dict[str, Any]) -> dict[str, int]:
    settings = config.get("settings")
    if not isinstance(settings, dict):
        raise SnapshotError("benchmark config must contain [settings]")
    warmup = settings.get("warmup")
    runs = settings.get("runs")
    if not isinstance(warmup, int) or not isinstance(runs, int):
        raise SnapshotError("benchmark config settings must define integer warmup and runs")
    return {"warmup": warmup, "runs": runs}


def reference_oracle(config: dict[str, Any]) -> str:
    oracle = config.get("oracle")
    if not isinstance(oracle, dict) or not isinstance(oracle.get("package"), str):
        raise SnapshotError("benchmark config must define [oracle] package")
    return oracle["package"]


def compare(reference_mean: float, candidate_mean: float) -> tuple[float, str]:
    if candidate_mean == 0:
        ratio = 0.0 if reference_mean == 0 else float("inf")
    else:
        ratio = reference_mean / candidate_mean
    if candidate_mean < reference_mean:
        winner = "candidate"
    elif reference_mean < candidate_mean:
        winner = "reference"
    else:
        winner = "tie"
    return ratio, winner


def build_snapshot(
    hyperfine_data: dict[str, Any],
    config: dict[str, Any],
    *,
    generated_at: str,
    candidate_ref: str,
    command: str,
    machine_label: str,
    notes: str,
) -> dict[str, Any]:
    pairs = parse_hyperfine_pairs(hyperfine_data)
    metadata = case_metadata(config)

    cases = []
    for case_id in sorted(pairs):
        if case_id not in metadata:
            raise SnapshotError(f"hyperfine result has no matching benchmark config case: {case_id}")
        reference_mean = pairs[case_id]["reference"]
        candidate_mean = pairs[case_id]["candidate"]
        ratio, winner = compare(reference_mean, candidate_mean)
        cases.append(
            {
                "id": case_id,
                "corpus": metadata[case_id]["corpus"],
                "detector": metadata[case_id]["detector"],
                "reference_mean_seconds": reference_mean,
                "candidate_mean_seconds": candidate_mean,
                "ratio": ratio,
                "winner": winner,
            }
        )

    return {
        "schema_version": 1,
        "generated_at": generated_at,
        "candidate_ref": candidate_ref,
        "reference_oracle": reference_oracle(config),
        "source": {
            "command": command,
            "machine_label": machine_label,
            "notes": notes,
        },
        "settings": snapshot_settings(config),
        "cases": cases,
    }


def write_snapshot(snapshot: dict[str, Any], path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8") as file:
        json.dump(snapshot, file, indent=2, sort_keys=False, allow_nan=False)
        file.write("\n")


def run_fixture_check() -> None:
    with tempfile.TemporaryDirectory() as directory:
        root = Path(directory)
        config_path = root / "cases.toml"
        config_path.write_text(
            """
[oracle]
package = "scenedetect-headless==0.7"

[settings]
warmup = 1
runs = 3

[[cases]]
id = "generated-content-hard-cuts"
corpus = "generated"
video = "fixture.mkv"
detector = "detect-content"
args = []
min_scene_len = "1"
""".lstrip(),
            encoding="utf-8",
        )
        config = load_toml(config_path)
        valid = {
            "results": [
                {"command": "generated-content-hard-cuts reference", "mean": 2.0},
                {"command": "generated-content-hard-cuts candidate", "mean": 0.5},
            ]
        }
        snapshot = build_snapshot(
            valid,
            config,
            generated_at="2026-06-19T00:00:00Z",
            candidate_ref="fixture-ref",
            command=DEFAULT_COMMAND,
            machine_label="fixture",
            notes=DEFAULT_NOTES,
        )
        case = snapshot["cases"][0]
        assert case["ratio"] == 4.0
        assert case["winner"] == "candidate"
        assert case["corpus"] == "generated"
        assert case["detector"] == "detect-content"

        missing_pair = {"results": [{"command": "generated-content-hard-cuts reference", "mean": 2.0}]}
        try:
            build_snapshot(
                missing_pair,
                config,
                generated_at="2026-06-19T00:00:00Z",
                candidate_ref="fixture-ref",
                command=DEFAULT_COMMAND,
                machine_label="fixture",
                notes=DEFAULT_NOTES,
            )
        except SnapshotError as error:
            assert "missing paired hyperfine result" in str(error)
        else:
            raise AssertionError("missing candidate pair should fail")


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Update the site Published Benchmark Snapshot from hyperfine results."
    )
    parser.add_argument("--input", type=Path, default=DEFAULT_HYPERFINE_JSON)
    parser.add_argument("--config", type=Path, default=DEFAULT_BENCHMARK_CONFIG)
    parser.add_argument("--output", type=Path, default=DEFAULT_SITE_SNAPSHOT)
    parser.add_argument("--machine-label", default="local")
    parser.add_argument("--candidate-ref", default=None)
    parser.add_argument("--generated-at", default=None)
    parser.add_argument("--command", default=DEFAULT_COMMAND)
    parser.add_argument("--notes", default=DEFAULT_NOTES)
    parser.add_argument(
        "--check-fixture",
        action="store_true",
        help="run converter behavior checks without reading local benchmark results",
    )
    args = parser.parse_args()

    try:
        if args.check_fixture:
            run_fixture_check()
            print("benchmark snapshot fixture check ok")
            return 0

        snapshot = build_snapshot(
            load_json(args.input),
            load_toml(args.config),
            generated_at=args.generated_at or datetime.now(UTC).replace(microsecond=0).isoformat(),
            candidate_ref=args.candidate_ref or git_candidate_ref(),
            command=args.command,
            machine_label=args.machine_label,
            notes=args.notes,
        )
        write_snapshot(snapshot, args.output)
        print(f"wrote Published Benchmark Snapshot: {args.output}")
        return 0
    except (AssertionError, SnapshotError) as error:
        print(f"benchmark snapshot error: {error}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
