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
fn decompiles_dex_to_java() {
    let bytes = std::fs::read(fixture("dex/test1.dex")).unwrap();
    let source = decompile_dex(&bytes).unwrap();
    assert!(source.contains("class "), "{source}");
    assert!(source.contains("BuildConfig"), "{source}");
}

#[test]
fn collapses_unchanged_dex_classes() {
    let left = std::fs::read(fixture("dex/test1.dex")).unwrap();
    let right = std::fs::read(fixture("dex/test2.dex")).unwrap();

    let diff = compare_dex(&left, &right, 3, 200).unwrap();
    assert_eq!(diff.added_lines, 0);
    assert_eq!(diff.removed_lines, 0);
    assert!(diff.hunks.is_empty());
}
