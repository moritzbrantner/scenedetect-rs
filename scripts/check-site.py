#!/usr/bin/env python3
from __future__ import annotations

import json
import re
import sys
import tomllib
from pathlib import Path
from typing import Any


ROOT_DIR = Path(__file__).resolve().parents[1]
SITE_DIR = ROOT_DIR / "site"
INDEX_PATH = SITE_DIR / "index.html"
CAPABILITIES_PATH = SITE_DIR / "capabilities.html"
BENCHMARKS_PAGE_PATH = SITE_DIR / "benchmarks.html"
QUALITY_PAGE_PATH = SITE_DIR / "quality.html"
WORKBENCH_PATH = SITE_DIR / "workbench.html"
BROWSER_ANALYSIS_PATH = SITE_DIR / "browser-analysis.html"
APP_JS_PATH = SITE_DIR / "app.js"
CAPABILITIES_JS_PATH = SITE_DIR / "capabilities.js"
QUALITY_JS_PATH = SITE_DIR / "quality.js"
WORKBENCH_JS_PATH = SITE_DIR / "workbench.js"
REVIEW_OVERVIEW_PATH = SITE_DIR / "review-overview.js"
REVIEW_WORKSPACE_PATH = SITE_DIR / "review-workspace.js"
VIDEO_FRAME_SYNC_PATH = SITE_DIR / "video-frame-sync.js"
WASM_LOADER_PATH = SITE_DIR / "scenedetect-wasm.js"
ANALYSIS_WORKER_PATH = SITE_DIR / "analysis-worker.js"
ANALYSIS_WORKER_CLIENT_PATH = SITE_DIR / "analysis-worker-client.js"
SESSION_STORE_PATH = SITE_DIR / "session-store.js"
SESSION_STORE_TEST_PATH = SITE_DIR / "session-store.test.js"
SITE_PACKAGE_PATH = SITE_DIR / "package.json"
KEYBOARD_CONTROLS_PATH = SITE_DIR / "keyboard-controls.js"
CAPABILITY_MANIFEST_PATH = SITE_DIR / "data" / "capabilities.json"
BENCHMARK_HISTORY_PATH = SITE_DIR / "data" / "benchmark-history.json"
BENCHMARK_PATH = SITE_DIR / "data" / "benchmarks.json"
QUALITY_PATH = SITE_DIR / "data" / "quality.json"
PARITY_CASES_PATH = ROOT_DIR / "tests" / "parity" / "cases.toml"
PAGES_WORKFLOW = ROOT_DIR / ".github" / "workflows" / "pages.yml"


class SiteCheckError(Exception):
    pass


def read_text(path: Path) -> str:
    if not path.exists():
        raise SiteCheckError(f"missing required file: {path.relative_to(ROOT_DIR)}")
    return path.read_text(encoding="utf-8")


