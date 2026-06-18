# Scene Detection

Scene detection identifies boundaries in decoded video and emits scene spans
with enough stats to explain and reproduce those boundaries.

## Language

**Frame Source**:
A producer of decoded video frames and timing metadata.
_Avoid_: Video backend

**Frame**:
One decoded RGB image at a known frame index.

**Timecode**:
A user-facing representation of a frame position or duration.
_Avoid_: Timestamp

**Detector**:
A strategy that scores frames and proposes scene boundaries.
_Avoid_: Algorithm

**Scene Boundary**:
A frame position where one scene ends and another begins.
_Avoid_: Cut

**Scene Span**:
A contiguous range of frames belonging to one scene.
_Avoid_: Segment

**Scene List**:
Ordered scene spans emitted by detection.

**Detection Stats**:
Per-frame metrics used to explain or tune detection.
_Avoid_: Metrics file

**Reference Oracle**:
PySceneDetect output used as the known-good comparison.
_Avoid_: Baseline script

**Candidate**:
The `scenedetect-rs` output being compared to the Reference Oracle.

**Parity Case**:
One Reference Oracle and Candidate comparison over a fixture, Detector options,
and tolerance.

**Capability Matrix**:
Supported and unsupported PySceneDetect behavior tracked by Detector and output
behavior.

**Expected Gap**:
Known PySceneDetect behavior that is documented but not yet required to pass.

**Benchmark Case**:
One report-only timing comparison over equivalent Reference Oracle and Candidate
commands.

**Benchmark Corpus**:
Generated and optional local real-video fixtures used for timing.
