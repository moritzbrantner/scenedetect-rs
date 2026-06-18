# Moonlight PySceneDetect Oracle

Moonlight parity checks compare PySceneDetect as the Reference Oracle with
`scenedetect-rs` as the Candidate on curated tiny videos. The oracle comparison
normalizes scene lists and allows one frame of boundary drift so backend-level
decode differences do not block useful behavior regression checks.