def load_json(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(read_text(path))
    except json.JSONDecodeError as error:
        raise SiteCheckError(f"invalid {label} JSON: {error}") from error
    if not isinstance(value, dict):
        raise SiteCheckError(f"{label} must be a JSON object")
    return value


def require_reference(html: str, reference: str, owner: str) -> None:
    if reference not in html:
        raise SiteCheckError(f"{owner} does not reference {reference}")
    if not (SITE_DIR / reference).exists():
        raise SiteCheckError(f"referenced asset is missing: site/{reference}")


def require_markers(path: Path, markers: tuple[str, ...]) -> None:
    text = read_text(path)
    for marker in markers:
        if marker not in text:
            raise SiteCheckError(
                f"{path.relative_to(ROOT_DIR)} missing browser contract marker: {marker}"
            )


def require_number(value: Any, label: str) -> None:
    if not isinstance(value, int | float):
        raise SiteCheckError(f"{label} must be a number")
    if value < 0:
        raise SiteCheckError(f"{label} must be non-negative")


def check_one_benchmark_snapshot(path: Path) -> dict[str, Any]:
    data = load_json(path, path.name)
    if data.get("schema_version") != 1:
        raise SiteCheckError(f"{path.name} schema_version must be 1")
    for key in ("generated_at", "candidate_ref", "reference_oracle"):
        if not isinstance(data.get(key), str) or data[key] == "":
            raise SiteCheckError(f"{path.name} must define non-empty {key}")
    source = data.get("source")
    if not isinstance(source, dict):
        raise SiteCheckError(f"{path.name} must define source object")
    for key in ("command", "machine_label", "notes"):
        if not isinstance(source.get(key), str) or source[key] == "":
            raise SiteCheckError(f"{path.name} source must define non-empty {key}")
    settings = data.get("settings")
    if not isinstance(settings, dict):
        raise SiteCheckError(f"{path.name} must define settings object")
    if not isinstance(settings.get("warmup"), int) or not isinstance(settings.get("runs"), int):
        raise SiteCheckError(f"{path.name} settings must define integer warmup and runs")

    cases = data.get("cases")
    if not isinstance(cases, list):
        raise SiteCheckError(f"{path.name} cases must be an array")
    for index, case in enumerate(cases, start=1):
        if not isinstance(case, dict):
            raise SiteCheckError(f"{path.name} case {index} must be an object")
        for key in ("id", "corpus", "detector", "winner"):
            if not isinstance(case.get(key), str) or case[key] == "":
                raise SiteCheckError(f"{path.name} case {index} must define non-empty {key}")
        if case["winner"] not in {"candidate", "reference", "tie"}:
            raise SiteCheckError(f"{path.name} case {index} has invalid winner: {case['winner']}")
        for key in ("reference_mean_seconds", "candidate_mean_seconds", "ratio"):
            require_number(case.get(key), f"{path.name} case {index} {key}")
    return data


def check_benchmark_history() -> None:
    history = load_json(BENCHMARK_HISTORY_PATH, "benchmark history")
    if history.get("schema_version") != 1:
        raise SiteCheckError("benchmark history schema_version must be 1")
    snapshots = history.get("snapshots")
    if not isinstance(snapshots, list) or not snapshots:
        raise SiteCheckError("benchmark history must contain snapshots")
    default_snapshot = history.get("default_snapshot")
    if not isinstance(default_snapshot, str) or not default_snapshot:
        raise SiteCheckError("benchmark history must define default_snapshot")

    seen_ids: set[str] = set()
    seen_paths: set[str] = set()
    default_found = False
    for index, entry in enumerate(snapshots, start=1):
        if not isinstance(entry, dict):
            raise SiteCheckError(f"benchmark history entry {index} must be an object")
        for key in ("id", "label", "path", "evidence_class", "description"):
            if not isinstance(entry.get(key), str) or not entry[key]:
                raise SiteCheckError(f"benchmark history entry {index} must define non-empty {key}")
        if entry["id"] in seen_ids:
            raise SiteCheckError(f"duplicate benchmark history id: {entry['id']}")
        if entry["path"] in seen_paths:
            raise SiteCheckError(f"duplicate benchmark history path: {entry['path']}")
        if Path(entry["path"]).name != entry["path"]:
            raise SiteCheckError("benchmark history paths must be files directly under site/data")
        seen_ids.add(entry["id"])
        seen_paths.add(entry["path"])
        default_found = default_found or entry["id"] == default_snapshot

        snapshot_path = SITE_DIR / "data" / entry["path"]
        snapshot = check_one_benchmark_snapshot(snapshot_path)
        if entry["evidence_class"] == "current-generated":
            corpora = {case["corpus"] for case in snapshot["cases"]}
            detectors = {case["detector"] for case in snapshot["cases"]}
            expected = {
                "detect-content",
                "detect-adaptive",
                "detect-threshold",
                "detect-hist",
                "detect-hash",
            }
            if corpora != {"generated"}:
                raise SiteCheckError("current generated benchmark snapshot must contain only generated cases")
            if detectors != expected:
                raise SiteCheckError("current generated benchmark snapshot must cover all five Detector families")

    if not default_found:
        raise SiteCheckError("benchmark history default_snapshot does not exist")

    historical = check_one_benchmark_snapshot(BENCHMARK_PATH)
    historical_corpora = {case["corpus"] for case in historical["cases"]}
    if historical["cases"] and not {"generated", "real"}.issubset(historical_corpora):
        raise SiteCheckError("historical full benchmark snapshot must include generated and real corpora")


def check_capability_manifest() -> None:
    manifest = load_json(CAPABILITY_MANIFEST_PATH, "capability manifest")
    if manifest.get("schema_version") != 1:
        raise SiteCheckError("capability manifest schema_version must be 1")

    try:
        parity = tomllib.loads(read_text(PARITY_CASES_PATH))
    except tomllib.TOMLDecodeError as error:
        raise SiteCheckError(f"invalid parity cases TOML: {error}") from error
    oracle = parity.get("oracle")
    if not isinstance(oracle, dict) or manifest.get("reference_oracle") != oracle.get("package"):
        raise SiteCheckError("capability manifest Reference Oracle must match parity cases")
    if manifest.get("evidence_source") != "tests/parity/cases.toml":
        raise SiteCheckError("capability manifest evidence_source must be tests/parity/cases.toml")

    parity_cases = parity.get("cases")
    if not isinstance(parity_cases, list):
        raise SiteCheckError("parity cases must contain [[cases]] entries")
    required_cases = {
        case["id"]: case
        for case in parity_cases
        if isinstance(case, dict)
        and isinstance(case.get("id"), str)
        and case.get("status") == "required"
    }
    required_detectors = {
        case["detector"]
        for case in required_cases.values()
        if isinstance(case.get("detector"), str)
    }

    detectors = manifest.get("detectors")
    if not isinstance(detectors, list) or not detectors:
        raise SiteCheckError("capability manifest must define detectors")
    supported_commands: set[str] = set()
    seen_ids: set[str] = set()
    for index, detector in enumerate(detectors, start=1):
        if not isinstance(detector, dict):
            raise SiteCheckError(f"capability detector {index} must be an object")
        for key in ("id", "command", "status", "parity", "verified_behavior"):
            if not isinstance(detector.get(key), str) or not detector[key]:
                raise SiteCheckError(f"capability detector {index} must define non-empty {key}")
        if detector["id"] in seen_ids:
            raise SiteCheckError(f"duplicate capability detector id: {detector['id']}")
        seen_ids.add(detector["id"])
        if detector["status"] not in {"supported", "unsupported", "not-claimed"}:
            raise SiteCheckError(f"invalid capability detector status: {detector['status']}")
        case_ids = detector.get("case_ids")
        if not isinstance(case_ids, list) or not all(isinstance(value, str) for value in case_ids):
            raise SiteCheckError(f"capability detector {detector['id']} must define case_ids")
        if detector["status"] == "supported":
            if detector["parity"] != "required" or not case_ids:
                raise SiteCheckError(f"supported detector {detector['id']} needs required parity evidence")
            for case_id in case_ids:
                case = required_cases.get(case_id)
                if case is None:
                    raise SiteCheckError(f"supported detector {detector['id']} cites non-required case {case_id}")
                if case.get("detector") != detector["command"]:
                    raise SiteCheckError(
                        f"capability detector {detector['id']} case {case_id} belongs to {case.get('detector')}"
                    )
            supported_commands.add(detector["command"])

    if supported_commands != required_detectors:
        missing = sorted(required_detectors - supported_commands)
        extra = sorted(supported_commands - required_detectors)
        raise SiteCheckError(
            f"capability support must exactly match required parity Detectors; missing={missing}, extra={extra}"
        )

    surfaces = manifest.get("surfaces")
    if not isinstance(surfaces, list) or not surfaces:
        raise SiteCheckError("capability manifest must define public surfaces")
    for index, surface in enumerate(surfaces, start=1):
        if not isinstance(surface, dict):
            raise SiteCheckError(f"capability surface {index} must be an object")
        for key in ("id", "name", "status", "owner", "guarantee", "evidence_href"):
            if not isinstance(surface.get(key), str) or not surface[key]:
                raise SiteCheckError(f"capability surface {index} must define non-empty {key}")
        if surface["status"] not in {"supported", "unsupported", "not-claimed"}:
            raise SiteCheckError(f"invalid capability surface status: {surface['status']}")


def iter_strings(value: Any):
    if isinstance(value, str):
        yield value
    elif isinstance(value, list):
        for item in value:
            yield from iter_strings(item)
    elif isinstance(value, dict):
        for item in value.values():
            yield from iter_strings(item)


def check_quality_snapshot() -> None:
    snapshot = load_json(QUALITY_PATH, "quality snapshot")
    if snapshot.get("schema_version") != 1:
        raise SiteCheckError("quality snapshot schema_version must be 1")
    for key in ("generated_at", "candidate_ref", "reference_oracle"):
        if not isinstance(snapshot.get(key), str) or not snapshot[key]:
            raise SiteCheckError(f"quality snapshot must define non-empty {key}")
    source = snapshot.get("source")
    if not isinstance(source, dict) or source.get("report_only") is not True:
        raise SiteCheckError("quality snapshot must be explicitly report-only")
    for key in ("command", "manifest", "machine_label"):
        if not isinstance(source.get(key), str) or not source[key]:
            raise SiteCheckError(f"quality snapshot source must define non-empty {key}")
    if source["manifest"] != "tests/quality/corpus.generated.toml":
        raise SiteCheckError("published quality snapshot must come from the generated corpus")

    for value in iter_strings(snapshot):
        if value.startswith("/") or re.match(r"^[A-Za-z]:\\", value):
            raise SiteCheckError(f"quality snapshot contains runner-local absolute path: {value}")

    totals = snapshot.get("totals")
    cases = snapshot.get("cases")
    divergences = snapshot.get("worst_divergences")
    if not isinstance(totals, dict) or not isinstance(cases, list) or not isinstance(divergences, list):
        raise SiteCheckError("quality snapshot must define totals, cases, and worst_divergences")
    total_keys = (
        "cases",
        "reference_boundaries",
        "candidate_boundaries",
        "matched_boundaries",
        "false_positives",
        "false_negatives",
    )
    for key in total_keys:
        require_number(totals.get(key), f"quality totals {key}")
    if totals["cases"] != len(cases):
        raise SiteCheckError("quality totals cases must match the number of case records")

    detector_set: set[str] = set()
    sums = {
        "reference_boundaries": 0,
        "candidate_boundaries": 0,
        "matched_boundaries": 0,
        "false_positives": 0,
        "false_negatives": 0,
    }
    case_ids: set[str] = set()
    for index, case in enumerate(cases, start=1):
        if not isinstance(case, dict):
            raise SiteCheckError(f"quality case {index} must be an object")
        case_id = case.get("id")
        detector = case.get("detector")
        if not isinstance(case_id, str) or not case_id or not isinstance(detector, str) or not detector:
            raise SiteCheckError(f"quality case {index} must define id and detector")
        if case_id in case_ids:
            raise SiteCheckError(f"duplicate quality case id: {case_id}")
        case_ids.add(case_id)
        detector_set.add(detector)
        if case.get("reproduction_command") != f"bun run quality:generated -- --case {case_id}":
            raise SiteCheckError(f"quality case {case_id} must use sanitized reproduction command")
        if "video" in case or "detection_stats" in case:
            raise SiteCheckError(f"quality case {case_id} must not expose local media paths")
        config = case.get("configuration")
        if not isinstance(config, dict) or not isinstance(config.get("tolerance_frames"), int):
            raise SiteCheckError(f"quality case {case_id} must expose tolerance_frames")
        field_map = {
            "reference_boundaries": "reference_boundary_count",
            "candidate_boundaries": "candidate_boundary_count",
            "matched_boundaries": "matched_boundary_count",
            "false_positives": "false_positive_count",
            "false_negatives": "false_negative_count",
        }
        for total_key, case_key in field_map.items():
            require_number(case.get(case_key), f"quality case {case_id} {case_key}")
            sums[total_key] += case[case_key]
        require_number(case.get("max_absolute_delta_frames"), f"quality case {case_id} max delta")
        require_number(case.get("mean_absolute_delta_frames"), f"quality case {case_id} mean delta")

    if detector_set != {"content", "adaptive", "threshold", "hist", "hash"}:
        raise SiteCheckError("published generated quality snapshot must cover all five Detector families")
    for key, value in sums.items():
        if totals[key] != value:
            raise SiteCheckError(f"quality totals {key} does not match case evidence")

    for item in divergences:
        if not isinstance(item, dict) or item.get("case") not in case_ids:
            raise SiteCheckError("quality divergence must reference a published quality case")
        if item.get("reproduction_command") != (
            f"bun run quality:generated -- --case {item['case']}"
        ):
            raise SiteCheckError("quality divergence reproduction command must be sanitized")


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
        "node --check site/app.js",
        "node --check site/capabilities.js",
        "node --check site/quality.js",
        "node --check site/video-frame-sync.js",
        "node --check site/review-overview.js",
        "node --check site/review-workspace.js",
        "node --check site/analysis-worker.js",
        "node --check site/analysis-worker-client.js",
        "node --check site/session-store.js",
        "node --check site/session-store.test.js",
        "node --test site/session-store.test.js",
        "node --check site/keyboard-controls.js",
    ]
    for value in required:
        if value not in workflow:
            raise SiteCheckError(f"pages workflow missing {value}")
    forbidden = [
        "--experimental-default-type",
        "run-hyperfine.sh",
        "tests/benchmarks/run.py",
        "quality:generated",
        "tests/quality/run.py",
        "hyperfine",
    ]
    for value in forbidden:
        if value in workflow:
            raise SiteCheckError(f"pages workflow must not contain {value}")


