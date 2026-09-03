from pathlib import Path
import re

path = Path("crates/scenedetect-core/src/lib.rs")
text = path.read_text()
pattern = re.compile(r"fn content_metrics\(\n.*?\n}\n\nfn rgb_to_opencv_hsv", re.S)
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
    let channel_weight_total =
        hue_weight.abs() + saturation_weight.abs() + luminance_weight.abs() + edge_weight.abs();
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

    // PySceneDetect only needs the comparatively expensive edge map when the
    // configured edge component participates in the detector score. Default
    // weights therefore retain the HSV-only fast path.
    let delta_edges = if edge_weight != 0.0 {
        content_edge_distance(previous, current)
    } else {
        0.0
    };
    weighted_sum += delta_edges * edge_weight * pixel_count;

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
        ("delta_edges".to_owned(), delta_edges),
    ])
}

fn content_edge_distance(previous: &Frame, current: &Frame) -> f64 {
    if previous.width != current.width
        || previous.height != current.height
        || previous.width == 0
        || previous.height == 0
    {
        return 0.0;
    }

    let previous_edges = content_edge_map(previous);
    let current_edges = content_edge_map(current);
    if previous_edges.len() != current_edges.len() || previous_edges.is_empty() {
        return 0.0;
    }

    let total_distance: u64 = previous_edges
        .iter()
        .zip(current_edges.iter())
        .map(|(left, right)| left.abs_diff(*right) as u64)
        .sum();
    total_distance as f64 / previous_edges.len() as f64
}

fn content_edge_map(frame: &Frame) -> Vec<u8> {
    let width = frame.width as usize;
    let height = frame.height as usize;
    let pixel_count = width.saturating_mul(height);
    if pixel_count == 0 {
        return Vec::new();
    }

    let value_channel: Vec<u8> = frame
        .rgb
        .chunks_exact(3)
        .take(pixel_count)
        .map(|pixel| rgb_to_opencv_hsv(pixel).2)
        .collect();
    if value_channel.len() != pixel_count {
        return vec![0; pixel_count];
    }

    let canny = canny_edges(&value_channel, width, height);
    dilate_binary_edges(
        &canny,
        width,
        height,
        estimated_edge_kernel_size(width, height),
    )
}

fn estimated_edge_kernel_size(width: usize, height: usize) -> usize {
    let area = width.saturating_mul(height) as f64;
    let mut size = 4 + (area.sqrt() / 192.0).round_ties_even() as usize;
    if size.is_multiple_of(2) {
        size += 1;
    }
    size
}

fn median_u8(values: &[u8]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }

    let mut counts = [0_usize; 256];
    for value in values {
        counts[*value as usize] += 1;
    }

    let lower_rank = (values.len() - 1) / 2;
    let upper_rank = values.len() / 2;
    let mut seen = 0_usize;
    let mut lower = 0_u8;
    let mut upper = 0_u8;
    let mut have_lower = false;
    for (value, count) in counts.into_iter().enumerate() {
        let next_seen = seen + count;
        if !have_lower && lower_rank < next_seen {
            lower = value as u8;
            have_lower = true;
        }
        if upper_rank < next_seen {
            upper = value as u8;
            break;
        }
        seen = next_seen;
    }

    (lower as f64 + upper as f64) / 2.0
}

