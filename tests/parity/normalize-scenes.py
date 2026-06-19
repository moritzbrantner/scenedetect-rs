#!/usr/bin/env python3
import argparse
import csv
import json
import sys
from pathlib import Path


def normalize_header(value: str) -> str:
    return value.strip().lower().replace(" ", "_").replace("(", "").replace(")", "")


def find_header(lines: list[str]) -> int:
    for index, line in enumerate(lines):
        lowered = line.lower()
        if "scene number" in lowered and "start frame" in lowered:
            return index
    raise SystemExit(f"could not find scene-list header in {sys.argv[1]}")


def pick(row: dict[str, str], *candidates: str) -> str:
    normalized = {normalize_header(key): value for key, value in row.items()}
    for candidate in candidates:
        key = normalize_header(candidate)
        if key in normalized and normalized[key] != "":
            return normalized[key]
    raise KeyError(candidates)


def normalize_scene(scene: dict[str, int], source: str) -> dict[str, int]:
    if source == "oracle":
        return {
            "start": scene["start"] - 1,
            "end": scene["end"],
        }
    if source == "candidate":
        return {
            "start": scene["start"],
            "end": scene["end"],
        }
    raise ValueError(f"unsupported source: {source}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Normalize scene-list CSV frame columns."
    )
    parser.add_argument("--source", choices=("oracle", "candidate"), required=True)
    parser.add_argument("csv_path")
    args = parser.parse_args()

    path = Path(args.csv_path)
    lines = path.read_text().splitlines()
    header = find_header(lines)
    reader = csv.DictReader(lines[header:])
    raw_scenes = []
    for row in reader:
        if not row:
            continue
        try:
            raw_scenes.append(
                {
                    "start": int(pick(row, "Start Frame")),
                    "end": int(pick(row, "End Frame")),
                }
            )
        except (KeyError, ValueError):
            continue
    scenes = [normalize_scene(scene, args.source) for scene in raw_scenes]
    print(json.dumps(scenes, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
