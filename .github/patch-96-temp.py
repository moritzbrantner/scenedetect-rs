from pathlib import Path
import re

path = Path("crates/scenedetect-core/src/lib.rs")
text = path.read_text()
pattern = re.compile(
    r"fn content_metrics\(\n.*?\n}\n\nfn luma_histogram",
    re.S,
)
match = pattern.search(text)
if match is None:
    raise SystemExit("content_metrics block not found")

replacement = r'''fn content_metrics(
    previous: &Frame,
    current: &Frame,
    weights: &ContentWeights,
    luma_only: bool,
) -> BTreeMap<String, f64> {
    let (hue_weight, saturation_weight, luminance_weight, edge_weight) = if luma_only {
        (0.0, 0.0, 1.0, 0.0)
    } else {
        (
            weights.hue,
            weights.saturation,
            weights.luminance,
            weights.edges,
        )
    };
    let channel_weight_total = hue_weight.abs()
        + saturation_weight.abs()
        + luminance_weight.abs()
        + edge_weight.abs();
    let mut weighted_sum = 0.0;
    let mut hue_sum = 0.0;
    let mut saturation_sum = 0.0;
    let mut luminance_sum = 0.0;
    let mut pixel_count = 0.0;

    for (prev, curr) in previous
        .rgb
        .chunks_exact(3)
        .zip(current.rgb.chunks_exact(3))
    {
        pixel_count += 1.0;
        let (prev_hue, prev_saturation, prev_luminance) = rgb_to_opencv_hsv(prev);
        let (curr_hue, curr_saturation, curr_luminance) = rgb_to_opencv_hsv(curr);
        let hue = (prev_hue as f64 - curr_hue as f64).abs();
        let saturation = (prev_saturation as f64 - curr_saturation as f64).abs();
        let luminance = (prev_luminance as f64 - curr_luminance as f64).abs();
        hue_sum += hue;
        saturation_sum += saturation;
        luminance_sum += luminance;
        weighted_sum += hue * hue_weight;
        weighted_sum += saturation * saturation_weight;
        weighted_sum += luminance * luminance_weight;
    }

    // PySceneDetect normalizes over all configured component weights. Edge
    // extraction itself remains a separate parity slice, so delta_edges stays
    // zero here until that behavior is implemented explicitly.
    let denominator = pixel_count * channel_weight_total;
    let content_val = if denominator == 0.0 {
        0.0
    } else {
        weighted_sum / denominator
    };
    let component_denominator = if pixel_count == 0.0 { 1.0 } else { pixel_count };

    BTreeMap::from([
        ("content_val".to_owned(), content_val),
        ("delta_hue".to_owned(), hue_sum / component_denominator),
        (
            "delta_saturation".to_owned(),
            saturation_sum / component_denominator,
        ),
        (
            "delta_luminance".to_owned(),
            luminance_sum / component_denominator,
        ),
        ("delta_edges".to_owned(), 0.0),
    ])
}

fn rgb_to_opencv_hsv(pixel: &[u8]) -> (u8, u8, u8) {
    const HSV_SHIFT: i32 = 12;
    const ROUNDING: i32 = 1 << (HSV_SHIFT - 1);

    let red = pixel[0] as i32;
    let green = pixel[1] as i32;
    let blue = pixel[2] as i32;
    let value = red.max(green).max(blue);
    let minimum = red.min(green).min(blue);
    let difference = value - minimum;
    if difference == 0 {
        return (0, 0, value as u8);
    }

    let saturation_divisor =
        ((255_i64 << HSV_SHIFT) as f64 / value as f64).round_ties_even() as i32;
    let saturation =
        ((difference * saturation_divisor + ROUNDING) >> HSV_SHIFT).clamp(0, 255);

    let hue_numerator = if value == red {
        green - blue
    } else if value == green {
        blue - red + 2 * difference
    } else {
        red - green + 4 * difference
    };
    let hue_divisor = ((180_i64 << HSV_SHIFT) as f64 / (6 * difference) as f64)
        .round_ties_even() as i32;
    let mut hue = (hue_numerator * hue_divisor + ROUNDING) >> HSV_SHIFT;
    if hue < 0 {
        hue += 180;
    }
    if hue >= 180 {
        hue -= 180;
    }

    (hue as u8, saturation as u8, value as u8)
}

fn luma_histogram'''

path.write_text(text[: match.start()] + replacement + text[match.end() :])
