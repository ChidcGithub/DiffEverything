//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn rejects_unknown_package_payloads() {
    assert!(parse_squashfs(b"not squashfs").is_err());
    assert!(parse_appimage(b"not an elf").is_err());
    assert!(parse_msix(b"not a zip").is_err());
    assert!(parse_ext4(b"not ext4").is_err());
    assert!(parse_dmg(b"not a dmg").is_err());
}
