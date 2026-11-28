//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn plans_and_applies_left_to_right() {
    let dir = tempfile::tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    fs::create_dir_all(left.join("sub")).unwrap();
    fs::create_dir_all(right.join("sub")).unwrap();
    fs::write(left.join("sub/a.txt"), "a").unwrap();
    fs::write(left.join("new.txt"), "new").unwrap();
    fs::write(right.join("old.txt"), "old").unwrap();
    fs::write(right.join("sub/a.txt"), "old a").unwrap();

    let plan = plan_sync(&left, &right, SyncMode::LeftToRight, &[], 1000).unwrap();
    assert_eq!(plan.actions.len(), 2);

    let stats = apply_sync(&plan, None).unwrap();
    assert_eq!(stats.copied, 2);
    assert_eq!(fs::read_to_string(right.join("sub/a.txt")).unwrap(), "a");
    assert!(right.join("old.txt").exists());
}

#[test]
fn mirror_deletes_extra_files_and_backs_up() {
    let dir = tempfile::tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    let backup = dir.path().join("backup");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("keep.txt"), "same").unwrap();
    fs::write(right.join("keep.txt"), "same").unwrap();
    fs::write(right.join("extra.txt"), "extra").unwrap();

    let plan = plan_sync(&left, &right, SyncMode::MirrorLeftToRight, &[], 1000).unwrap();
    let stats = apply_sync(&plan, Some(&backup)).unwrap();
    assert_eq!(stats.deleted, 1);
    assert_eq!(stats.backed_up, 1);
    assert!(!right.join("extra.txt").exists());
    assert_eq!(
        fs::read_to_string(backup.join("extra.txt")).unwrap(),
        "extra"
    );
}

#[test]
fn excludes_are_respected() {
    let dir = tempfile::tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();
    fs::write(left.join("keep.txt"), "keep").unwrap();
    fs::write(left.join("skip.log"), "skip").unwrap();

    let plan = plan_sync(
        &left,
        &right,
        SyncMode::LeftToRight,
        &["*.log".to_string()],
        1000,
    )
    .unwrap();
    assert_eq!(plan.actions.len(), 1);
    assert_eq!(plan.actions[0].relative(), "keep.txt");
}
