use diff_core::folder_patch::{apply_folder_patch, create_folder_patch};
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn write(root: &Path, relative: &str, data: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, data).unwrap();
}

#[test]
fn folder_patch_roundtrip_adds_modifies_and_deletes() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    let output = dir.path().join("output");
    let patch = dir.path().join("changes.patch");

    write(&left, "unchanged.txt", b"same\n");
    write(&right, "unchanged.txt", b"same\n");
    write(&left, "nested/modify.txt", b"old content\n");
    write(&right, "nested/modify.txt", b"new content\n");
    write(&left, "delete.txt", b"only on the left\n");
    write(&right, "added/new.txt", b"only on the right\n");
    let old_binary: Vec<u8> = (0u8..=100).cycle().take(4096).collect();
    let mut new_binary = old_binary.clone();
    new_binary[100..140].fill(0x7f);
    write(&left, "binary.bin", &old_binary);
    write(&right, "binary.bin", &new_binary);

    let created = create_folder_patch(&left, &right, &patch, &[], 10_000).unwrap();
    assert_eq!(created.added, 1);
    assert_eq!(created.modified, 2);
    assert_eq!(created.deleted, 1);
    assert_eq!(created.unchanged, 1);
    assert!(patch.is_file());

    let applied = apply_folder_patch(&left, &patch, &output, 10_000).unwrap();
    assert_eq!(applied.copied, 4);
    assert_eq!(applied.added, 1);
    assert_eq!(applied.modified, 2);
    assert_eq!(applied.deleted, 1);

    assert_eq!(fs::read(output.join("unchanged.txt")).unwrap(), b"same\n");
    assert_eq!(
        fs::read(output.join("nested/modify.txt")).unwrap(),
        b"new content\n"
    );
    assert_eq!(
        fs::read(output.join("added/new.txt")).unwrap(),
        b"only on the right\n"
    );
    assert!(!output.join("delete.txt").exists());
    assert_eq!(fs::read(output.join("binary.bin")).unwrap(), new_binary);

    // Applying a patch must not modify the source tree.
    assert_eq!(
        fs::read(left.join("nested/modify.txt")).unwrap(),
        b"old content\n"
    );
    assert!(left.join("delete.txt").exists());
}

#[test]
fn folder_patch_rejects_wrong_source_base() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    let other = dir.path().join("other");
    let output = dir.path().join("output");
    let patch = dir.path().join("changes.patch");

    write(&left, "a.txt", b"base\n");
    write(&right, "a.txt", b"target\n");
    write(&other, "a.txt", b"different base\n");
    create_folder_patch(&left, &right, &patch, &[], 1000).unwrap();

    let error = apply_folder_patch(&other, &patch, &output, 1000).unwrap_err();
    assert!(error.to_string().contains("does not match"), "{error}");
}
