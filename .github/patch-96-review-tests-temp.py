from pathlib import Path

path = Path("crates/scenedetect-cli/tests/cli.rs")
text = path.read_text()

for function_name in [
    "content_detector_writes_score_ranked_boundary_candidates_to_csv",
    "content_detector_writes_boundary_candidates_as_json_to_stdout",
    "content_boundary_review_threshold_controls_near_miss_inclusion",
]:
    marker = f"fn {function_name}()"
    start = text.index(marker)
    next_test = text.find("\n#[test]", start)
    end = len(text) if next_test == -1 else next_test
    block = text[start:end]
    block = block.replace('"200"', '"80"')
    block = block.replace('"100"', '"40"')
    text = text[:start] + block + text[end:]

path.write_text(text)
