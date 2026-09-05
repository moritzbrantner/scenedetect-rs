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
WORKBENCH_PATH = SITE_DIR / "workbench.html"
WORKBENCH_ENTRY_CSS_PATH = SITE_DIR / "workbench-entry.css"
HOW_IT_WORKS_PATH = SITE_DIR / "how-it-works.html"
WORKBENCH_JS_PATH = SITE_DIR / "workbench.js"
REVIEW_OVERVIEW_PATH = SITE_DIR / "review-overview.js"
VIDEO_FRAME_SYNC_PATH = SITE_DIR / "video-frame-sync.js"
WASM_LOADER_PATH = SITE_DIR / "scenedetect-wasm.js"
BENCHMARK_PATH = SITE_DIR / "data" / "benchmarks.json"
PAGES_WORKFLOW = ROOT_DIR / ".github" / "workflows" / "pages.yml"


class SiteCheckError(Exception):
    pass


def read_text(path: Path) -> str:
    if not path.exists():
        raise SiteCheckError(f"missing required file: {path.relative_to(ROOT_DIR)}")
    return path.read_text(encoding="utf-8")


def require_reference(html: str, reference: str, owner: str) -> None:
    if reference not in html:
        raise SiteCheckError(f"{owner} does not reference {reference}")
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
        "actions/upload-pages-artifact@v5",
        "actions/deploy-pages@v4",
        "pages: write",
        "id-token: write",
        "path: site",
        "rustup target add wasm32-unknown-unknown",
        "cargo build --locked -p scenedetect-wasm --target wasm32-unknown-unknown --release",
        "site/wasm/scenedetect_wasm.wasm",
        "node --check site/video-frame-sync.js",
        "node --check site/review-overview.js",
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
    require_reference(html, "styles.css", "site/index.html")
    require_reference(html, "app.js", "site/index.html")
    require_reference(html, "data/benchmarks.json", "site/index.html")
    require_reference(html, "workbench.html", "site/index.html")
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


def check_workbench() -> None:
    html = read_text(WORKBENCH_PATH)
    require_reference(html, "styles.css", "site/workbench.html")
    require_reference(html, "workbench.css", "site/workbench.html")
    require_reference(html, "workbench-entry.css", "site/workbench.html")
    require_reference(html, "how-it-works.html", "site/workbench.html")
    require_reference(html, "workbench.js", "site/workbench.html")
    if not re.search(r"<main\b", html):
        raise SiteCheckError("site/workbench.html must contain a main landmark")
    if "workbench-hero" in html:
        raise SiteCheckError("site/workbench.html must keep the workflow above explanatory hero content")
    for text in (
        "Choose a video",
        "Run SceneDetect in your browser",
        "Your video stays local",
        "Browser decode",
        "Content",
        "Adaptive",
        "Threshold / fades",
        "Histogram",
        "Perceptual hash",
        "Detector stats CSV",
        "Ranked boundary candidates",
        "Boundary review CSV",
        "Boundary review JSON",
    ):
        if text not in html:
            raise SiteCheckError(f"site/workbench.html missing expected content: {text}")

    entry_css = read_text(WORKBENCH_ENTRY_CSS_PATH)
    for value in (".input-layout", ".sampling-panel", "#video-preview:not([src])"):
        if value not in entry_css:
            raise SiteCheckError(f"site/workbench-entry.css missing entry layout marker: {value}")

    workbench_js = read_text(WORKBENCH_JS_PATH)
    for value in (
        'from "./review-overview.js"',
        'from "./scenedetect-wasm.js"',
        'from "./video-frame-sync.js"',
        "createResultsOverview",
        "seekPresentedVideoFrame",
        "presentedFrame.mediaTime",
        "createSession",
        "pushFrame",
        "scene_list_csv",
        "scene_list_json",
        "scene_events_ndjson",
        "stats_csv",
        "scene_list_html",
        "review_threshold",
        "boundary_review_csv",
        "boundary_review_json",
        "data-boundary-frame",
    ):
        if value not in workbench_js:
            raise SiteCheckError(f"site/workbench.js missing browser contract marker: {value}")
    for detector_name in ("content", "adaptive", "threshold", "histogram", "hash"):
        if detector_name not in workbench_js:
            raise SiteCheckError(
                f"site/workbench.js missing detector configuration: {detector_name}"
            )

    review_overview = read_text(REVIEW_OVERVIEW_PATH)
    for value in (
        'from "./video-frame-sync.js"',
        "candidateReviewGrouping",
        '"edge_case"',
        '"obvious"',
        '"accepted"',
        '"suppressed_min_scene_len"',
        '"near_miss"',
        "boundary-status-filter",
        "boundary-review-filter",
        "boundary-sort",
        "scene-search-filter",
        "scene-sort",
        "Before / after split review",
        "seekPresentedVideoFrame",
        "THUMBNAIL_CACHE_LIMIT",
        "VISUAL_PAGE_SIZE",
    ):
        if value not in review_overview:
            raise SiteCheckError(
                f"site/review-overview.js missing review browser contract marker: {value}"
            )

    frame_sync = read_text(VIDEO_FRAME_SYNC_PATH)
    for value in (
        "requestVideoFrameCallback",
        "cancelVideoFrameCallback",
        "metadata.mediaTime",
        'synchronization: "video-frame-callback"',
        'synchronization: "animation-frame-fallback"',
    ):
        if value not in frame_sync:
            raise SiteCheckError(
                f"site/video-frame-sync.js missing presentation contract marker: {value}"
            )

    wasm_loader = read_text(WASM_LOADER_PATH)
    for value in (
        "wasm/scenedetect_wasm.wasm",
        "scenedetect_abi_version",
        "scenedetect_session_new",
        "scenedetect_session_push",
        "scenedetect_session_finish",
    ):
        if value not in wasm_loader:
            raise SiteCheckError(f"site/scenedetect-wasm.js missing WASM contract marker: {value}")


def check_how_it_works() -> None:
    html = read_text(HOW_IT_WORKS_PATH)
    require_reference(html, "styles.css", "site/how-it-works.html")
    require_reference(html, "workbench.css", "site/how-it-works.html")
    require_reference(html, "workbench-entry.css", "site/how-it-works.html")
    require_reference(html, "workbench.html", "site/how-it-works.html")
    if not re.search(r"<main\b", html):
        raise SiteCheckError("site/how-it-works.html must contain a main landmark")
    for text in (
        "How local SceneDetect analysis works",
        "Your video stays on this device",
        "Rust owns detection semantics",
        "requestVideoFrameCallback",
        "native FFmpeg",
    ):
        if text not in html:
            raise SiteCheckError(f"site/how-it-works.html missing expected content: {text}")


def main() -> int:
    try:
        check_index()
        check_workbench()
        check_how_it_works()
        check_benchmark_snapshot()
        check_pages_workflow()
    except SiteCheckError as error:
        print(f"site check failed: {error}", file=sys.stderr)
        return 1
    print("site check ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
