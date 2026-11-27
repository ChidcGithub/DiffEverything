use diff_core::folder_merge::merge_folders;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

fn write(root: &Path, relative: &str, contents: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

#[test]
fn merges_changes_additions_deletions_and_conflicts() {
    let dir = tempdir().unwrap();
    let base = dir.path().join("base");
    let ours = dir.path().join("ours");
    let theirs = dir.path().join("theirs");
    let output = dir.path().join("merged");
    for root in [&base, &ours, &theirs] {
        fs::create_dir_all(root).unwrap();
    }

    for root in [&base, &ours, &theirs] {
        write(root, "keep.txt", b"base\n");
        write(root, "same.txt", b"base\n");
        write(root, "deleted-both.txt", b"base\n");
        write(root, "delete-modify.txt", b"base\n");
        write(root, "binary.bin", &[0, 1, 2]);
        write(root, "src/main.rs", b"base\n");
    }
    write(&base, "ours.txt", b"base\n");
    write(&ours, "ours.txt", b"ours\n");
    write(&theirs, "ours.txt", b"base\n");
    write(&base, "theirs.txt", b"base\n");
    write(&ours, "theirs.txt", b"base\n");
    write(&theirs, "theirs.txt", b"theirs\n");

    write(&ours, "keep.txt", b"ours\n");
    write(&theirs, "keep.txt", b"theirs\n");
    write(&ours, "same.txt", b"same\n");
    write(&theirs, "same.txt", b"same\n");
    fs::remove_file(ours.join("deleted-both.txt")).unwrap();
    fs::remove_file(theirs.join("deleted-both.txt")).unwrap();
    fs::remove_file(ours.join("delete-modify.txt")).unwrap();
    write(&theirs, "delete-modify.txt", b"modified\n");
    write(&ours, "binary.bin", &[0, 1, 9]);
    write(&theirs, "binary.bin", &[0, 1, 8]);
    write(&theirs, "src/main.rs", b"theirs\n");
    write(&ours, "add-left.txt", b"left\n");
    write(&theirs, "add-right.txt", b"right\n");

    let result = merge_folders(&ours, &theirs, &base, &output, &[], 1000).unwrap();
    assert!(!result.clean);
    assert_eq!(
        result.conflicts,
        vec![
            "binary.bin".to_string(),
            "delete-modify.txt".to_string(),
            "keep.txt".to_string()
        ]
    );
    assert_eq!(result.written, 9);
    assert_eq!(result.deleted, 1);
    assert_eq!(fs::read(output.join("ours.txt")).unwrap(), b"ours\n");
    assert_eq!(fs::read(output.join("theirs.txt")).unwrap(), b"theirs\n");
    assert_eq!(fs::read(output.join("same.txt")).unwrap(), b"same\n");
    assert_eq!(fs::read(output.join("src/main.rs")).unwrap(), b"theirs\n");
    assert_eq!(fs::read(output.join("add-left.txt")).unwrap(), b"left\n");
    assert_eq!(fs::read(output.join("add-right.txt")).unwrap(), b"right\n");
    assert_eq!(
        fs::read(output.join("delete-modify.txt")).unwrap(),
        b"modified\n"
    );
    assert_eq!(fs::read(output.join("binary.bin")).unwrap(), vec![0, 1, 9]);
    assert!(!output.join("deleted-both.txt").exists());

    let conflict = fs::read_to_string(output.join("keep.txt")).unwrap();
    assert!(conflict.contains("<<<<<<< ours\nours\n"), "{conflict}");
    assert!(conflict.contains("=======\ntheirs\n"), "{conflict}");
    assert!(conflict.contains(">>>>>>> theirs\n"), "{conflict}");
}

#[test]
fn reports_file_directory_type_conflicts() {
    let dir = tempdir().unwrap();
    let base = dir.path().join("base");
    let ours = dir.path().join("ours");
    let theirs = dir.path().join("theirs");
    let output = dir.path().join("merged");
    for root in [&base, &ours, &theirs] {
        fs::create_dir_all(root).unwrap();
    }
    write(&ours, "node", b"file in ours\n");
    write(&theirs, "node/child.txt", b"child in theirs\n");

    let result = merge_folders(&ours, &theirs, &base, &output, &[], 1000).unwrap();
    assert!(!result.clean);
    assert!(result.conflicts.contains(&"node/child.txt".to_string()));
    assert_eq!(fs::read(output.join("node")).unwrap(), b"file in ours\n");
}

#[test]
fn clean_merge_writes_only_changed_side() {
    let dir = tempdir().unwrap();
    let base = dir.path().join("base");
    let ours = dir.path().join("ours");
    let theirs = dir.path().join("theirs");
    let output = dir.path().join("merged");
    for root in [&base, &ours, &theirs] {
        fs::create_dir_all(root).unwrap();
    }
    write(&base, "a.txt", b"base\n");
    write(&ours, "a.txt", b"ours\n");
    write(&theirs, "a.txt", b"base\n");

    let result = merge_folders(&ours, &theirs, &base, &output, &[], 1000).unwrap();
    assert!(result.clean);
    assert!(result.conflicts.is_empty());
    assert_eq!(fs::read(output.join("a.txt")).unwrap(), b"ours\n");
}
