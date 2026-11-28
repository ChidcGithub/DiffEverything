//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn zlib_lzma2_bzip2_hdiff_encoders_roundtrip() {
    let large = vec![b'B'; 50_000];
    for (name, patch) in [
        ("zlib", create_pure_hdiff_zlib(&[], &large)),
        ("lzma2", create_pure_hdiff_lzma2(&[], &large)),
        ("bzip2", create_pure_hdiff_bzip2(&[], &large)),
    ] {
        assert!(
            patch.starts_with(format!("HDIFF13&{name}\0").as_bytes()),
            "{name}"
        );
        assert!(patch.len() < 10_000, "{name}: {}", patch.len());
        assert_eq!(apply_pure_hdiff(&[], &patch).unwrap(), large, "{name}");
    }
}

#[test]
fn zlib_lzma2_bzip2_dir_hdiff_roundtrip() {
    let workspace = tempfile::tempdir().unwrap();
    let old = workspace.path().join("old");
    let new = workspace.path().join("new");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::create_dir_all(&new).unwrap();
    std::fs::write(old.join("keep.txt"), b"old").unwrap();
    std::fs::write(new.join("keep.txt"), b"new").unwrap();
    std::fs::write(new.join("large.txt"), vec![b'C'; 50_000]).unwrap();

    for (name, create) in [
        (
            "zlib",
            create_pure_dir_hdiff_zlib as fn(&Path, &Path) -> Result<Vec<u8>>,
        ),
        (
            "lzma2",
            create_pure_dir_hdiff_lzma2 as fn(&Path, &Path) -> Result<Vec<u8>>,
        ),
        (
            "bzip2",
            create_pure_dir_hdiff_bzip2 as fn(&Path, &Path) -> Result<Vec<u8>>,
        ),
    ] {
        let out = workspace.path().join(format!("out-{name}"));
        let patch = create(&old, &new).unwrap();
        assert!(
            patch.starts_with(format!("HDIFF19&{name}&fadler64\0").as_bytes()),
            "{name}"
        );
        apply_pure_dir_hdiff(&old, &patch, &out).unwrap();
        assert_eq!(std::fs::read(out.join("keep.txt")).unwrap(), b"new");
        assert_eq!(std::fs::read(out.join("large.txt")).unwrap().len(), 50_000);
    }
}

#[test]
fn zstd_hdiff_encoder_roundtrip() {
    let old = b"the quick brown fox jumps over the lazy dog\nwith a second line\n";
    let new =
        b"the quick red fox jumps over a very lazy dog\nwith a changed second line\nand a tail\n";
    let patch = create_pure_hdiff_zstd(old, new);
    assert!(patch.starts_with(b"HDIFF13&zstd\0"));
    assert_eq!(apply_pure_hdiff(old, &patch).unwrap(), new);

    let large_new = vec![b'A'; 50_000];
    let large_patch = create_pure_hdiff_zstd(&[], &large_new);
    assert!(large_patch.len() < 10_000, "{}", large_patch.len());
    assert_eq!(apply_pure_hdiff(&[], &large_patch).unwrap(), large_new);
}

#[test]
fn zstd_dir_hdiff_roundtrip() {
    let workspace = tempfile::tempdir().unwrap();
    let old = workspace.path().join("old");
    let new = workspace.path().join("new");
    let out = workspace.path().join("out");
    std::fs::create_dir_all(old.join("nested")).unwrap();
    std::fs::create_dir_all(new.join("sub")).unwrap();
    std::fs::write(old.join("keep.txt"), b"old").unwrap();
    std::fs::write(old.join("same.txt"), b"same").unwrap();
    std::fs::write(old.join("remove.txt"), b"remove").unwrap();
    std::fs::write(old.join("nested/old.txt"), b"old nested").unwrap();
    std::fs::write(new.join("keep.txt"), b"new").unwrap();
    std::fs::write(new.join("same.txt"), b"same").unwrap();
    std::fs::write(new.join("added.txt"), b"added").unwrap();
    std::fs::write(new.join("sub/deep.txt"), b"deep").unwrap();
    std::fs::write(new.join("large.txt"), vec![b'x'; 50_000]).unwrap();

    let plain = create_pure_dir_hdiff(&old, &new).unwrap();
    let patch = create_pure_dir_hdiff_zstd(&old, &new).unwrap();
    assert!(patch.starts_with(b"HDIFF19&zstd&fadler64\0"));
    assert!(
        patch.len() < plain.len(),
        "{} !< {}",
        patch.len(),
        plain.len()
    );
    apply_pure_dir_hdiff(&old, &patch, &out).unwrap();
    assert_eq!(std::fs::read(out.join("keep.txt")).unwrap(), b"new");
    assert_eq!(std::fs::read(out.join("same.txt")).unwrap(), b"same");
    assert_eq!(std::fs::read(out.join("sub/deep.txt")).unwrap(), b"deep");
    assert!(!out.join("remove.txt").exists());
}

