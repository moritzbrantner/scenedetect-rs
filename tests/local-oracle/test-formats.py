#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
from pathlib import Path


ROOT_DIR = Path(__file__).resolve().parents[2]
CHECKER = ROOT_DIR / "tests" / "local-oracle" / "check.py"


def load_checker():
    spec = importlib.util.spec_from_file_location("local_oracle_check", CHECKER)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"could not load local oracle checker from {CHECKER}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


CHECK = load_checker()


json_scenes = CHECK.parse_json_scene_list(
    '{"scene_count":2,"scenes":['
    '{"start_frame":1,"end_frame":10},'
    '{"start_frame":11,"end_frame":20}]}' ,
    "format-test",
)
assert json_scenes == [{"start": 0, "end": 10}, {"start": 10, "end": 20}]

ndjson_scenes = CHECK.parse_ndjson_scene_list(
    '{"event":"scene","start_frame":1,"end_frame":10}\n'
    '{"event":"scene","start_frame":11,"end_frame":20}\n',
    "format-test",
)
assert ndjson_scenes == json_scenes

try:
    CHECK.parse_ndjson_scene_list(
        '{"event":"boundary","start_frame":1,"end_frame":10}\n',
        "format-test",
    )
except CHECK.ORACLE.OracleError as error:
    assert "format-test: candidate-ndjson line 1 field event" in str(error)
else:
    raise AssertionError("expected invalid NDJSON event to fail")

try:
    CHECK.parse_json_scene_list(
        '{"scene_count":2,"scenes":[{"start_frame":1,"end_frame":10}]}',
        "format-test",
    )
except CHECK.ORACLE.OracleError as error:
    assert "scene_count=2" in str(error)
else:
    raise AssertionError("expected mismatched JSON scene_count to fail")

print("local oracle format normalization ok")