fn canny_edges(values: &[u8], width: usize, height: usize) -> Vec<u8> {
    if values.len() != width.saturating_mul(height) || width == 0 || height == 0 {
        return vec![0; width.saturating_mul(height)];
    }

    let median = median_u8(values);
    let sigma = 1.0 / 3.0;
    let low = ((1.0_f64 - sigma) * median).max(0.0) as i32;
    let high = ((1.0_f64 + sigma) * median).min(255.0) as i32;
    let (gradient_x, gradient_y) = sobel_gradients(values, width, height);
    let magnitudes: Vec<i32> = gradient_x
        .iter()
        .zip(gradient_y.iter())
        .map(|(x, y)| x.abs() + y.abs())
        .collect();

    // Mirrors OpenCV's Canny map states: 0 = weak candidate, 1 = rejected,
    // 2 = strong/connected edge. The fixed-point angle thresholds are the
    // 22.5/67.5 degree cutoffs used for 4-direction non-maximum suppression.
    const TAN_22_5_FIXED: i32 = 13_573;
    let mut states = vec![1_u8; values.len()];
    let mut stack = Vec::new();

    for y in 0..height {
        for x in 0..width {
            let index = y * width + x;
            let magnitude = magnitudes[index];
            if magnitude <= low {
                continue;
            }

            let dx = gradient_x[index];
            let dy = gradient_y[index];
            let abs_x = dx.abs();
            let abs_y_scaled = dy.abs() << 15;
            let tan_22_x = abs_x * TAN_22_5_FIXED;
            let is_local_max = if abs_y_scaled < tan_22_x {
                magnitude > magnitude_at(&magnitudes, width, height, x as isize - 1, y as isize)
                    && magnitude
                        >= magnitude_at(
                            &magnitudes,
                            width,
                            height,
                            x as isize + 1,
                            y as isize,
                        )
            } else {
                let tan_67_x = tan_22_x + (abs_x << 16);
                if abs_y_scaled > tan_67_x {
                    magnitude
                        > magnitude_at(
                            &magnitudes,
                            width,
                            height,
                            x as isize,
                            y as isize - 1,
                        )
                        && magnitude
                            >= magnitude_at(
                                &magnitudes,
                                width,
                                height,
                                x as isize,
                                y as isize + 1,
                            )
                } else {
                    let direction = if (dx ^ dy) < 0 { -1 } else { 1 };
                    magnitude
                        > magnitude_at(
                            &magnitudes,
                            width,
                            height,
                            x as isize - direction,
                            y as isize - 1,
                        )
                        && magnitude
                            > magnitude_at(
                                &magnitudes,
                                width,
                                height,
                                x as isize + direction,
                                y as isize + 1,
                            )
                }
            };

            if is_local_max {
                if magnitude > high {
                    states[index] = 2;
                    stack.push(index);
                } else {
                    states[index] = 0;
                }
            }
        }
    }

    while let Some(index) = stack.pop() {
        let x = index % width;
        let y = index / width;
        let min_x = x.saturating_sub(1);
        let max_x = (x + 1).min(width - 1);
        let min_y = y.saturating_sub(1);
        let max_y = (y + 1).min(height - 1);
        for neighbour_y in min_y..=max_y {
            for neighbour_x in min_x..=max_x {
                let neighbour = neighbour_y * width + neighbour_x;
                if states[neighbour] == 0 {
                    states[neighbour] = 2;
                    stack.push(neighbour);
                }
            }
        }
    }

    states
        .into_iter()
        .map(|state| if state == 2 { 255 } else { 0 })
        .collect()
}

fn magnitude_at(
    magnitudes: &[i32],
    width: usize,
    height: usize,
    x: isize,
    y: isize,
) -> i32 {
    if x < 0 || y < 0 || x >= width as isize || y >= height as isize {
        0
    } else {
        magnitudes[y as usize * width + x as usize]
    }
}

fn sobel_gradients(values: &[u8], width: usize, height: usize) -> (Vec<i32>, Vec<i32>) {
    let mut gradient_x = vec![0_i32; values.len()];
    let mut gradient_y = vec![0_i32; values.len()];
    for y in 0..height {
        let above = y.saturating_sub(1);
        let below = (y + 1).min(height - 1);
        for x in 0..width {
            let left = x.saturating_sub(1);
            let right = (x + 1).min(width - 1);
            let top_left = values[above * width + left] as i32;
            let top = values[above * width + x] as i32;
            let top_right = values[above * width + right] as i32;
            let middle_left = values[y * width + left] as i32;
            let middle_right = values[y * width + right] as i32;
            let bottom_left = values[below * width + left] as i32;
            let bottom = values[below * width + x] as i32;
            let bottom_right = values[below * width + right] as i32;
            let index = y * width + x;

            gradient_x[index] = -top_left + top_right - 2 * middle_left
                + 2 * middle_right
                - bottom_left
                + bottom_right;
            gradient_y[index] = -top_left - 2 * top - top_right
                + bottom_left
                + 2 * bottom
                + bottom_right;
        }
    }
    (gradient_x, gradient_y)
}

fn dilate_binary_edges(
    edges: &[u8],
    width: usize,
    height: usize,
    kernel_size: usize,
) -> Vec<u8> {
    if edges.len() != width.saturating_mul(height) || edges.is_empty() {
        return vec![0; width.saturating_mul(height)];
    }

    let radius = kernel_size / 2;
    let mut dilated = vec![0_u8; edges.len()];
    for y in 0..height {
        for x in 0..width {
            if edges[y * width + x] == 0 {
                continue;
            }
            let min_x = x.saturating_sub(radius);
            let max_x = x.saturating_add(radius).min(width - 1);
            let min_y = y.saturating_sub(radius);
            let max_y = y.saturating_add(radius).min(height - 1);
            for out_y in min_y..=max_y {
                for out_x in min_x..=max_x {
                    dilated[out_y * width + out_x] = 255;
                }
            }
        }
    }
    dilated
}

fn rgb_to_opencv_hsv'''

path.write_text(text[: match.start()] + replacement + text[match.end() :])
