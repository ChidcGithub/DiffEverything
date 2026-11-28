use diff_core::model::{ComparisonCompleteness, DiffBody, DiffStatus};
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
fn xar_table_of_contents_cannot_reserve_beyond_the_limit() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left.xar");
    let right = dir.path().join("right.xar");
    let toc = zlib_bytes(b"<xar><toc/></xar>");
    fs::write(&left, xar_header(&toc, 1024 * 1024 * 1024)).unwrap();
    fs::write(&right, xar_header(&toc, 13)).unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&left, &right)
        .unwrap();
    // A declared TOC size above the per-file limit is rejected before any
    // allocation, and the file falls back to a binary diff.
    assert!(matches!(report.root.body, DiffBody::Binary(_)));
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("table of contents declares")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn xar_table_of_contents_is_read_through_the_limit() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left.xar");
    let right = dir.path().join("right.xar");
    // The header claims a 16-byte TOC and the compressed TOC is small enough
    // for the whole file to stay under the 200-byte limit, but the zlib stream
    // expands to 50 KB; the reader must stop at the limit and report the
    // mismatch instead of decompressing everything.
    let toc = zlib_bytes(&[b'a'; 50_000]);
    assert!(
        28 + toc.len() <= 200,
        "fixture must fit the limit: {}",
        toc.len()
    );
    fs::write(&left, xar_header(&toc, 16)).unwrap();
    fs::write(&right, xar_header(&toc, 17)).unwrap();

    let options = DiffOptions {
        max_file_bytes: 200,
        ..DiffOptions::default()
    };
    let report = DiffEngine::new(options).compare(&left, &right).unwrap();
    assert!(matches!(report.root.body, DiffBody::Binary(_)));
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("table of contents size mismatch")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn directory_scan_stops_at_the_entry_limit_and_stays_indeterminate() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    for root in [&left, &right] {
        fs::create_dir_all(root).unwrap();
        for index in 0..4 {
            fs::write(root.join(format!("file{index}.txt")), "same\n").unwrap();
        }
    }

    let options = DiffOptions {
        max_dir_entries: 2,
        ..DiffOptions::default()
    };
    let report = DiffEngine::new(options).compare(&left, &right).unwrap();
    // Everything that was compared is equal, but the scan stopped after the
    // entry limit, so the equal verdict only covers the compared part.
    assert_eq!(report.status, DiffStatus::Equal);
    assert_eq!(report.completeness, ComparisonCompleteness::Truncated);
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("the rest was not scanned")),
        "{:?}",
        report.warnings
    );

    // Without the entry limit the same tree compares completely.
    let report = DiffEngine::new(DiffOptions::default())
        .compare(&left, &right)
        .unwrap();
    assert_eq!(report.status, DiffStatus::Equal);
    assert_eq!(report.completeness, ComparisonCompleteness::Complete);
}

#[test]
fn directory_scan_at_the_exact_entry_limit_is_complete() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    for root in [&left, &right] {
        fs::create_dir_all(root).unwrap();
        for index in 0..3 {
            fs::write(root.join(format!("file{index}.txt")), "same\n").unwrap();
        }
    }

    let options = DiffOptions {
        max_dir_entries: 3,
        ..DiffOptions::default()
    };
    let report = DiffEngine::new(options).compare(&left, &right).unwrap();
    assert_eq!(report.status, DiffStatus::Equal);
    // The tree has exactly as many files as the limit allows, so nothing was
    // omitted and the equal verdict covers the whole comparison.
    assert_eq!(report.completeness, ComparisonCompleteness::Complete);
    assert!(
        !report
            .warnings
            .iter()
            .any(|warning| warning.contains("directory entry limit")),
        "{:?}",
        report.warnings
    );
    let DiffBody::Directory(directory) = &report.root.body else {
        panic!("expected a directory report");
    };
    assert_eq!(directory.entries.len(), 3);
}

#[test]
fn table_change_limit_marks_the_comparison_incomplete() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left.csv");
    let right = dir.path().join("right.csv");
    fs::write(&left, "id,name\n1,Alice\n2,Bob\n3,Carol\n").unwrap();
    fs::write(&right, "id,name\n1,Alicia\n2,Robert\n3,Caroline\n").unwrap();
    let options = DiffOptions {
        max_table_rows: 1,
        ..DiffOptions::default()
    };
    let report = DiffEngine::new(options).compare(&left, &right).unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert_eq!(report.completeness, ComparisonCompleteness::Truncated);
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("table change limit")),
        "{:?}",
        report.warnings
    );
}

