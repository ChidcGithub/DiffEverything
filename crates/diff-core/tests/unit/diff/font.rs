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
fn extracts_font_metadata_and_advances() {
    let bytes = std::fs::read(fixture("font/regular.ttf")).unwrap();
    let value = parse_font(&bytes).unwrap();
    assert_eq!(value["family"], "Lato");
    assert_eq!(value["subfamily"], "Regular");
    assert_eq!(value["weight"], 400);
    assert_eq!(value["bold"], false);
    assert!(value["number_of_glyphs"].as_u64().unwrap() > 100);
    assert_eq!(value["ascii_advances"].as_array().unwrap().len(), 95);
}

#[test]
fn extracts_woff_metadata() {
    let bytes = std::fs::read(fixture("font/regular.woff")).unwrap();
    let value = parse_font(&bytes).unwrap();
    assert_eq!(value["family"], "Lato");
    assert_eq!(value["subfamily"], "Regular");
    assert_eq!(value["weight"], 400);
    assert!(value["number_of_glyphs"].as_u64().unwrap() > 100);
}

#[test]
fn extracts_woff2_metadata() {
    let bytes = std::fs::read(fixture("font/regular.woff2")).unwrap();
    let value = parse_font(&bytes).unwrap();
    assert_eq!(value["family"], "Lato");
    assert_eq!(value["subfamily"], "Regular");
    assert_eq!(value["weight"], 400);
    assert!(value["number_of_glyphs"].as_u64().unwrap() > 100);
}

#[test]
fn rejects_non_font_bytes() {
    assert!(parse_font(b"not a font").is_err());
}
