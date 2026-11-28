//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;
use std::io::Write;

#[test]
fn detects_packer_sections() {
    assert_eq!(packer_for_section("UPX0"), Some("UPX"));
    assert_eq!(packer_for_section(".vmp0"), Some("VMProtect"));
    assert_eq!(packer_for_section(".aspack"), Some("ASPack"));
}

#[test]
fn embedded_signature_strings_are_not_evidence() {
    let info = detect_packers_bytes(b"MZ\0\0UPX! ASPack Themida VMProtect MPRESS");
    assert!(info.is_empty(), "{info:?}");
}

#[test]
fn detects_apk_packer_library() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("packed.apk");
    let mut writer = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let options = zip::write::SimpleFileOptions::default();
    writer
        .start_file("lib/arm64-v8a/libjiagu.so", options)
        .unwrap();
    writer.write_all(b"stub").unwrap();
    writer.finish().unwrap();
    let info = detect_packers(&path);
    assert!(info.iter().any(|item| item.name == "360 Jiagu"), "{info:?}");
}
