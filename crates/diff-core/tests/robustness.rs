use diff_core::model::{DiffBody, DiffStatus};
use diff_core::{DiffEngine, DiffOptions};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn fixture(relative: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.join("fixtures").join(relative)
}

#[test]
fn invalid_json_falls_back_to_text() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left.json");
    let right = dir.path().join("right.json");
    fs::write(&left, r#"{"a":1}"#).unwrap();
    fs::write(&right, "{oops").unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&left, &right)
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert!(matches!(report.root.body, DiffBody::Text(_)));
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("falling back to text diff"))
    );
}

#[test]
fn invalid_cab_falls_back_to_binary() {
    let dir = tempdir().unwrap();
    let right = dir.path().join("broken.cab");
    fs::write(&right, b"MSCF broken cabinet").unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&fixture("cab/old.cab"), &right)
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert!(matches!(report.root.body, DiffBody::Binary(_)));
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("falling back to binary"))
    );
}

#[test]
fn invalid_font_falls_back_to_binary() {
    let dir = tempdir().unwrap();
    let right = dir.path().join("broken.ttf");
    fs::write(&right, b"\x00\x01\x00\x00broken font").unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&fixture("font/regular.ttf"), &right)
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert!(matches!(report.root.body, DiffBody::Binary(_)));
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("invalid font"))
    );
}

#[test]
fn invalid_executable_falls_back_to_binary() {
    let dir = tempdir().unwrap();
    let right = dir.path().join("broken.exe");
    fs::write(&right, b"MZ this is not a PE file").unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&fixture("executable/hello-old"), &right)
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert!(matches!(report.root.body, DiffBody::Binary(_)));
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("invalid executable"))
    );
}

#[test]
fn invalid_sqlite_falls_back_to_binary() {
    let dir = tempdir().unwrap();
    let right = dir.path().join("broken.db");
    fs::write(&right, b"SQLite format 3\0broken database").unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&fixture("sqlite/old.db"), &right)
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert!(matches!(report.root.body, DiffBody::Binary(_)));
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("invalid sqlite database"))
    );
}

#[test]
fn invalid_zip_falls_back_to_binary() {
    let dir = tempdir().unwrap();
    let right = dir.path().join("right.zip");
    fs::write(&right, b"PK\x03\x04this is not a zip file").unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&fixture("zip/old.zip"), &right)
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert!(matches!(report.root.body, DiffBody::Binary(_)));
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("falling back to binary"))
    );
}

#[test]
fn invalid_package_and_image_formats_fall_back_to_binary() {
    for extension in ["deb", "rpm", "iso", "cpio"] {
        let dir = tempdir().unwrap();
        let left = dir.path().join(format!("left.{extension}"));
        let right = dir.path().join(format!("right.{extension}"));
        fs::write(&left, vec![0x41; 4096]).unwrap();
        fs::write(&right, vec![0x42; 4096]).unwrap();

        let report = DiffEngine::new(DiffOptions::default())
            .compare(&left, &right)
            .unwrap();
        assert_eq!(report.status, DiffStatus::Different, "{extension}");
        assert!(
            matches!(report.root.body, DiffBody::Binary(_)),
            "{extension}: {:?}",
            report.root.body
        );
        assert!(
            report
                .warnings
                .iter()
                .any(|warning| warning.contains("falling back to binary")),
            "{extension}: {:?}",
            report.warnings
        );
    }
}

#[test]
fn invalid_pdf_falls_back_gracefully() {
    let dir = tempdir().unwrap();
    let right = dir.path().join("broken.pdf");
    fs::write(&right, b"%PDF-1.4\nthis is not a valid pdf\n").unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&fixture("pdf/old.pdf"), &right)
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert!(matches!(
        report.root.body,
        DiffBody::Binary(_) | DiffBody::Text(_)
    ));
}

#[test]
fn empty_files_are_equal() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left.txt");
    let right = dir.path().join("right.txt");
    fs::write(&left, "").unwrap();
    fs::write(&right, "").unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&left, &right)
        .unwrap();
    assert_eq!(report.status, DiffStatus::Equal);
    assert_eq!(report.summary.files_equal, 1);
}

