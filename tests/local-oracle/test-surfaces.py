#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
from pathlib import Path


ROOT_DIR = Path(__file__).resolve().parents[2]
SURFACE_CHECKER = ROOT_DIR / "tests" / "local-oracle" / "check-surfaces.py"


def load_checker():
    spec = importlib.util.spec_from_file_location("local_oracle_surfaces", SURFACE_CHECKER)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load local oracle surface checker from {SURFACE_CHECKER}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


CHECK = load_checker()

json_boundaries = CHECK.accepted_boundaries_from_json(
    '{"boundary_candidates":['
    '{"status":"accepted","boundary_frame_index":10},'
    '{"status":"near_miss","boundary_frame_index":12},'
    '{"status":"accepted","boundary_frame_index":20}]}' ,
    "surface-test",
    "boundary-json",
)
assert json_boundaries == [10, 20]

csv_boundaries = CHECK.accepted_boundaries_from_csv(
    "Status,Boundary Frame Index\naccepted,10\nnear_miss,12\naccepted,20\n",
    "surface-test",
    "boundary-csv",
)
assert csv_boundaries == json_boundaries

html = "<!doctype html><html><table><tr><th>Scene</th></tr><tr><td>1</td></tr><tr><td>2</td></tr></table></html>"
assert CHECK.html_scene_count(html, "surface-test", "html") == 2

case = {"id": "surface-test", "tolerance_frames": 1}
CHECK.compare_boundaries(case, [10, 20], [11, 19], "boundary-test")

try:
    CHECK.compare_boundaries(case, [10], [12], "boundary-test")
except CHECK.ORACLE.OracleError as error:
    assert "surface-test: boundary-test: boundary 1 differs" in str(error)
else:
    raise AssertionError("expected out-of-tolerance boundary to fail")

print("local oracle surface validation ok")
