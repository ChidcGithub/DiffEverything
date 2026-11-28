//! Unit tests for this module, compiled as its child through `#[path]`.
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