#[test]
fn detects_directory_renames_by_content() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("old-name.txt"), "same content\n").unwrap();
    fs::write(right.join("new-name.txt"), "same content\n").unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&left, &right)
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert_eq!(report.summary.files_compared, 1);
    assert_eq!(report.summary.files_changed, 1);
    assert_eq!(report.summary.files_equal, 0);
    let DiffBody::Directory(directory) = &report.root.body else {
        panic!("expected directory diff, got {:?}", report.root.body);
    };
    let renamed = directory
        .entries
        .iter()
        .find(|entry| entry.renamed_from.is_some())
        .expect("renamed entry");
    assert_eq!(renamed.label, "new-name.txt");
    assert_eq!(renamed.renamed_from.as_deref(), Some("old-name.txt"));
    assert!(
        !directory
            .entries
            .iter()
            .any(|entry| matches!(entry.body, DiffBody::OneSided(_))),
        "{:?}",
        directory.entries
    );
}

#[test]
fn detects_archive_renames_by_content() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left.zip");
    let right = dir.path().join("right.zip");
    fs::write(&left, zip_bytes(&[("old.txt", b"same content\n")])).unwrap();
    fs::write(&right, zip_bytes(&[("new.txt", b"same content\n")])).unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&left, &right)
        .unwrap();
    assert_eq!(report.summary.files_compared, 2);
    assert_eq!(report.summary.files_changed, 2);
    assert_eq!(report.summary.files_equal, 0);
    let DiffBody::Archive(archive) = &report.root.body else {
        panic!("expected archive diff, got {:?}", report.root.body);
    };
    let renamed = archive
        .entries
        .iter()
        .find(|entry| entry.renamed_from.is_some())
        .expect("renamed entry");
    assert_eq!(renamed.label, "new.txt");
    assert_eq!(renamed.renamed_from.as_deref(), Some("old.txt"));
}

#[test]
fn rename_pairs_duplicate_content_stably() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    for name in ["old-1.txt", "old-2.txt"] {
        fs::write(left.join(name), "duplicate content\n").unwrap();
    }
    for name in ["new-1.txt", "new-2.txt"] {
        fs::write(right.join(name), "duplicate content\n").unwrap();
    }

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&left, &right)
        .unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert_eq!(report.completeness, ComparisonCompleteness::Complete);
    let DiffBody::Directory(directory) = &report.root.body else {
        panic!("expected directory diff, got {:?}", report.root.body);
    };
    let pairs: Vec<(&str, Option<&str>)> = directory
        .entries
        .iter()
        .map(|entry| (entry.label.as_str(), entry.renamed_from.as_deref()))
        .collect();
    assert_eq!(
        pairs,
        vec![
            ("new-1.txt", Some("old-1.txt")),
            ("new-2.txt", Some("old-2.txt")),
        ],
        "{:?}",
        directory.entries
    );
}

#[test]
fn renamed_entries_reach_json_and_changed_paths() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("old.txt"), "same content\n").unwrap();
    fs::write(right.join("new.txt"), "same content\n").unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&left, &right)
        .unwrap();
    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains("\"renamed_from\":\"old.txt\""), "{json}");
    let changed = diff_core::render::changed_entries(&report.root);
    assert!(
        changed
            .iter()
            .any(|(marker, path)| *marker == '~' && path.contains("renamed from old.txt")),
        "{changed:?}"
    );
}

#[test]
fn zip_stream_compares_many_entries_without_losing_order() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left.zip");
    let right = dir.path().join("right.zip");
    let entries: Vec<(String, Vec<u8>)> = (0..1_000)
        .map(|index| {
            (
                format!("dir/{index:04}.txt"),
                format!("value {index}\n").into_bytes(),
            )
        })
        .collect();
    let mut right_entries = entries.clone();
    right_entries[500].1 = b"changed\n".to_vec();
    let left_refs: Vec<(&str, &[u8])> = entries
        .iter()
        .map(|(name, data)| (name.as_str(), data.as_slice()))
        .collect();
    let right_refs: Vec<(&str, &[u8])> = right_entries
        .iter()
        .map(|(name, data)| (name.as_str(), data.as_slice()))
        .collect();
    fs::write(&left, zip_bytes(&left_refs)).unwrap();
    fs::write(&right, zip_bytes(&right_refs)).unwrap();

    let report = DiffEngine::new(DiffOptions {
        max_zip_entries: 2_000,
        ..DiffOptions::default()
    })
    .compare(&left, &right)
    .unwrap();
    let DiffBody::Archive(archive) = &report.root.body else {
        panic!("expected archive diff, got {:?}", report.root.body);
    };
    assert_eq!(archive.total_entries, 1_000);
    assert_eq!(
        archive
            .entries
            .iter()
            .filter(|entry| entry.status.is_different())
            .count(),
        1
    );
    assert_eq!(archive.entries[500].label, "dir/0500.txt");
}

