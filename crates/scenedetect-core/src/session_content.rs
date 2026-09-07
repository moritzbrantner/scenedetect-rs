use super::{content_score, rgb_to_opencv_hsv, ContentWeights, Frame};

/// Prepared content-detector state retained between incremental frames.
///
/// The common content/adaptive path stores only OpenCV-compatible HSV pixels so
/// each RGB frame is converted once before it becomes the previous frame. Edge
/// weighted configurations keep the raw frame and deliberately fall back to the
/// established scorer because edge-map parity remains more important than this
/// fast path.
pub(super) enum PreparedContentFrame {
    Hsv(Vec<(u8, u8, u8)>),
    Raw(Frame),
}

pub(super) fn score_and_prepare(
    previous: Option<&PreparedContentFrame>,
    frame: Frame,
    weights: &ContentWeights,
    luma_only: bool,
) -> (f64, PreparedContentFrame) {
    let uses_edges = !luma_only && weights.edges != 0.0;
    if uses_edges {
        let score = match previous {
            Some(PreparedContentFrame::Raw(previous)) => {
                content_score(previous, &frame, weights, luma_only)
            }
            Some(PreparedContentFrame::Hsv(_)) => {
                unreachable!("incremental content configuration cannot change edge mode")
            }
            None => 0.0,
        };
        return (score, PreparedContentFrame::Raw(frame));
    }

    let current_hsv = frame
        .rgb
        .chunks_exact(3)
        .map(rgb_to_opencv_hsv)
        .collect::<Vec<_>>();
    let score = match previous {
        Some(PreparedContentFrame::Hsv(previous_hsv)) => {
            hsv_content_score(previous_hsv, &current_hsv, weights, luma_only)
        }
        Some(PreparedContentFrame::Raw(previous)) => {
            // This can only occur if an internal caller changes a detector's
            // configuration between frames. Preserve score semantics rather
            // than silently comparing incompatible prepared state.
            content_score(previous, &frame, weights, luma_only)
        }
        None => 0.0,
    };

    (score, PreparedContentFrame::Hsv(current_hsv))
}

fn hsv_content_score(
    previous: &[(u8, u8, u8)],
    current: &[(u8, u8, u8)],
    weights: &ContentWeights,
    luma_only: bool,
) -> f64 {
    let (hue_weight, saturation_weight, luminance_weight) = if luma_only {
        (0.0, 0.0, 1.0)
    } else {
        (weights.hue, weights.saturation, weights.luminance)
    };
    let channel_weight_total =
        hue_weight.abs() + saturation_weight.abs() + luminance_weight.abs();
    let mut weighted_sum = 0.0;
    let mut pixel_count = 0.0;

    for (previous, current) in previous.iter().zip(current.iter()) {
        pixel_count += 1.0;
        let hue = (previous.0 as f64 - current.0 as f64).abs();
        let saturation = (previous.1 as f64 - current.1 as f64).abs();
        let luminance = (previous.2 as f64 - current.2 as f64).abs();
        weighted_sum += hue * hue_weight;
        weighted_sum += saturation * saturation_weight;
        weighted_sum += luminance * luminance_weight;
    }

    let denominator = pixel_count * channel_weight_total;
    if denominator == 0.0 {
        0.0
    } else {
        weighted_sum / denominator
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FrameIndex;

    fn patterned_frame(index: u64, seed: u32) -> Frame {
        let width = 17_u32;
        let height = 9_u32;
        let mut rgb = Vec::with_capacity(width as usize * height as usize * 3);
        for y in 0..height {
            for x in 0..width {
                let phase = x * 17 + y * 29 + seed * 37 + index as u32 * 11;
                rgb.extend_from_slice(&[
                    phase.wrapping_mul(3) as u8,
                    phase.wrapping_mul(5).wrapping_add(41) as u8,
                    phase.wrapping_mul(7).wrapping_add(97) as u8,
                ]);
            }
        }
        Frame {
            index: FrameIndex(index),
            width,
            height,
            rgb,
        }
    }

    #[test]
    fn prepared_hsv_score_matches_established_content_score() {
        let cases = [
            (ContentWeights::default(), false),
            (
                ContentWeights {
                    hue: 0.75,
                    saturation: 1.25,
                    luminance: 2.0,
                    edges: 0.0,
                },
                false,
            ),
            (ContentWeights::default(), true),
        ];

        for (weights, luma_only) in cases {
            let previous = patterned_frame(0, 3);
            let current = patterned_frame(1, 19);
            let expected = content_score(&previous, &current, &weights, luma_only);
            let (_, prepared_previous) =
                score_and_prepare(None, previous, &weights, luma_only);
            let (actual, prepared_current) = score_and_prepare(
                Some(&prepared_previous),
                current,
                &weights,
                luma_only,
            );

            assert_eq!(actual, expected);
            assert!(matches!(prepared_current, PreparedContentFrame::Hsv(_)));
        }
    }

    #[test]
    fn edge_weighted_session_keeps_established_edge_scorer() {
        let weights = ContentWeights {
            edges: 1.0,
            ..ContentWeights::default()
        };
        let previous = patterned_frame(0, 7);
        let current = patterned_frame(1, 23);
        let expected = content_score(&previous, &current, &weights, false);
        let (_, prepared_previous) = score_and_prepare(None, previous, &weights, false);
        let (actual, prepared_current) =
            score_and_prepare(Some(&prepared_previous), current, &weights, false);

        assert_eq!(actual, expected);
        assert!(matches!(prepared_current, PreparedContentFrame::Raw(_)));
    }
}
