#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


def parser() -> argparse.ArgumentParser:
    result = argparse.ArgumentParser(
        description="Run the locked PySceneDetect detector API for quality comparison."
    )
    result.add_argument("--input", required=True, type=Path)
    result.add_argument("--output", required=True, type=Path)
    result.add_argument(
        "--detector", required=True, choices=["content", "adaptive", "threshold", "hist", "hash"]
    )
    result.add_argument("--threshold", required=True, type=float)
    result.add_argument("--min-scene-len", required=True)
    result.add_argument("--weights", nargs=4, type=float)
    result.add_argument("--luma-only", action="store_true")
    result.add_argument("--min-content-val", type=float, default=15.0)
    result.add_argument("--frame-window", type=int, default=2)
    result.add_argument("--fade-bias", type=float, default=0.0)
    result.add_argument(
        "--add-last-scene",
        action=argparse.BooleanOptionalAction,
        default=True,
    )
    result.add_argument("--bins", type=int, default=256)
    result.add_argument("--size", type=int, default=16)
    result.add_argument("--lowpass", type=int, default=2)
    return result


def build_detector(args: argparse.Namespace) -> Any:
    from scenedetect.detectors import (
        AdaptiveDetector,
        ContentDetector,
        HashDetector,
        HistogramDetector,
        ThresholdDetector,
    )

    weights = (
        ContentDetector.Components(*args.weights)
        if args.weights is not None
        else ContentDetector.DEFAULT_COMPONENT_WEIGHTS
    )
    common_content = {"weights": weights, "luma_only": args.luma_only}

    if args.detector == "content":
        return ContentDetector(
            threshold=args.threshold,
            min_scene_len=args.min_scene_len,
            **common_content,
        )
    if args.detector == "adaptive":
        return AdaptiveDetector(
            adaptive_threshold=args.threshold,
            min_scene_len=args.min_scene_len,
            window_width=args.frame_window,
            min_content_val=args.min_content_val,
            **common_content,
        )
    if args.detector == "threshold":
        if not -1.0 <= args.fade_bias <= 1.0:
            raise ValueError("fade_bias must be between -1.0 and 1.0")
        return ThresholdDetector(
            threshold=args.threshold,
            min_scene_len=args.min_scene_len,
            fade_bias=args.fade_bias,
            add_final_scene=args.add_last_scene,
        )
    if args.detector == "hist":
        return HistogramDetector(
            threshold=args.threshold,
            bins=args.bins,
            min_scene_len=args.min_scene_len,
        )
    if args.detector == "hash":
        return HashDetector(
            threshold=args.threshold,
            size=args.size,
            lowpass=args.lowpass,
            min_scene_len=args.min_scene_len,
        )
    raise AssertionError(f"unsupported detector: {args.detector}")


def main() -> int:
    args = parser().parse_args()

    from scenedetect import SceneManager, open_video

    video = open_video(str(args.input))
    manager = SceneManager()
    manager.add_detector(build_detector(args))
    manager.detect_scenes(video=video)
    scenes = manager.get_scene_list(start_in_scene=True)

    payload = {
        "frame_rate": float(video.frame_rate),
        "scenes": [
            {"start": start.get_frames(), "end": end.get_frames()} for start, end in scenes
        ],
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