def check_index() -> None:
    html = read_text(INDEX_PATH)
    for reference in (
        "styles.css",
        "workbench.html",
        "capabilities.html",
        "quality.html",
        "benchmarks.html",
        "browser-analysis.html",
    ):
        require_reference(html, reference, "site/index.html")
    if not re.search(r"<main\b", html):
        raise SiteCheckError("site/index.html must contain a main landmark")
    for text in (
        "scenedetect-rs",
        "Detection Stats",
        "Follow the evidence",
        "Quality",
        "detect-content",
        "detect-adaptive",
        "detect-threshold",
        "detect-hist",
        "detect-hash",
    ):
        if text not in html:
            raise SiteCheckError(f"site/index.html missing expected content: {text}")


def check_capabilities() -> None:
    html = read_text(CAPABILITIES_PATH)
    for reference in (
        "styles.css",
        "evidence.css",
        "capabilities.js",
        "data/capabilities.json",
        "index.html",
        "workbench.html",
        "quality.html",
        "benchmarks.html",
        "browser-analysis.html",
    ):
        require_reference(html, reference, "site/capabilities.html")
    if not re.search(r"<main\b", html):
        raise SiteCheckError("site/capabilities.html must contain a main landmark")
    for text in (
        "Support is a claim only when repository evidence proves it",
        "tests/parity/cases.toml",
        'id="capability-detector-rows"',
        'id="capability-surfaces"',
        "machine-readable capability manifest",
    ):
        if text not in html:
            raise SiteCheckError(f"site/capabilities.html missing expected content: {text}")
    require_markers(
        CAPABILITIES_JS_PATH,
        (
            'fetch("data/capabilities.json", { cache: "no-store" })',
            'const DETECTOR_ROWS_ID = "capability-detector-rows"',
            'const SURFACES_ID = "capability-surfaces"',
            "case_ids",
            "evidence_href",
        ),
    )


