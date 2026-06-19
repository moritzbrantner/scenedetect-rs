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

**Histogram Detector**:
A Detector that compares adjacent Frame luma histograms to propose Scene
Boundaries.
_Avoid_: Hist algorithm

**Perceptual Hash Detector**:
A Detector that compares adjacent Frame perceptual hashes to propose Scene
Boundaries.
_Avoid_: Hash algorithm

**Histogram Correlation Score**:
A Detection Stats value describing adjacent luma histogram similarity.
_Avoid_: Histogram distance

**Hash Distance**:
A Detection Stats value describing normalized adjacent perceptual hash
difference.
_Avoid_: Raw hamming distance

**Scene Boundary**:
A frame position where one scene ends and another begins.
_Avoid_: Cut

**Boundary Candidate**:
A frame position considered during review as a possible Scene Boundary,
including accepted, suppressed, and near-threshold positions.
_Avoid_: Candidate, split

**Boundary Score**:
A Detector-specific numeric value used to rank Boundary Candidates for review.
_Avoid_: Score

**Scene Span**:
A contiguous range of frames belonging to one scene.
_Avoid_: Segment

**Scene List**:
Ordered scene spans emitted by detection.

**Scene List Artifact**:
Canonical reusable representation of a Scene List plus detection provenance.
_Avoid_: Cache file

**Reusable Output**:
A rendered output file whose manifest proves it was produced from a matching
Scene List Artifact.
_Avoid_: Existing file

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

**Published Benchmark Snapshot**:
A committed point-in-time Benchmark Report derived from local Benchmark Cases
and published on the project site.