#[test]
fn decodes_utf16_text() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left.txt");
    let right = dir.path().join("right.txt");

    let mut left_bytes = vec![0xFF, 0xFE];
    for unit in "hello\nworld\n".encode_utf16() {
        left_bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let mut right_bytes = vec![0xFF, 0xFE];
    for unit in "hello\nthere\n".encode_utf16() {
        right_bytes.extend_from_slice(&unit.to_le_bytes());
    }
    fs::write(&left, left_bytes).unwrap();
    fs::write(&right, right_bytes).unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&left, &right)
        .unwrap();
    match report.root.body {
        DiffBody::Text(diff) => {
            assert_eq!(diff.added_lines, 1);
            assert_eq!(diff.removed_lines, 1);
            assert_eq!(diff.encoding_left, "UTF-16LE");
        }
        other => panic!("expected UTF-16 text diff, got {other:?}"),
    }
}

#[test]
fn large_files_use_streaming_summary() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left.bin");
    let right = dir.path().join("right.bin");
    fs::write(&left, vec![0u8; 100]).unwrap();
    fs::write(&right, vec![1u8; 100]).unwrap();

    let options = DiffOptions {
        max_file_bytes: 10,
        ..DiffOptions::default()
    };
    let report = DiffEngine::new(options).compare(&left, &right).unwrap();
    match report.root.body {
        DiffBody::Binary(diff) => {
            assert!(diff.truncated);
            assert_eq!(diff.differing_bytes, 100);
            assert_eq!(diff.first_difference, Some(0));
        }
        other => panic!("expected streaming binary diff, got {other:?}"),
    }
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("detailed-diff limit"))
    );
}

#[test]
fn archive_entry_limit_is_enforced() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left.zip");
    let right = dir.path().join("right.zip");
    write_zip(&left, 5, "left");
    write_zip(&right, 5, "right");

    let options = DiffOptions {
        max_zip_entries: 2,
        ..DiffOptions::default()
    };
    let report = DiffEngine::new(options).compare(&left, &right).unwrap();
    assert!(matches!(report.root.body, DiffBody::Archive(_)));
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("only the first 2 were read"))
    );
}

#[test]
fn compares_gzip_single_files() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("old.txt.gz");
    let right = dir.path().join("new.txt.gz");
    fs::write(&left, gzip_bytes(b"alpha\nbeta\n")).unwrap();
    fs::write(&right, gzip_bytes(b"alpha\nBETA\n")).unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&left, &right)
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    match report.root.body {
        DiffBody::Text(diff) => {
            assert_eq!(diff.added_lines, 1);
            assert_eq!(diff.removed_lines, 1);
        }
        other => panic!("expected gzip text diff, got {other:?}"),
    }
}

#[test]
fn nested_zip_archives_are_compared_recursively() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left.zip");
    let right = dir.path().join("right.zip");
    let inner_left = zip_bytes(&[("a.txt", b"old\n")]);
    let inner_right = zip_bytes(&[("a.txt", b"new\n")]);
    fs::write(&left, zip_bytes(&[("inner.zip", inner_left.as_slice())])).unwrap();
    fs::write(&right, zip_bytes(&[("inner.zip", inner_right.as_slice())])).unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&left, &right)
        .unwrap();
    match &report.root.body {
        DiffBody::Archive(outer) => match &outer.entries[0].body {
            DiffBody::Archive(inner) => {
                assert!(
                    inner
                        .entries
                        .iter()
                        .any(|entry| entry.status == DiffStatus::Different)
                );
            }
            other => panic!("expected nested archive, got {other:?}"),
        },
        other => panic!("expected archive diff, got {other:?}"),
    }
    assert_eq!(report.summary.lines_added, 1);
    assert_eq!(report.summary.lines_removed, 1);
}

#[test]
fn merge_clean_and_conflict() {
    let clean = diff_core::merge::merge_text(b"a\nb\nc\n", b"A\nb\nc\n", b"a\nb\nC\n");
    assert!(clean.clean);
    assert!(clean.text.contains('A'));
    assert!(clean.text.contains('C'));

    let conflict = diff_core::merge::merge_text(b"a\nb\nc\n", b"a\nLEFT\nc\n", b"a\nRIGHT\nc\n");
    assert!(!conflict.clean);
    assert!(conflict.text.contains("<<<<<<< ours"));
}

fn write_zip(path: &Path, count: usize, marker: &str) {
    let file = fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for index in 0..count {
        writer
            .start_file(format!("file-{index}.txt"), options)
            .unwrap();
        writer
            .write_all(format!("{marker} {index}\n").as_bytes())
            .unwrap();
    }
    writer.finish().unwrap();
}

fn gzip_bytes(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let cursor = std::io::Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(cursor);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap().into_inner()
}
