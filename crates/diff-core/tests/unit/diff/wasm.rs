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
fn parses_exports_functions_and_code() {
    let bytes = std::fs::read(fixture("wasm/old.wasm")).unwrap();
    let value = parse_wasm(&bytes).unwrap();
    assert_eq!(value["format"], "wasm");
    assert_eq!(value["version"], 1);
    assert!(
        value["exports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|export| export["name"] == "add")
    );
    assert!(!value["code"].as_array().unwrap().is_empty());
}

#[test]
fn rejects_truncated_binary() {
    let bytes = std::fs::read(fixture("wasm/old.wasm")).unwrap();
    assert!(parse_wasm(&bytes[..12]).is_err());
}
