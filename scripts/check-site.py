#!/usr/bin/env python3
from __future__ import annotations

import json
import re
import sys
from pathlib import Path
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[1]
SITE_DIR = ROOT_DIR / "site"
INDEX_PATH = SITE_DIR / "index.html"
BENCHMARK_PATH = SITE_DIR / "data" / "benchmarks.json"
PAGES_WORKFLOW = ROOT_DIR / ".github" / "workflows" / "pages.yml"


class SiteCheckError(Exception):
    pass


def read_text(path: Path) -> str:
    if not path.exists():
        raise SiteCheckError(f"missing required file: {path.relative_to(ROOT_DIR)}")
    return path.read_text(encoding="utf-8")


def require_reference(html: str, reference: str) -> None:
    if reference not in html:
        raise SiteCheckError(f"site/index.html does not reference {reference}")
    if not (SITE_DIR / reference).exists():
        raise SiteCheckError(f"referenced asset is missing: site/{reference}")


def require_number(value: Any, label: str) -> None:
    if not isinstance(value, int | float):
        raise SiteCheckError(f"{label} must be a number")
    if value < 0:
        raise SiteCheckError(f"{label} must be non-negative")


def check_benchmark_snapshot() -> None:
    if not BENCHMARK_PATH.exists():
        raise SiteCheckError("missing required file: site/data/benchmarks.json")
    try:
        data = json.loads(BENCHMARK_PATH.read_text(encoding="utf-8"))
    except json.JSONDecodeError as error:
        raise SiteCheckError(f"invalid benchmark JSON: {error}") from error

    if not isinstance(data, dict):
        raise SiteCheckError("benchmark snapshot must be a JSON object")
    if data.get("schema_version") != 1:
        raise SiteCheckError("benchmark snapshot schema_version must be 1")
    for key in ("generated_at", "candidate_ref", "reference_oracle"):
        if not isinstance(data.get(key), str) or data[key] == "":
            raise SiteCheckError(f"benchmark snapshot must define non-empty {key}")
    source = data.get("source")
    if not isinstance(source, dict):
        raise SiteCheckError("benchmark snapshot must define source object")
    for key in ("command", "machine_label", "notes"):
        if not isinstance(source.get(key), str) or source[key] == "":
            raise SiteCheckError(f"benchmark snapshot source must define non-empty {key}")
    settings = data.get("settings")
    if not isinstance(settings, dict):
        raise SiteCheckError("benchmark snapshot must define settings object")
    if not isinstance(settings.get("warmup"), int) or not isinstance(settings.get("runs"), int):
        raise SiteCheckError("benchmark snapshot settings must define integer warmup and runs")

    cases = data.get("cases")
    if not isinstance(cases, list):
        raise SiteCheckError("benchmark snapshot cases must be an array")
    corpora: set[str] = set()
    for index, case in enumerate(cases, start=1):
        if not isinstance(case, dict):
            raise SiteCheckError(f"benchmark case {index} must be an object")
        for key in ("id", "corpus", "detector", "winner"):
            if not isinstance(case.get(key), str) or case[key] == "":
                raise SiteCheckError(f"benchmark case {index} must define non-empty {key}")
        if case["winner"] not in {"candidate", "reference", "tie"}:
            raise SiteCheckError(f"benchmark case {index} has invalid winner: {case['winner']}")
        for key in ("reference_mean_seconds", "candidate_mean_seconds", "ratio"):
            require_number(case.get(key), f"benchmark case {index} {key}")
        corpora.add(case["corpus"])

    if cases and not {"generated", "real"}.issubset(corpora):
        raise SiteCheckError(
            "benchmark snapshot with cases must include both generated and real corpora"
        )


def check_pages_workflow() -> None:
    workflow = read_text(PAGES_WORKFLOW)
    required = [
        "actions/configure-pages@v5",
        "actions/upload-pages-artifact@v4",
        "actions/deploy-pages@v4",
        "pages: write",
        "id-token: write",
        "path: site",
    ]
    for value in required:
        if value not in workflow:
            raise SiteCheckError(f"pages workflow missing {value}")
    forbidden = ["run-hyperfine.sh", "tests/benchmarks/run.py", "hyperfine"]
    for value in forbidden:
        if value in workflow:
            raise SiteCheckError(f"pages workflow must not run benchmarks: found {value}")


def check_index() -> None:
    html = read_text(INDEX_PATH)
    require_reference(html, "styles.css")
    require_reference(html, "app.js")
    require_reference(html, "data/benchmarks.json")
    if not re.search(r"<main\b", html):
        raise SiteCheckError("site/index.html must contain a main landmark")
    for text in (
        "scenedetect-rs",
        "PySceneDetect",
        "Benchmark",
        "detect-content",
        "detect-adaptive",
        "detect-threshold",
        "detect-hist",
        "detect-hash",
    ):
        if text not in html:
            raise SiteCheckError(f"site/index.html missing expected content: {text}")


def main() -> int:
    try:
        check_index()
        check_benchmark_snapshot()
        check_pages_workflow()
    except SiteCheckError as error:
        print(f"site check failed: {error}", file=sys.stderr)
        return 1
    print("site check ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
