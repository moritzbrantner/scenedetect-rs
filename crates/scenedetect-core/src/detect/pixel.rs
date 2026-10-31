pub(crate) fn rgb_to_opencv_hsv(pixel: &[u8]) -> (u8, u8, u8) {
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
    let saturation = ((difference * saturation_divisor + ROUNDING) >> HSV_SHIFT).clamp(0, 255);

    let hue_numerator = if value == red {
        green - blue
    } else if value == green {
        blue - red + 2 * difference
    } else {
        red - green + 4 * difference
    };
    let hue_divisor =
        ((180_i64 << HSV_SHIFT) as f64 / (6 * difference) as f64).round_ties_even() as i32;
    let mut hue = (hue_numerator * hue_divisor + ROUNDING) >> HSV_SHIFT;
    if hue < 0 {
        hue += 180;
    }
    if hue >= 180 {
        hue -= 180;
    }

    (hue as u8, saturation as u8, value as u8)
}

pub(super) fn rgb_luma(px: &[u8]) -> f64 {
    0.299 * px[0] as f64 + 0.587 * px[1] as f64 + 0.114 * px[2] as f64
}
