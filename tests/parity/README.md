# PySceneDetect Parity

The parity suite runs PySceneDetect as the Reference Oracle and `scenedetect-rs`
as the Candidate on generated tiny videos. It normalizes scene-list CSV files
into frame ranges and allows up to one frame of boundary drift.

Run:

```sh
tests/parity/run-all.sh
```
