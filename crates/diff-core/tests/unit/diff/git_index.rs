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
fn parses_entries_and_modes() {
    let bytes = std::fs::read(fixture("git/test1.git-index")).unwrap();
    let value = parse_git_index(&bytes).unwrap();
    assert_eq!(value["format"], "git-index");
    assert_eq!(value["version"], 2);
    let entries = value["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0]["path"], "a");
    assert_eq!(entries[0]["mode"], "100644");
    assert_eq!(entries[0]["sha"].as_str().unwrap().len(), 40);
}

#[test]
fn parses_multiple_entries() {
    let bytes = std::fs::read(fixture("git/test2.git-index")).unwrap();
    let value = parse_git_index(&bytes).unwrap();
    let entries = value["entries"].as_array().unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[1]["path"], "b");
}

#[test]
fn rejects_truncated_index() {
    let bytes = std::fs::read(fixture("git/test1.git-index")).unwrap();
    assert!(parse_git_index(&bytes[..16]).is_err());
}