def check_benchmarks_page() -> None:
    html = read_text(BENCHMARKS_PAGE_PATH)
    for reference in (
        "styles.css",
        "evidence.css",
        "app.js",
        "data/benchmark-history.json",
        "index.html",
        "capabilities.html",
        "quality.html",
        "workbench.html",
    ):
        require_reference(html, reference, "site/benchmarks.html")
    if not re.search(r"<main\b", html):
        raise SiteCheckError("site/benchmarks.html must contain a main landmark")
    for text in (
        "Every benchmark remains attached to the run that produced it",
        "report-only",
        'id="benchmark-snapshot"',
        'id="benchmark-status"',
        'id="benchmark-meta"',
        'id="benchmark-rows"',
        'id="benchmark-raw-link"',
        "bun run benchmark:real",
        "scripts/update-site-benchmarks.py",
    ):
        if text not in html:
            raise SiteCheckError(f"site/benchmarks.html missing expected content: {text}")
    require_markers(
        APP_JS_PATH,
        (
            'const SELECT_ID = "benchmark-snapshot"',
            'fetchJson("data/benchmark-history.json")',
            "window.history.replaceState",
            "snapshotAgeDays",
            "entry.path",
        ),
    )


def check_quality_page() -> None:
    html = read_text(QUALITY_PAGE_PATH)
    for reference in (
        "styles.css",
        "evidence.css",
        "quality.js",
        "data/quality.json",
        "index.html",
        "capabilities.html",
        "benchmarks.html",
        "workbench.html",
    ):
        require_reference(html, reference, "site/quality.html")
    if not re.search(r"<main\b", html):
        raise SiteCheckError("site/quality.html must contain a main landmark")
    for text in (
        "How closely does the Candidate agree with the Reference Oracle?",
        "report-only",
        'id="quality-status"',
        'id="quality-meta"',
        'id="quality-case-rows"',
        'id="quality-divergences"',
        "bun run quality:generated",
    ):
        if text not in html:
            raise SiteCheckError(f"site/quality.html missing expected content: {text}")
    require_markers(
        QUALITY_JS_PATH,
        (
            'fetch("data/quality.json", { cache: "no-store" })',
            'const QUALITY_ROWS_ID = "quality-case-rows"',
            'const DIVERGENCES_ID = "quality-divergences"',
            "false_positives",
            "false_negatives",
            "worst_divergences",
        ),
    )


