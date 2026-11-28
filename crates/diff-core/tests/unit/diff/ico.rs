//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;
use std::path::PathBuf;

fn fixture(relative: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.join("fixtures").join(relative)
}

#[test]
fn parses_icon_directory() {
    let bytes = std::fs::read(fixture("images/diffoscope-test1.ico")).unwrap();
    let value = parse_ico(&bytes).unwrap();
    assert_eq!(value["format"], "ico");
    assert_eq!(value["type"], "icon");
    assert_eq!(value["image_count"], 1);
    assert_eq!(value["images"][0]["width"], 16);
    assert_eq!(value["images"][0]["bit_count"], 0);
    assert_eq!(value["images"][0]["color_count"], 16);
    assert_eq!(value["images"][0]["dib_bit_count"], 4);
    assert_eq!(value["images"][0]["format"], "bmp");
    assert_eq!(value["images"][0]["dib_height"], 16);
}

#[test]
fn rejects_truncated_ico() {
    let bytes = std::fs::read(fixture("images/diffoscope-test1.ico")).unwrap();
    assert!(parse_ico(&bytes[..10]).is_err());
}