#[test]
fn tar_stream_compares_many_entries_and_renames() {
    let dir = tempdir().unwrap();
    let entries: Vec<(String, Vec<u8>)> = (0..1_000)
        .map(|index| {
            (
                format!("dir/{index:04}.txt"),
                format!("tar value {index}\n").into_bytes(),
            )
        })
        .collect();
    let mut right_entries = entries.clone();
    right_entries[500].1 = b"changed\n".to_vec();
    right_entries[999].0 = "dir/renamed.txt".to_string();
    let left = dir.path().join("left.tar");
    let right = dir.path().join("right.tar");
    fs::write(&left, tar_bytes(&entries)).unwrap();
    fs::write(&right, tar_bytes(&right_entries)).unwrap();

    let report = DiffEngine::new(DiffOptions {
        max_zip_entries: 2_000,
        ..DiffOptions::default()
    })
    .compare(&left, &right)
    .unwrap();
    let DiffBody::Archive(archive) = &report.root.body else {
        panic!("expected tar archive diff, got {:?}", report.root.body);
    };
    assert_eq!(archive.format, "tar");
    assert_eq!(archive.total_entries, 1_000);
    assert_eq!(
        archive
            .entries
            .iter()
            .filter(|entry| entry.status.is_different())
            .count(),
        2
    );
    let renamed = archive
        .entries
        .iter()
        .find(|entry| entry.renamed_from.is_some())
        .expect("renamed tar entry");
    assert_eq!(renamed.label, "dir/renamed.txt");
    assert_eq!(renamed.renamed_from.as_deref(), Some("dir/0999.txt"));
}

#[test]
fn mixed_plain_and_compressed_tar_still_compares() {
    let dir = tempdir().unwrap();
    let left_entries = vec![("a.txt".to_string(), b"alpha\n".to_vec())];
    let right_entries = vec![("a.txt".to_string(), b"beta\n".to_vec())];
    let left = dir.path().join("left.tar");
    let right = dir.path().join("right.tar.gz");
    fs::write(&left, tar_bytes(&left_entries)).unwrap();
    fs::write(&right, gzip_bytes(&tar_bytes(&right_entries))).unwrap();

    let report = DiffEngine::new(DiffOptions::default())
        .compare(&left, &right)
        .unwrap();
    let DiffBody::Archive(archive) = &report.root.body else {
        panic!("expected tar archive diff, got {:?}", report.root.body);
    };
    assert_eq!(archive.format, "tar");
    assert_eq!(archive.entries.len(), 1);
    assert_eq!(archive.entries[0].label, "a.txt");
}

fn tar_bytes(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, data) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, name, data.as_slice())
            .unwrap();
    }
    builder.into_inner().unwrap()
}

#[test]
fn expanded_bytes_budget_limits_nested_archives() {
    let dir = tempdir().unwrap();
    // The inner archives differ, so the outer files are not byte-identical and
    // the archive reader runs; its entries cannot be unpacked within the
    // budget, so both sides end up empty and nothing differs.
    let inner_left = zip_bytes(&[("a.txt", b"same text\n")]);
    let inner_right = zip_bytes(&[("a.txt", b"same text 2\n")]);
    let left = dir.path().join("left.zip");
    let right = dir.path().join("right.zip");
    fs::write(&left, zip_bytes(&[("inner.zip", inner_left.as_slice())])).unwrap();
    fs::write(&right, zip_bytes(&[("inner.zip", inner_right.as_slice())])).unwrap();

    // Unpacking the nested archive alone would exceed the budget, so the
    // comparison stops before reading it; nothing differs, hence Equal.
    let options = DiffOptions {
        max_expanded_bytes: 10,
        ..DiffOptions::default()
    };
    let report = DiffEngine::new(options).compare(&left, &right).unwrap();
    assert_eq!(report.status, DiffStatus::Equal);
    assert_eq!(report.completeness, ComparisonCompleteness::Truncated);
    assert!(
        report
            .warnings
            .iter()
            .any(|warning| warning.contains("expanded-bytes budget exhausted")),
        "{:?}",
        report.warnings
    );
    assert_eq!(report.expanded_bytes, 0);
}

#[test]
fn inflight_gate_serializes_large_files_without_changing_results() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    let payload = vec![0x51u8; 2 * 1024 * 1024];
    for root in [&left, &right] {
        fs::create_dir_all(root).unwrap();
        for index in 0..4 {
            fs::write(root.join(format!("big{index}.bin")), &payload).unwrap();
        }
    }

    // Each pair has a 4 MiB footprint against a 3 MiB in-flight budget, so
    // the scheduler clamps and serializes the large comparisons; the result
    // must be identical to an ungated run.
    let gated = DiffEngine::new(DiffOptions {
        max_inflight_bytes: 3 * 1024 * 1024,
        ..DiffOptions::default()
    })
    .compare(&left, &right)
    .unwrap();
    assert_eq!(gated.status, DiffStatus::Equal);
    assert_eq!(gated.completeness, ComparisonCompleteness::Complete);

    let ungated = DiffEngine::new(DiffOptions {
        max_inflight_bytes: 0,
        ..DiffOptions::default()
    })
    .compare(&left, &right)
    .unwrap();
    assert_eq!(ungated.status, DiffStatus::Equal);
}

