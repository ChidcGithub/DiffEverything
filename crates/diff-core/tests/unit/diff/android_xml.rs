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
fn parses_manifest_attributes_and_tree() {
    let bytes = std::fs::read(fixture("apk/manifest1.axml")).unwrap();
    let value = parse_android_xml(&bytes).unwrap();
    assert_eq!(value["format"], "android-xml");
    assert_eq!(
        value["namespaces"]["android"],
        "http://schemas.android.com/apk/res/android"
    );
    assert_eq!(value["root"]["name"], "manifest");
    assert_eq!(value["root"]["attributes"]["android:versionCode"], "1");
    assert_eq!(value["root"]["attributes"]["package"], "com.any.any");
    assert_eq!(value["root"]["children"][0]["name"], "uses-permission");
}

#[test]
fn rejects_truncated_manifest() {
    let bytes = std::fs::read(fixture("apk/manifest1.axml")).unwrap();
    assert!(parse_android_xml(&bytes[..16]).is_err());
}
