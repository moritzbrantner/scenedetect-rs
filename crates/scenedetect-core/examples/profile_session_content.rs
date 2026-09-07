use scenedetect_core::{
    ContentDetectorConfig, DetectionOptions, DetectionSession, DetectorConfig, Frame, FrameIndex,
    FrameRate,
};

const WIDTH: u32 = 320;
const HEIGHT: u32 = 180;
const FRAME_COUNT: u64 = 120;
const REPEATS: usize = 8;

fn main() {
    let frames = (0..FRAME_COUNT).map(patterned_frame).collect::<Vec<_>>();
    let detector = DetectorConfig::Content(ContentDetectorConfig::default());
    let options = DetectionOptions {
        min_scene_len: 1,
        ..DetectionOptions::default()
    };
    let mut checksum = 0_usize;

    for _ in 0..REPEATS {
        let mut session =
            DetectionSession::new(detector.clone(), FrameRate(24.0), options.clone());
        for frame in &frames {
            session.push_frame(frame.clone()).expect("profile frame");
        }
        let result = session.finish().expect("profile session");
        assert_eq!(result.stats.rows.len(), FRAME_COUNT as usize);
        checksum ^= result.scene_list.scenes.len();
        std::hint::black_box(&result);
    }

    std::hint::black_box(checksum);
}

fn patterned_frame(index: u64) -> Frame {
    let mut rgb = Vec::with_capacity(WIDTH as usize * HEIGHT as usize * 3);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let phase = x
                .wrapping_mul(17)
                .wrapping_add(y.wrapping_mul(29))
                .wrapping_add((index as u32).wrapping_mul(13));
            rgb.extend_from_slice(&[
                phase.wrapping_mul(3).wrapping_add(index as u32 * 5) as u8,
                phase.wrapping_mul(5).wrapping_add(41 + index as u32 * 7) as u8,
                phase.wrapping_mul(7).wrapping_add(97 + index as u32 * 11) as u8,
            ]);
        }
    }
    Frame {
        index: FrameIndex(index),
        width: WIDTH,
        height: HEIGHT,
        rgb,
    }
}