#[test]
fn directory_at_the_depth_limit_is_reported_not_skipped() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    for (root, text) in [(&left, "old\n"), (&right, "new\n")] {
        fs::create_dir_all(root.join("a/b")).unwrap();
        fs::write(root.join("top.txt"), text).unwrap();
        fs::write(root.join("a/b/deep.txt"), text).unwrap();
    }

    let options = DiffOptions {
        max_depth: 1,
        ..DiffOptions::default()
    };
    let report = DiffEngine::new(options).compare(&left, &right).unwrap();
    match &report.root.body {
        DiffBody::Directory(diff) => {
            assert_eq!(diff.total_entries, 2);
            let top = diff
                .entries
                .iter()
                .find(|entry| entry.label == "top.txt")
                .expect("top.txt");
            assert_eq!(top.status, DiffStatus::Different);
            let nested = diff
                .entries
                .iter()
                .find(|entry| entry.label == "a")
                .expect("directory a");
            assert_eq!(nested.status, DiffStatus::Unsupported);
            match &nested.body {
                DiffBody::Unsupported(info) => {
                    assert_eq!(info.note, "directory depth limit reached");
                }
                other => panic!("expected unsupported node, got {other:?}"),
            }
        }
        other => panic!("expected directory diff, got {other:?}"),
    }
    assert_eq!(report.summary.lines_added, 1);
    assert_eq!(report.summary.lines_removed, 1);
}

#[test]
fn depth_limit_applies_identically_to_archives_and_directories() {
    let dir = tempdir().unwrap();
    let archive_left = dir.path().join("left.zip");
    let archive_right = dir.path().join("right.zip");
    fs::write(
        &archive_left,
        zip_bytes(&[("inner.zip", zip_bytes(&[("a.txt", b"old\n")]).as_slice())]),
    )
    .unwrap();
    fs::write(
        &archive_right,
        zip_bytes(&[("inner.zip", zip_bytes(&[("a.txt", b"new\n")]).as_slice())]),
    )
    .unwrap();
    let tree_left = dir.path().join("tree-left");
    let tree_right = dir.path().join("tree-right");
    fs::create_dir_all(tree_left.join("inner")).unwrap();
    fs::create_dir_all(tree_right.join("inner")).unwrap();
    fs::write(tree_left.join("inner/a.txt"), b"old\n").unwrap();
    fs::write(tree_right.join("inner/a.txt"), b"new\n").unwrap();

    let options = DiffOptions {
        max_depth: 1,
        ..DiffOptions::default()
    };
    let engine = DiffEngine::new(options);
    let archive = engine.compare(&archive_left, &archive_right).unwrap();
    let tree = engine.compare(&tree_left, &tree_right).unwrap();
    // Both containers sit at depth 1 == max_depth, so neither may expand and
    // both report the same kind of unsupported node.
    let archive_note = unsupported_note(&archive.root);
    let tree_note = unsupported_note(&tree.root);
    assert!(
        archive_note.contains("nested archive depth limit reached"),
        "{archive_note}"
    );
    assert!(
        tree_note.contains("directory depth limit reached"),
        "{tree_note}"
    );
}

fn unsupported_note(node: &diff_core::model::DiffNode) -> String {
    fn find(node: &diff_core::model::DiffNode) -> Option<String> {
        match &node.body {
            DiffBody::Unsupported(info) => Some(info.note.clone()),
            DiffBody::Directory(diff) => diff.entries.iter().find_map(find),
            DiffBody::Archive(diff) => diff.entries.iter().find_map(find),
            _ => None,
        }
    }
    find(node).unwrap_or_default()
}

fn xar_header(toc: &[u8], toc_uncompressed: u64) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(28 + toc.len());
    bytes.extend_from_slice(b"xar!");
    bytes.extend_from_slice(&28u16.to_be_bytes()); // header size
    bytes.extend_from_slice(&1u16.to_be_bytes()); // version
    bytes.extend_from_slice(&(toc.len() as u64).to_be_bytes());
    bytes.extend_from_slice(&toc_uncompressed.to_be_bytes());
    bytes.extend_from_slice(&1u32.to_be_bytes()); // sha1 checksum
    bytes.extend_from_slice(toc);
    bytes
}

fn zlib_bytes(data: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
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