def check_browser_analysis() -> None:
    html = read_text(BROWSER_ANALYSIS_PATH)
    require_reference(html, "styles.css", "site/browser-analysis.html")
    require_reference(html, "workbench.css", "site/browser-analysis.html")
    require_reference(html, "workbench.html", "site/browser-analysis.html")
    require_reference(html, "quality.html", "site/browser-analysis.html")
    if not re.search(r"<main\b", html):
        raise SiteCheckError("site/browser-analysis.html must contain a main landmark")
    for text in (
        "How SceneDetect runs locally in the browser",
        "Local media stays local",
        "The browser owns decode and sampling",
        "Rust owns scene-detection semantics",
        "requestVideoFrameCallback()",
        "Human review is separate evidence",
        "does not claim native",
    ):
        if text not in html:
            raise SiteCheckError(f"site/browser-analysis.html missing expected content: {text}")


def check_workbench() -> None:
    html = read_text(WORKBENCH_PATH)
    require_reference(html, "styles.css", "site/workbench.html")
    require_reference(html, "workbench.css", "site/workbench.html")
    require_reference(html, "workbench.js", "site/workbench.html")
    require_reference(html, "browser-analysis.html", "site/workbench.html")
    if not re.search(r"<main\b", html):
        raise SiteCheckError("site/workbench.html must contain a main landmark")
    for text in (
        "Run SceneDetect in your browser",
        "Your video stays local",
        "Content",
        "Adaptive",
        "Threshold / fades",
        "Histogram",
        "Perceptual hash",
        "Scene timeline",
        "Human review layer",
        "Compare runs",
        "Keyboard shortcuts",
        "Detector stats CSV",
        "Ranked boundary candidates",
        "Boundary review CSV",
        "Boundary review JSON",
    ):
        if text not in html:
            raise SiteCheckError(f"site/workbench.html missing expected content: {text}")

    require_markers(SITE_PACKAGE_PATH, ('"type": "module"',))
    require_markers(
        WORKBENCH_JS_PATH,
        (
            'from "./analysis-worker-client.js"',
            'from "./review-overview.js"',
            'from "./review-workspace.js"',
            'from "./session-store.js"',
            'from "./keyboard-controls.js"',
            'from "./video-frame-sync.js"',
            "presentedFrame.mediaTime",
            "analysis.pushFrame",
            "saveWorkbenchSettings",
            "saveRunSnapshot",
            "sessionArtifact",
            "reviewRestorationResult",
            "detector_snapshot",
            "scene_list_csv",
            "boundary_review_json",
            "data-boundary-frame",
        ),
    )
    for detector_name in ("content", "adaptive", "threshold", "histogram", "hash"):
        if detector_name not in read_text(WORKBENCH_JS_PATH):
            raise SiteCheckError(
                f"site/workbench.js missing detector configuration: {detector_name}"
            )

    require_markers(
        REVIEW_WORKSPACE_PATH,
        (
            "presented_samples",
            "reviewArtifact",
            "reviewed_scene_list",
            "addCutAtPlayhead",
            "mergeSelectedWithNext",
            "compareWith",
            "timeline-boundary",
            "detection: output?.detection",
            "boundary_review: output?.boundary_review",
            "presented_samples: presentedSamples",
        ),
    )
    require_markers(
        ANALYSIS_WORKER_CLIENT_PATH,
        ("new Worker", 'type: "module"', "pushFrame", "bytes.buffer", "transfer"),
    )
    require_markers(
        ANALYSIS_WORKER_PATH,
        ('from "./scenedetect-wasm.js"', "createSession", "payload.mediaTimeSeconds", "finish"),
    )
    require_markers(
        SESSION_STORE_PATH,
        (
            "localStorage",
            "history.replaceState",
            "config",
            "saveRunSnapshot",
            "detectorSnapshotsMatch",
            "reviewRestorationResult",
        ),
    )
    require_markers(
        SESSION_STORE_TEST_PATH,
        (
            "createReviewWorkspace",
            "detector snapshots reject changed detection stats",
            "detector snapshots reject changed non-boundary presentation timing",
            "workspace session exports complete detector and presentation identity",
            "public workspace restores decisions only for an exact completed run",
            "public workspace keeps decisions detached when detector stats change",
            "public workspace keeps decisions detached when non-boundary timing changes",
            "detector snapshots fail closed when either snapshot is missing",
        ),
    )
    require_markers(
        KEYBOARD_CONTROLS_PATH,
        ("DEFAULT_KEY_BINDINGS", "previous_boundary", "add_cut", "merge_next", "zoom_in"),
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
        "Before / after split review",
        "seekPresentedVideoFrame",
        "THUMBNAIL_CACHE_LIMIT",
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

    require_markers(
        WASM_LOADER_PATH,
        (
            "wasm/scenedetect_wasm.wasm",
            "SUPPORTED_ABI_VERSION = 2",
            "scenedetect_abi_version",
            "scenedetect_session_new",
            "scenedetect_session_push",
            "mediaTimeSeconds",
            "scenedetect_session_finish",
        ),
    )


def main() -> int:
    try:
        check_index()
        check_capabilities()
        check_quality_page()
        check_benchmarks_page()
        check_workbench()
        check_browser_analysis()
        check_capability_manifest()
        check_quality_snapshot()
        check_benchmark_history()
        check_pages_workflow()
    except SiteCheckError as error:
        print(f"site check failed: {error}", file=sys.stderr)
        return 1
    print("site check ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
