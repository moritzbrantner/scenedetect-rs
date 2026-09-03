from pathlib import Path

path = Path("crates/scenedetect-core/src/lib.rs")
text = path.read_text()
old = "while let Some(frame_with_timing) = source.next_frame_with_timing()? {\n        let frame = frame_with_timing.frame;\n"
count = text.count(old)
if count < 5:
    raise SystemExit(f"expected at least five timing-aware detector loops, found {count}")
text = text.replace(old, "while let Some(frame) = source.next_frame()? {\n")
path.write_text(text)
print(f"restored plain-frame detector path in {count} loops")
