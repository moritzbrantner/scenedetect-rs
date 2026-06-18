#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import shlex
import subprocess
import sys
import tomllib
from pathlib import Path
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[2]
BENCH_DIR = ROOT_DIR / "tests" / "benchmarks"
CONFIG_PATH = BENCH_DIR / "cases.toml"
RESULTS_DIR = BENCH_DIR / "results"
OUTPUT_DIR = RESULTS_DIR / "output"
SCENES_FILENAME = "scenes.csv"


class ConfigError(Exception):
    pass


def load_config() -> dict[str, Any]:
    try:
        with CONFIG_PATH.open("rb") as file:
            config = tomllib.load(file)
    except tomllib.TOMLDecodeError as error:
        raise ConfigError(f"invalid TOML in {CONFIG_PATH}: {error}") from error

    oracle = config.get("oracle")
    if not isinstance(oracle, dict):
        raise ConfigError("missing [oracle] table")
    require_string(oracle, "python", "[oracle]")
    require_string(oracle, "package", "[oracle]")

    settings = config.get("settings", {})
    if not isinstance(settings, dict):
        raise ConfigError("[settings] must be a table")
    for key in ("warmup", "runs"):
        value = settings.get(key)
        if not isinstance(value, int) or value < 1:
            raise ConfigError(f"[settings] {key} must be a positive integer")

    cases = config.get("cases")
    if not isinstance(cases, list) or not cases:
        raise ConfigError("missing [[cases]] entries")
    seen_ids: set[str] = set()
    for index, case in enumerate(cases, start=1):
        if not isinstance(case, dict):
            raise ConfigError(f"case {index} must be a table")
        validate_case(case, index)
        if case["id"] in seen_ids:
            raise ConfigError(f"duplicate case id: {case['id']}")
        seen_ids.add(case["id"])
    return config


def validate_case(case: dict[str, Any], index: int) -> None:
    label = f"case {index}"
    for key in ("id", "corpus", "video", "detector", "min_scene_len"):
        require_string(case, key, label)
    if case["corpus"] not in {"generated", "real"}:
        raise ConfigError(f"{label} corpus must be generated or real")
    args = case.get("args")
    if not isinstance(args, list) or not all(isinstance(value, str) for value in args):
        raise ConfigError(f"{label} must define args as a string array")


def require_string(table: dict[str, Any], key: str, label: str) -> None:
    if not isinstance(table.get(key), str) or table[key] == "":
        raise ConfigError(f"{label} must define non-empty {key}")


def resolve_path(path: str) -> Path:
    value = Path(path)
    if value.is_absolute():
        return value
    return ROOT_DIR / value


def quote_command(parts: list[str]) -> str:
    return " ".join(shlex.quote(part) for part in parts)


def build_commands(
    cases: list[dict[str, Any]],
    config: dict[str, Any],
    uv_bin: Path,
    candidate_bin: Path,
) -> list[tuple[str, str]]:
    commands = []
    oracle = config["oracle"]

    for case in cases:
        video = resolve_path(case["video"])
        if not video.exists():
            print(f"skipping benchmark case with missing video: {case['id']}: {video}")
            continue

        reference_dir = OUTPUT_DIR / case["id"] / "reference"
        candidate_dir = OUTPUT_DIR / case["id"] / "candidate"
        reference_dir.mkdir(parents=True, exist_ok=True)
        candidate_dir.mkdir(parents=True, exist_ok=True)

        reference = [
            str(uv_bin),
            "run",
            "--python",
            oracle["python"],
            "--with",
            oracle["package"],
            "--",
            "scenedetect",
            "-i",
            str(video),
            case["detector"],
            *case["args"],
            "--min-scene-len",
            case["min_scene_len"],
            "list-scenes",
            "--output",
            str(reference_dir),
            "--filename",
            SCENES_FILENAME,
        ]
        candidate = [
            str(candidate_bin),
            "-i",
            str(video),
            "-m",
            case["min_scene_len"],
            case["detector"],
            *case["args"],
            "list-scenes",
            "--output",
            str(candidate_dir),
            "--filename",
            SCENES_FILENAME,
            "--quiet",
        ]
        commands.append((f"{case['id']} reference", quote_command(reference)))
        commands.append((f"{case['id']} candidate", quote_command(candidate)))

    return commands


def main() -> int:
    parser = argparse.ArgumentParser(description="Run CLI benchmarks through hyperfine.")
    parser.add_argument(
        "--include-real",
        action="store_true",
        help="include real-video benchmark cases whose clips exist",
    )
    args = parser.parse_args()

    try:
        config = load_config()
    except ConfigError as error:
        print(f"config error: {error}", file=sys.stderr)
        return 2

    RESULTS_DIR.mkdir(parents=True, exist_ok=True)
    OUTPUT_DIR.mkdir(parents=True, exist_ok=True)

    selected = [
        case
        for case in config["cases"]
        if args.include_real or case["corpus"] == "generated"
    ]
    if not selected:
        print("no benchmark cases selected", file=sys.stderr)
        return 1

    uv_bin = Path(os.environ.get("UV_BIN", str(ROOT_DIR / ".tools" / "uv" / "uv")))
    if not uv_bin.exists():
        uv_bin = Path(
            subprocess.run(
                [str(ROOT_DIR / "scripts" / "setup-python-oracle.sh")],
                cwd=ROOT_DIR,
                check=True,
                text=True,
                stdout=subprocess.PIPE,
            ).stdout.strip()
        )

    candidate_bin = Path(
        os.environ.get("CANDIDATE_BIN", str(ROOT_DIR / "target" / "release" / "scenedetect-rs"))
    )

    commands = build_commands(selected, config, uv_bin, candidate_bin)
    if not commands:
        print("no benchmark commands had existing videos", file=sys.stderr)
        return 1

    hyperfine = ["hyperfine"]
    settings = config["settings"]
    hyperfine.extend(["--warmup", str(settings["warmup"])])
    hyperfine.extend(["--runs", str(settings["runs"])])
    hyperfine.extend(["--export-json", str(RESULTS_DIR / "cli.json")])
    hyperfine.extend(["--export-markdown", str(RESULTS_DIR / "cli.md")])

    for name, command in commands:
        hyperfine.extend(["--command-name", name, command])

    subprocess.run(hyperfine, cwd=ROOT_DIR, check=True)
    print(f"benchmark results: {RESULTS_DIR / 'cli.md'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
