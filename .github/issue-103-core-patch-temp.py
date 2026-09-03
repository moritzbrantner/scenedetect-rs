from pathlib import Path

path = Path("crates/scenedetect-core/src/lib.rs")
text = path.read_text()

old_trait = '''pub trait FrameSource {
    fn frame_rate(&self) -> FrameRate;
    fn next_frame(&mut self) -> Result<Option<Frame>>;
}
'''
new_trait = '''#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeBase {
    pub numerator: i64,
    pub denominator: i64,
}

impl TimeBase {
    pub fn new(numerator: i64, denominator: i64) -> Option<Self> {
        if numerator == 0 || denominator == 0 {
            None
        } else {
            Some(Self {
                numerator,
                denominator,
            })
        }
    }

    pub fn seconds_per_tick(self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaTime {
    pub ticks: i64,
    pub time_base: TimeBase,
}

impl MediaTime {
    pub fn new(ticks: i64, time_base: TimeBase) -> Self {
        Self { ticks, time_base }
    }

    pub fn seconds(self) -> f64 {
        self.ticks as f64 * self.time_base.seconds_per_tick()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameTiming {
    pub presentation_time: Option<MediaTime>,
    pub duration: Option<MediaTime>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrameWithTiming {
    pub frame: Frame,
    pub timing: FrameTiming,
}

impl From<Frame> for FrameWithTiming {
    fn from(frame: Frame) -> Self {
        Self {
            frame,
            timing: FrameTiming::default(),
        }
    }
}

pub trait FrameSource {
    fn frame_rate(&self) -> FrameRate;
    fn next_frame(&mut self) -> Result<Option<Frame>>;

    fn next_frame_with_timing(&mut self) -> Result<Option<FrameWithTiming>> {
        Ok(self.next_frame()?.map(FrameWithTiming::from))
    }
}
'''
if text.count(old_trait) != 1:
    raise SystemExit("FrameSource trait block changed unexpectedly")
text = text.replace(old_trait, new_trait, 1)

old_loop = "while let Some(frame) = source.next_frame()? {\n"
count = text.count(old_loop)
if count < 5:
    raise SystemExit(f"expected detector/source loops to use next_frame at least 5 times, found {count}")
text = text.replace(
    old_loop,
    "while let Some(frame_with_timing) = source.next_frame_with_timing()? {\n        let frame = frame_with_timing.frame;\n",
)

path.write_text(text)
print(f"updated {count} core frame-consumption loops")
