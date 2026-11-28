use crate::model::{BoundingBox, ImageDiff};
use image::{DynamicImage, RgbaImage};

pub fn compare_images(left: &DynamicImage, right: &DynamicImage, threshold: f64) -> ImageDiff {
    compare_rgba_with_threshold(&left.to_rgba8(), &right.to_rgba8(), threshold)
}

/// Compare two RGBA buffers. Missing pixels (when dimensions differ) count as
/// changed, which makes the ratio meaningful for resized exports.
pub fn compare_rgba(left: &RgbaImage, right: &RgbaImage) -> ImageDiff {
    compare_rgba_with_threshold(left, right, 0.0)
}

/// Compare two RGBA buffers using a perceptual (YIQ) color distance.
/// `threshold` is in 0..=1, matching pixelmatch's tolerance: 0 counts every
/// visible color change, while larger values ignore subtle differences.
pub fn compare_rgba_with_threshold(
    left: &RgbaImage,
    right: &RgbaImage,
    threshold: f64,
) -> ImageDiff {
    let (left_width, left_height) = left.dimensions();
    let (right_width, right_height) = right.dimensions();
    let width = left_width.max(right_width);
    let height = left_height.max(right_height);
    let total_pixels = width as u64 * height as u64;
    let max_delta = 35_215.0 * threshold.clamp(0.0, 1.0).powi(2);
    let cell = if width.max(height) > 256 { 16 } else { 1 };
    let grid_width = width.div_ceil(cell) as usize;
    let grid_height = height.div_ceil(cell) as usize;
    let mut changed_cells = vec![false; grid_width * grid_height];

    let mut differing_pixels = 0u64;
    let mut max_channel_delta = 0u8;
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0u32;
    let mut max_y = 0u32;

    for y in 0..height {
        for x in 0..width {
            let left_pixel = (x < left_width && y < left_height).then(|| left.get_pixel(x, y));
            let right_pixel = (x < right_width && y < right_height).then(|| right.get_pixel(x, y));

            let (differing, channel_delta) = match (left_pixel, right_pixel) {
                (Some(a), Some(b)) => {
                    let channel_delta =
                        a.0.iter()
                            .zip(b.0.iter())
                            .map(|(a, b)| (*a).abs_diff(*b))
                            .max()
                            .unwrap_or(0);
                    (yiq_delta(a, b) > max_delta, channel_delta)
                }
                _ => (true, 255),
            };

            if differing {
                differing_pixels += 1;
                changed_cells[(y / cell) as usize * grid_width + (x / cell) as usize] = true;
                max_channel_delta = max_channel_delta.max(channel_delta);
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    let bounding_box = (differing_pixels > 0).then(|| BoundingBox {
        x: min_x,
        y: min_y,
        width: max_x - min_x + 1,
        height: max_y - min_y + 1,
    });

    let (regions, regions_truncated) =
        merge_changed_regions(&changed_cells, grid_width, grid_height, cell, width, height);

    ImageDiff {
        left_width,
        left_height,
        right_width,
        right_height,
        differing_pixels,
        total_pixels,
        diff_ratio: if total_pixels == 0 {
            0.0
        } else {
            differing_pixels as f64 / total_pixels as f64
        },
        max_channel_delta,
        bounding_box,
        regions,
        regions_truncated,
    }
}

const MAX_REGIONS: usize = 200;

fn merge_changed_regions(
    grid: &[bool],
    grid_width: usize,
    grid_height: usize,
    cell: u32,
    width: u32,
    height: u32,
) -> (Vec<BoundingBox>, bool) {
    let mut rectangles: Vec<BoundingBox> = Vec::new();
    let mut open: std::collections::BTreeMap<(u32, u32), usize> = std::collections::BTreeMap::new();

    for grid_y in 0..grid_height {
        let mut runs = Vec::new();
        let mut grid_x = 0;
        while grid_x < grid_width {
            while grid_x < grid_width && !grid[grid_y * grid_width + grid_x] {
                grid_x += 1;
            }
            let start = grid_x;
            while grid_x < grid_width && grid[grid_y * grid_width + grid_x] {
                grid_x += 1;
            }
            if start < grid_x {
                let x = start as u32 * cell;
                let end = (grid_x as u32 * cell).min(width);
                runs.push((x, end - x));
            }
        }

        let y = grid_y as u32 * cell;
        let cell_height = cell.min(height - y);
        for (x, run_width) in runs {
            if let Some(index) = open.get(&(x, run_width)).copied() {
                let rectangle = &mut rectangles[index];
                if rectangle.y + rectangle.height == y {
                    rectangle.height += cell_height;
                    continue;
                }
            }
            rectangles.push(BoundingBox {
                x,
                y,
                width: run_width,
                height: cell_height,
            });
            open.insert((x, run_width), rectangles.len() - 1);
        }
    }

    let truncated = rectangles.len() > MAX_REGIONS;
    rectangles.truncate(MAX_REGIONS);
    (rectangles, truncated)
}

fn yiq_delta(left: &image::Rgba<u8>, right: &image::Rgba<u8>) -> f64 {
    if left[3] != right[3] {
        return 35_215.0;
    }
    let dr = f64::from(left[0]) - f64::from(right[0]);
    let dg = f64::from(left[1]) - f64::from(right[1]);
    let db = f64::from(left[2]) - f64::from(right[2]);
    let y = 0.298_895_31 * dr + 0.586_622_47 * dg + 0.114_482_23 * db;
    let i = 0.595_977_99 * dr - 0.274_176_10 * dg - 0.321_801_89 * db;
    let q = 0.211_470_17 * dr - 0.522_617_11 * dg + 0.311_146_94 * db;
    0.5053 * y * y + 0.299 * i * i + 0.1957 * q * q
}

/// Coarse changed regions in pixel coordinates, suitable for drawing vector
/// rectangles on a PDF page. Adjacent cells with differences are merged.
pub fn diff_rectangles(left: &RgbaImage, right: &RgbaImage, cell: u32) -> Vec<BoundingBox> {
    if cell == 0 {
        return Vec::new();
    }
    let width = left.width().max(right.width());
    let height = left.height().max(right.height());
    let mut rectangles: Vec<BoundingBox> = Vec::new();
    let mut open: std::collections::BTreeMap<(u32, u32), usize> = std::collections::BTreeMap::new();

    let mut y = 0u32;
    while y < height {
        let mut row_runs: Vec<(u32, u32)> = Vec::new();
        let mut x = 0u32;
        while x < width {
            let cell_width = cell.min(width - x);
            let cell_height = cell.min(height - y);
            let mut changed = false;
            'pixels: for py in y..y + cell_height {
                for px in x..x + cell_width {
                    let left_pixel =
                        (px < left.width() && py < left.height()).then(|| left.get_pixel(px, py));
                    let right_pixel = (px < right.width() && py < right.height())
                        .then(|| right.get_pixel(px, py));
                    let delta = match (left_pixel, right_pixel) {
                        (Some(a), Some(b)) => {
                            a.0.iter()
                                .zip(b.0.iter())
                                .map(|(a, b)| (*a).abs_diff(*b))
                                .max()
                                .unwrap_or(0)
                        }
                        _ => 255,
                    };
                    if delta > 0 {
                        changed = true;
                        break 'pixels;
                    }
                }
            }
            if changed {
                if let Some((run_x, run_width)) = row_runs.last_mut()
                    && *run_x + *run_width == x
                {
                    *run_width += cell_width;
                } else {
                    row_runs.push((x, cell_width));
                }
            }
            x += cell_width;
        }

        for (run_x, run_width) in row_runs {
            if let Some(index) = open.get(&(run_x, run_width)).copied() {
                let rectangle = &mut rectangles[index];
                if rectangle.y + rectangle.height == y {
                    rectangle.height += cell.min(height - y);
                } else {
                    rectangles.push(BoundingBox {
                        x: run_x,
                        y,
                        width: run_width,
                        height: cell.min(height - y),
                    });
                    open.insert((run_x, run_width), rectangles.len() - 1);
                }
            } else {
                rectangles.push(BoundingBox {
                    x: run_x,
                    y,
                    width: run_width,
                    height: cell.min(height - y),
                });
                open.insert((run_x, run_width), rectangles.len() - 1);
            }
        }
        y += cell;
    }
    rectangles
}

#[cfg(test)]
#[path = "../../tests/unit/diff/image.rs"]
mod tests;
