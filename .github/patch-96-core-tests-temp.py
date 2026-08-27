from pathlib import Path

path = Path("crates/scenedetect-core/src/lib.rs")
text = path.read_text()


def patch_test(name: str, replacements: list[tuple[str, str]]) -> None:
    global text
    marker = f"fn {name}()"
    start = text.index(marker)
    next_test = text.find("\n    #[test]", start)
    end = len(text) if next_test == -1 else next_test
    block = text[start:end]
    for old, new in replacements:
        count = block.count(old)
        if count != 1:
            raise SystemExit(f"{name}: expected one {old!r}, found {count}")
        block = block.replace(old, new, 1)
    text = text[:start] + block + text[end:]


patch_test(
    "content_detector_threshold_controls_scene_boundary_sensitivity",
    [
        ("threshold: 20.0,", "threshold: 10.0,"),
        ("threshold: 80.0,", "threshold: 20.0,"),
    ],
)

patch_test(
    "content_boundary_review_classifies_candidates_without_changing_scene_list",
    [
        ("threshold: 100.0,", "threshold: 33.0,"),
        ("review_threshold: Some(50.0),", "review_threshold: Some(16.0),"),
    ],
)

patch_test(
    "content_boundary_review_defaults_to_eighty_percent_of_detector_threshold",
    [
        ("threshold: 100.0,", "threshold: 40.0,"),
        ("assert_eq!(review.review_threshold, 80.0);", "assert_eq!(review.review_threshold, 32.0);"),
    ],
)

patch_test(
    "boundary_review_sorts_candidates_by_distance_to_detector_threshold",
    [("threshold: 100.0,", "threshold: 33.0,")],
)

patch_test(
    "adaptive_boundary_review_keeps_min_content_value_as_noise_floor",
    [
        ("min_content_val: 100.0,", "min_content_val: 34.0,"),
        ("min_content_val: 90.0,", "min_content_val: 30.0,"),
    ],
)

patch_test(
    "content_detector_luma_only_ignores_chroma_only_scene_boundary",
    [
        (
            "let frames = frames(&[[255, 0, 0], [255, 0, 0], [0, 130, 0], [0, 130, 0]]);",
            "let frames = frames(&[[255, 0, 0], [255, 0, 0], [255, 255, 0], [255, 255, 0]]);",
        )
    ],
)

patch_test(
    "adaptive_detector_options_control_scene_boundary_sensitivity",
    [
        ("min_content_val: 90.0,", "min_content_val: 30.0,"),
        ("min_content_val: 90.0,", "min_content_val: 30.0,"),
        ("min_content_val: 101.0,", "min_content_val: 34.0,"),
        ("min_content_val: 90.0,", "min_content_val: 30.0,"),
    ],
)

path.write_text(text)
