#!/usr/bin/env python3
from __future__ import annotations

import importlib.util
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("run.py")
SPEC = importlib.util.spec_from_file_location("quality_runner", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
quality = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(quality)


class QualityRunnerTests(unittest.TestCase):
    def test_matches_with_tolerance_and_reports_false_boundaries(self) -> None:
        matches, false_positives, false_negatives = quality.match_boundaries(
            [10, 20, 30], [9, 21, 40], 1
        )
        self.assertEqual(
            [(item["reference_frame"], item["candidate_frame"]) for item in matches],
            [(10, 9), (20, 21)],
        )
        self.assertEqual(false_positives, [40])
        self.assertEqual(false_negatives, [30])

    def test_matching_maximizes_cardinality_before_delta(self) -> None:
        matches, false_positives, false_negatives = quality.match_boundaries(
            [10, 11], [9, 10], 1
        )
        self.assertEqual(
            [(item["reference_frame"], item["candidate_frame"]) for item in matches],
            [(10, 9), (11, 10)],
        )
        self.assertEqual(false_positives, [])
        self.assertEqual(false_negatives, [])

    def test_aggregate_ranks_false_boundaries_before_frame_delta(self) -> None:
        case = {
            "id": "sample",
            "video": "/video.mp4",
            "detector": "content",
            "configuration": {"threshold": 27.0},
            "reproduction_command": "scenedetect-rs detect content -i /video.mp4",
            "reference_boundary_count": 2,
            "candidate_boundary_count": 2,
            "matched_boundary_count": 1,
            "false_positive_count": 1,
            "false_negative_count": 1,
            "false_positives": [{"frame": 40, "timecode": "00:00:04.000"}],
            "false_negatives": [{"frame": 30, "timecode": "00:00:03.000"}],
            "matches": [
                {
                    "reference_frame": 10,
                    "candidate_frame": 11,
                    "reference_timecode": "00:00:01.000",
                    "candidate_timecode": "00:00:01.100",
                    "delta_frames": 1,
                    "absolute_delta_frames": 1,
                }
            ],
        }
        report = quality.aggregate([case], 10)
        self.assertEqual(report["totals"]["false_positives"], 1)
        self.assertEqual(report["totals"]["false_negatives"], 1)
        self.assertIn(
            report["worst_divergences"][0]["kind"], {"false_positive", "false_negative"}
        )
        self.assertEqual(report["worst_divergences"][-1]["kind"], "frame_delta")

    def test_manifest_defaults_and_incremental_selection(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            manifest = Path(directory) / "corpus.toml"
            manifest.write_text(
                """
                [[cases]]
                id = "one"
                video = "video.mp4"
                detector = "content"
                threshold = 27.0

                [[cases]]
                id = "two"
                video = "video.mp4"
                detector = "adaptive"
                threshold = 3.0
                enabled = false
                """
            )
            config = quality.load_config(manifest)
            self.assertEqual(config["oracle"]["package"], "scenedetect-headless==0.7")
            self.assertEqual(config["cases"][0]["tolerance_frames"], 1)
            selected = quality.select_cases(config["cases"], {"one"}, {"content"})
            self.assertEqual([case["id"] for case in selected], ["one"])

    def test_manifest_rejects_case_ids_that_can_escape_work_directory(self) -> None:
        for case_id in ["/tmp/project", "../project", "nested/case", r"nested\case"]:
            with self.subTest(case_id=case_id), tempfile.TemporaryDirectory() as directory:
                manifest = Path(directory) / "corpus.toml"
                manifest.write_text(
                    f"""
                    [[cases]]
                    id = {case_id!r}
                    video = "video.mp4"
                    detector = "content"
                    threshold = 27.0
                    """
                )
                with self.assertRaisesRegex(
                    quality.ConfigError, "single safe path component"
                ):
                    quality.load_config(manifest)


if __name__ == "__main__":
    unittest.main()
