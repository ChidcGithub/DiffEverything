//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn detects_common_formats() {
    assert_eq!(
        detect_bytes("a.png", b"\x89PNG\r\n\x1a\nrest"),
        FileKind::Image
    );
    assert_eq!(detect_bytes("a.json", b"{\"a\":1}"), FileKind::Json);
    assert_eq!(detect_bytes("a.pdf", b"%PDF-1.7"), FileKind::Pdf);
    assert_eq!(detect_bytes("a.zip", b"PK\x03\x04"), FileKind::Zip);
    assert_eq!(detect_bytes("a.docx", b"PK\x03\x04"), FileKind::Docx);
    assert_eq!(detect_bytes("a.xlsx", b"PK\x03\x04"), FileKind::Xlsx);
    assert_eq!(detect_bytes("a.pptx", b"PK\x03\x04"), FileKind::Pptx);
    assert_eq!(detect_bytes("a.rs", b"fn main() {}"), FileKind::Text);
    assert_eq!(detect_bytes("a.bin", &[0, 1, 2]), FileKind::Binary);
}

#[test]
fn detects_structured_and_tabular_formats() {
    assert_eq!(detect_bytes("a.yaml", b"name: x\n"), FileKind::Yaml);
    assert_eq!(detect_bytes("a.toml", b"name = 'x'\n"), FileKind::Toml);
    assert_eq!(detect_bytes("a.xml", b"<root/>"), FileKind::Xml);
    assert_eq!(
        detect_bytes("a.plist", b"<?xml version=\"1.0\"?><plist/>"),
        FileKind::Plist
    );
    assert_eq!(detect_bytes("a.plist", b"bplist00rest"), FileKind::Plist);
    assert_eq!(detect_bytes("a.pkl", b"\x80\x04rest"), FileKind::Pickle);
    assert_eq!(
        detect_bytes("model.pickle", b"\x80\x05rest"),
        FileKind::Pickle
    );
    assert_eq!(
        detect_bytes("binary-plist", b"bplist00rest"),
        FileKind::Plist
    );
    assert_eq!(detect_bytes("a.csv", b"a,b\n1,2\n"), FileKind::Csv);
    assert_eq!(detect_bytes("a.tsv", b"a\tb\n"), FileKind::Csv);
    assert_eq!(
        detect_bytes("a.ipynb", b"{\"cells\":[]}"),
        FileKind::Notebook
    );
    assert_eq!(detect_bytes("a.mp3", b"ID3"), FileKind::Audio);
}

#[test]
fn detects_archives() {
    assert_eq!(detect_bytes("a.tar", b"far away"), FileKind::Tar);
    assert_eq!(detect_bytes("a.tar.gz", b"\x1f\x8b"), FileKind::Tar);
    assert_eq!(detect_bytes("a.tgz", b"\x1f\x8b"), FileKind::Tar);
    assert_eq!(
        detect_bytes("a.tar.zst", b"\x28\xb5\x2f\xfd"),
        FileKind::Tar
    );
    assert_eq!(detect_bytes("a.tzst", b"\x28\xb5\x2f\xfd"), FileKind::Tar);
    assert_eq!(
        detect_bytes("a.log.zst", b"\x28\xb5\x2f\xfd"),
        FileKind::Zstd
    );
    assert_eq!(
        detect_bytes("archive.zst", b"\x28\xb5\x2f\xfd"),
        FileKind::Zstd
    );
    assert_eq!(detect_bytes("data.bz2", b"BZh9rest"), FileKind::Bzip2);
    assert_eq!(detect_bytes("data.xz", b"\xfd7zXZ\0rest"), FileKind::Xz);
    assert_eq!(
        detect_bytes("data.lz4", b"\x04\x22\x4d\x18rest"),
        FileKind::Lz4
    );
    assert_eq!(
        detect_bytes("a.tar.lz4", b"\x04\x22\x4d\x18rest"),
        FileKind::Tar
    );
    assert_eq!(
        detect_bytes("a.7z", b"7z\xbc\xaf\x27\x1c"),
        FileKind::SevenZip
    );
    assert_eq!(detect_bytes("a.deb", b"!<arch>\n"), FileKind::Deb);
    assert_eq!(detect_bytes("a.iso", b"CD001"), FileKind::Iso);
    assert_eq!(detect_bytes("a.cpio", b"070701rest"), FileKind::Cpio);
    assert_eq!(
        detect_bytes("a.rpm", b"\xed\xab\xee\xdbrest"),
        FileKind::Rpm
    );
}

#[test]
fn extensionless_utf8_is_text() {
    assert_eq!(detect_bytes("README", b"hello\nworld\n"), FileKind::Text);
}

#[test]
fn xml_magic_is_detected() {
    assert_eq!(
        detect_bytes("unknown", b"<?xml version=\"1.0\"?><root/>"),
        FileKind::Xml
    );
}