#[test]
fn fadler64_matches_official_directory_checksums() {
    assert_eq!(fadler64(b""), 1);
    assert_eq!(fadler64(b"old"), 0xe668_d4dc_b4ae_66a5);
    assert_eq!(fadler64(b"newadded"), 0xb9be_39c5_b556_1a97);
    assert_eq!(fadler64(b"same"), 0xc1e5_0323_ae2a_402c);
}

#[test]
fn pure_dir_hdiff_roundtrip() {
    let workspace = tempfile::tempdir().unwrap();
    let old = workspace.path().join("old");
    let new = workspace.path().join("new");
    let out = workspace.path().join("out");
    std::fs::create_dir_all(old.join("nested")).unwrap();
    std::fs::create_dir_all(new.join("sub")).unwrap();
    std::fs::write(old.join("keep.txt"), b"old").unwrap();
    std::fs::write(old.join("same.txt"), b"same").unwrap();
    std::fs::write(old.join("remove.txt"), b"remove").unwrap();
    std::fs::write(old.join("nested/old.txt"), b"old nested").unwrap();
    std::fs::write(new.join("keep.txt"), b"new").unwrap();
    std::fs::write(new.join("same.txt"), b"same").unwrap();
    std::fs::write(new.join("added.txt"), b"added").unwrap();
    std::fs::write(new.join("empty.txt"), b"").unwrap();
    std::fs::write(new.join("sub/deep.txt"), b"deep").unwrap();

    let patch = create_pure_dir_hdiff(&old, &new).unwrap();
    assert!(patch.starts_with(b"HDIFF19&&fadler64\0"));
    apply_pure_dir_hdiff(&old, &patch, &out).unwrap();

    assert_eq!(std::fs::read(out.join("keep.txt")).unwrap(), b"new");
    assert_eq!(std::fs::read(out.join("same.txt")).unwrap(), b"same");
    assert_eq!(std::fs::read(out.join("added.txt")).unwrap(), b"added");
    assert_eq!(std::fs::read(out.join("empty.txt")).unwrap(), b"");
    assert_eq!(std::fs::read(out.join("sub/deep.txt")).unwrap(), b"deep");
    assert!(!out.join("remove.txt").exists());
    assert!(!out.join("nested/old.txt").exists());
}

#[test]
fn uncompressed_hdiff_roundtrip() {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let old = std::fs::read(workspace.join("fixtures/binary/old.bin")).unwrap();
    let expected = std::fs::read(workspace.join("fixtures/binary/new.bin")).unwrap();
    let patch = std::fs::read(workspace.join("fixtures/hdiff/change.hdiff")).unwrap();
    assert!(is_uncompressed_hdiff(&patch));
    let actual = apply_pure_hdiff(&old, &patch).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn compressed_zstd_hdiff_roundtrip() {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let old = std::fs::read(workspace.join("fixtures/hdiff/large-old.bin")).unwrap();
    let expected = std::fs::read(workspace.join("fixtures/hdiff/large-new.bin")).unwrap();
    let patch = std::fs::read(workspace.join("fixtures/hdiff/large-zstd.hdiff")).unwrap();
    assert!(is_pure_hdiff(&patch));
    assert!(!is_uncompressed_hdiff(&patch));
    let actual = apply_pure_hdiff(&old, &patch).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn compressed_lzma2_hdiff_roundtrip() {
    let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let old = std::fs::read(workspace.join("fixtures/hdiff/large-old.bin")).unwrap();
    let expected = std::fs::read(workspace.join("fixtures/hdiff/large-new.bin")).unwrap();
    let patch = std::fs::read(workspace.join("fixtures/hdiff/large-lzma2.hdiff")).unwrap();
    assert!(is_pure_hdiff(&patch));
    assert!(!is_uncompressed_hdiff(&patch));
    let actual = apply_pure_hdiff(&old, &patch).unwrap();
    assert_eq!(actual, expected);
}

#[test]
fn pure_hdiff_encoder_roundtrip() {
    let old = b"the quick brown fox jumps over the lazy dog\nwith a second line\n";
    let new = b"the quick red fox jumps over a very lazy dog\nwith a changed second line\nand an extra tail\n";
    let patch = create_pure_hdiff(old, new);
    assert!(is_uncompressed_hdiff(&patch));
    assert_eq!(apply_pure_hdiff(old, &patch).unwrap(), new);

    let empty = create_pure_hdiff(b"", b"new file\n");
    assert_eq!(apply_pure_hdiff(b"", &empty).unwrap(), b"new file\n");
    let removed = create_pure_hdiff(b"removed file\n", b"");
    assert_eq!(apply_pure_hdiff(b"removed file\n", &removed).unwrap(), b"");
}

#[test]
fn patch_roundtrip() {
    let old = b"the quick brown fox jumps over the lazy dog";
    let new = b"the quick red fox jumps over the very lazy dog";
    let patch = create_patch(old, new).unwrap();
    let restored = apply_patch(old, &patch).unwrap();
    assert_eq!(restored, new);
}
