use crate::error::Result;
use image::{Rgba, RgbaImage};
use std::path::Path;

/// Write a visual heatmap: unchanged pixels are light gray, changed pixels
/// are red (intensity follows the channel delta), and the changed bounding
/// box is outlined in blue.
pub fn write_heatmap(left_path: &Path, right_path: &Path, output: &Path) -> Result<()> {
    let left = image::open(left_path)?.to_rgba8();
    let right = image::open(right_path)?.to_rgba8();
    let (left_width, left_height) = left.dimensions();
    let (right_width, right_height) = right.dimensions();
    let width = left_width.max(right_width);
    let height = left_height.max(right_height);

    let mut heatmap = RgbaImage::new(width, height);
    let mut min_x = u32::MAX;
    let mut min_y = u32::MAX;
    let mut max_x = 0u32;
    let mut max_y = 0u32;
    let mut has_diff = false;

    for y in 0..height {
        for x in 0..width {
            let left_pixel = (x < left_width && y < left_height).then(|| left.get_pixel(x, y));
            let right_pixel = (x < right_width && y < right_height).then(|| right.get_pixel(x, y));

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

            let color = if delta == 0 {
                Rgba([235, 235, 230, 255])
            } else {
                has_diff = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                let green = 255u8.saturating_sub(delta);
                Rgba([255, green, green, 255])
            };
            heatmap.put_pixel(x, y, color);
        }
    }

    if has_diff && (max_x - min_x > 2 || max_y - min_y > 2) {
        let border = Rgba([30, 80, 220, 255]);
        for x in min_x..=max_x {
            heatmap.put_pixel(x, min_y, border);
            heatmap.put_pixel(x, max_y, border);
        }
        for y in min_y..=max_y {
            heatmap.put_pixel(min_x, y, border);
            heatmap.put_pixel(max_x, y, border);
        }
    }

    heatmap.save(output)?;
    Ok(())
}

/// Write the left page with changed pixels tinted red, preserving the page
/// layout. This is used for the `--output-diff`-style PDF report.
pub fn write_overlay(left_path: &Path, right_path: &Path, output: &Path) -> Result<()> {
    let left = image::open(left_path)?.to_rgba8();
    let right = image::open(right_path)?.to_rgba8();
    let width = left.width().max(right.width());
    let height = left.height().max(right.height());
    let mut overlay = RgbaImage::new(width, height);

    for y in 0..height {
        for x in 0..width {
            let left_pixel = (x < left.width() && y < left.height()).then(|| left.get_pixel(x, y));
            let right_pixel =
                (x < right.width() && y < right.height()).then(|| right.get_pixel(x, y));
            let base = left_pixel
                .or(right_pixel)
                .copied()
                .unwrap_or(Rgba([255, 255, 255, 255]));
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
            let color = if delta == 0 {
                base
            } else {
                Rgba([
                    255,
                    (base.0[1] as f32 * 0.35) as u8,
                    (base.0[2] as f32 * 0.35) as u8,
                    255,
                ])
            };
            overlay.put_pixel(x, y, color);
        }
    }
    overlay.save(output)?;
    Ok(())
}

/// Write the two images side by side on a neutral canvas. Useful for
/// reviewing moved/resized content where a heatmap alone is hard to read.
pub fn write_side_by_side(left_path: &Path, right_path: &Path, output: &Path) -> Result<()> {
    let left = image::open(left_path)?.to_rgba8();
    let right = image::open(right_path)?.to_rgba8();
    let gap = 16u32;
    let width = left.width() + gap + right.width();
    let height = left.height().max(right.height());
    let mut canvas = RgbaImage::from_pixel(width, height, Rgba([245, 245, 240, 255]));
    let left_y = ((height - left.height()) / 2) as i64;
    let right_y = ((height - right.height()) / 2) as i64;
    image::imageops::overlay(&mut canvas, &left, 0, left_y);
    image::imageops::overlay(&mut canvas, &right, (left.width() + gap) as i64, right_y);
    canvas.save(output)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::RgbaImage;

    #[test]
    fn writes_side_by_side_file() {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("left.png");
        let right = dir.path().join("right.png");
        let output = dir.path().join("side-by-side.png");
        RgbaImage::from_pixel(3, 2, Rgba([255, 0, 0, 255]))
            .save(&left)
            .unwrap();
        RgbaImage::from_pixel(4, 3, Rgba([0, 0, 255, 255]))
            .save(&right)
            .unwrap();
        write_side_by_side(&left, &right, &output).unwrap();
        let result = image::open(&output).unwrap().to_rgba8();
        assert_eq!(result.dimensions(), (3 + 16 + 4, 3));
    }

    #[test]
    fn writes_overlay_file() {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("left.png");
        let right = dir.path().join("right.png");
        let overlay = dir.path().join("overlay.png");
        RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255]))
            .save(&left)
            .unwrap();
        let mut changed = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255]));
        changed.put_pixel(1, 1, Rgba([255, 255, 255, 255]));
        changed.save(&right).unwrap();
        write_overlay(&left, &right, &overlay).unwrap();
        let image = image::open(&overlay).unwrap().to_rgba8();
        assert_eq!(image.get_pixel(1, 1).0[0], 255);
        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0, 255]);
    }

    #[test]
    fn writes_heatmap_file() {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("left.png");
        let right = dir.path().join("right.png");
        let heatmap = dir.path().join("heatmap.png");

        RgbaImage::from_pixel(4, 4, Rgba([0, 0, 0, 255]))
            .save(&left)
            .unwrap();
        let mut changed = RgbaImage::from_pixel(4, 4, Rgba([0, 0, 0, 255]));
        changed.put_pixel(2, 2, Rgba([255, 255, 255, 255]));
        changed.save(&right).unwrap();

        write_heatmap(&left, &right, &heatmap).unwrap();
        let output = image::open(&heatmap).unwrap().to_rgba8();
        assert_eq!(output.dimensions(), (4, 4));
        assert_eq!(output.get_pixel(2, 2).0[0], 255);
    }
}
