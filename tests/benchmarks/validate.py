#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
from pathlib import Path


ROOT_DIR = Path(__file__).resolve().parents[2]
RUNNER = ROOT_DIR / "tests" / "benchmarks" / "run.py"
REQUIRED_GENERATED_DETECTORS = {
    "detect-content",
    "detect-adaptive",
    "detect-threshold",
    "detect-hist",
    "detect-hash",
}


def load_runner():
    spec = importlib.util.spec_from_file_location("benchmark_run", RUNNER)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load benchmark runner from {RUNNER}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


RUN = load_runner()
config = RUN.load_config()
generated = [case for case in config["cases"] if case["corpus"] == "generated"]
detectors = {case["detector"] for case in generated}
missing = REQUIRED_GENERATED_DETECTORS - detectors
if missing:
    raise SystemExit(
        "benchmark configuration missing generated Detector cases: "
        + ", ".join(sorted(missing))
    )

ids = [case["id"] for case in config["cases"]]
if len(ids) != len(set(ids)):
    raise SystemExit("benchmark configuration contains duplicate case ids")

print(
    "benchmark configuration ok: generated coverage for "
    + ", ".join(sorted(REQUIRED_GENERATED_DETECTORS))
)
