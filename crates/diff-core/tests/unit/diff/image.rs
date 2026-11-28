//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;
use image::Rgba;

#[test]
fn identical_images_match() {
    let image = RgbaImage::from_pixel(2, 2, Rgba([1, 2, 3, 255]));
    let diff = compare_rgba(&image, &image);
    assert_eq!(diff.differing_pixels, 0);
    assert_eq!(diff.diff_ratio, 0.0);
    assert!(diff.bounding_box.is_none());
}

#[test]
fn detects_changed_pixel_and_box() {
    let left = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255]));
    let mut right = left.clone();
    right.put_pixel(1, 1, Rgba([255, 0, 0, 255]));
    let diff = compare_rgba(&left, &right);
    assert_eq!(diff.differing_pixels, 1);
    assert_eq!(diff.max_channel_delta, 255);
    let bbox = diff.bounding_box.unwrap();
    assert_eq!((bbox.x, bbox.y, bbox.width, bbox.height), (1, 1, 1, 1));
}

#[test]
fn finds_changed_rectangles() {
    let left = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 255]));
    let mut right = left.clone();
    right.put_pixel(4, 4, Rgba([255, 255, 255, 255]));
    let rectangles = diff_rectangles(&left, &right, 4);
    assert!(!rectangles.is_empty());
    assert!(rectangles.iter().any(|rectangle| {
        rectangle.x <= 4
            && rectangle.y <= 4
            && rectangle.x + rectangle.width > 4
            && rectangle.y + rectangle.height > 4
    }));
}

#[test]
fn threshold_ignores_subtle_color_differences() {
    let left = RgbaImage::from_pixel(1, 1, Rgba([100, 100, 100, 255]));
    let mut right = left.clone();
    right.put_pixel(0, 0, Rgba([103, 100, 100, 255]));
    assert_eq!(compare_rgba(&left, &right).differing_pixels, 1);
    assert_eq!(
        compare_rgba_with_threshold(&left, &right, 0.1).differing_pixels,
        0
    );

    right.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    assert_eq!(
        compare_rgba_with_threshold(&left, &right, 0.4).differing_pixels,
        1
    );
}

#[test]
fn reports_multiple_changed_regions() {
    let left = RgbaImage::from_pixel(4, 1, Rgba([0, 0, 0, 255]));
    let mut right = left.clone();
    right.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
    right.put_pixel(3, 0, Rgba([0, 255, 0, 255]));
    let diff = compare_rgba(&left, &right);
    assert_eq!(diff.regions.len(), 2, "{:?}", diff.regions);
    assert_eq!((diff.regions[0].x, diff.regions[0].width), (0, 1));
    assert_eq!((diff.regions[1].x, diff.regions[1].width), (3, 1));
    assert!(!diff.regions_truncated);
}

#[test]
fn merges_nearby_changes_into_coarse_regions() {
    let left = RgbaImage::from_pixel(300, 16, Rgba([0, 0, 0, 255]));
    let mut right = left.clone();
    right.put_pixel(1, 1, Rgba([255, 0, 0, 255]));
    right.put_pixel(2, 2, Rgba([255, 0, 0, 255]));
    right.put_pixel(290, 10, Rgba([0, 255, 0, 255]));
    let diff = compare_rgba(&left, &right);
    assert_eq!(diff.regions.len(), 2, "{:?}", diff.regions);
    assert_eq!(diff.regions[0].width, 16);
    assert_eq!(diff.regions[0].height, 16);
}

#[test]
fn threshold_removes_subtle_regions() {
    let left = RgbaImage::from_pixel(2, 2, Rgba([100, 100, 100, 255]));
    let mut right = left.clone();
    right.put_pixel(1, 1, Rgba([103, 100, 100, 255]));
    let diff = compare_rgba_with_threshold(&left, &right, 0.1);
    assert_eq!(diff.differing_pixels, 0);
    assert!(diff.regions.is_empty());
}

#[test]
fn dimension_change_counts_missing_pixels() {
    let left = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 255]));
    let right = RgbaImage::from_pixel(3, 2, Rgba([0, 0, 0, 255]));
    let diff = compare_rgba(&left, &right);
    assert_eq!(diff.differing_pixels, 2);
    assert_eq!(diff.total_pixels, 6);
}
