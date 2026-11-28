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
fn parses_class_methods_and_code() {
    let bytes = std::fs::read(fixture("java/Test1.class")).unwrap();
    let value = parse_class(&bytes).unwrap();
    assert_eq!(value["format"], "java-class");
    assert_eq!(value["major_version"], 50);
    assert_eq!(value["class"], "Test");
    let methods = value["methods"].as_array().unwrap();
    assert_eq!(methods.len(), 2);
    let main = methods
        .iter()
        .find(|method| method["name"] == "main")
        .expect("main method");
    assert_eq!(main["code"]["size"], 3);
}

#[test]
fn rejects_truncated_class() {
    let bytes = std::fs::read(fixture("java/Test1.class")).unwrap();
    assert!(parse_class(&bytes[..16]).is_err());
}
