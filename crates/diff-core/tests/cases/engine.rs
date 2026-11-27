use diff_core::model::DiffBody;
use diff_core::progress::ProgressHandle;
use diff_core::{DiffEngine, DiffOptions, DiffStatus};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn engine() -> DiffEngine {
    DiffEngine::new(DiffOptions::default())
}

fn fixture(relative: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.join("fixtures").join(relative)
}

#[test]
fn compares_text_files() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("old.txt");
    let right = dir.path().join("new.txt");
    fs::write(&left, "a\nb\n").unwrap();
    fs::write(&right, "a\nB\nc\n").unwrap();

    let report = engine().compare(&left, &right).unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert_eq!(report.summary.files_compared, 1);
    assert_eq!(report.summary.files_changed, 1);
    assert_eq!(report.summary.lines_added, 2);
    assert_eq!(report.summary.lines_removed, 1);
    assert!(matches!(report.root.body, DiffBody::Text(_)));
}

#[test]
fn identical_files_are_equal() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("a.txt");
    let right = dir.path().join("b.txt");
    fs::write(&left, "same\ncontent\n").unwrap();
    fs::write(&right, "same\ncontent\n").unwrap();

    let report = engine().compare(&left, &right).unwrap();
    assert_eq!(report.status, DiffStatus::Equal);
    assert_eq!(report.summary.files_equal, 1);
    assert!(matches!(report.root.body, DiffBody::Equal(_)));
}

#[test]
fn compares_json_semantically() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("old.json");
    let right = dir.path().join("new.json");
    fs::write(&left, r#"{"a":1,"b":{"c":true}}"#).unwrap();
    fs::write(&right, r#"{"a":2,"b":{"c":true}}"#).unwrap();

    let report = engine().compare(&left, &right).unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert_eq!(report.summary.structured_changes, 1);
    assert!(matches!(report.root.body, DiffBody::Structured(_)));
}

#[test]
fn compares_directories() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("left");
    let right = dir.path().join("right");
    fs::create_dir_all(&left).unwrap();
    fs::create_dir_all(&right).unwrap();

    fs::write(left.join("keep.txt"), "same\n").unwrap();
    fs::write(right.join("keep.txt"), "same\n").unwrap();
    fs::write(left.join("change.txt"), "left\n").unwrap();
    fs::write(right.join("change.txt"), "right\n").unwrap();
    fs::write(left.join("old-only.txt"), "old\n").unwrap();
    fs::write(right.join("new-only.txt"), "new\n").unwrap();

    let report = engine().compare(&left, &right).unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert_eq!(report.summary.files_compared, 2);
    assert_eq!(report.summary.files_changed, 1);
    assert_eq!(report.summary.files_added, 1);
    assert_eq!(report.summary.files_removed, 1);
    assert_eq!(report.summary.files_equal, 1);
    assert!(matches!(report.root.body, DiffBody::Directory(_)));
}

#[test]
fn compares_zip_entries() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("old.zip");
    let right = dir.path().join("new.zip");
    write_zip(&left, &[("a.txt", b"same\n"), ("b.txt", b"old\n")]);
    write_zip(&right, &[("a.txt", b"same\n"), ("b.txt", b"new\n")]);

    let report = engine().compare(&left, &right).unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert!(matches!(report.root.body, DiffBody::Archive(_)));
    assert_eq!(report.summary.lines_added, 1);
    assert_eq!(report.summary.lines_removed, 1);
}

#[test]
fn compares_images() {
    let dir = tempdir().unwrap();
    let left = dir.path().join("old.png");
    let right = dir.path().join("new.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255]))
        .save(&left)
        .unwrap();
    let mut changed = image::RgbaImage::from_pixel(2, 2, image::Rgba([0, 0, 0, 255]));
    changed.put_pixel(1, 1, image::Rgba([255, 255, 255, 255]));
    changed.save(&right).unwrap();

    let report = engine().compare(&left, &right).unwrap();
    assert_eq!(report.status, DiffStatus::Different);
    assert!(matches!(report.root.body, DiffBody::Image(_)));
}

#[test]
fn parallel_and_sequential_reports_match() {
    let compare = |left: &Path, right: &Path, jobs: usize| {
        let options = DiffOptions {
            jobs,
            ..DiffOptions::default()
        };
        DiffEngine::new(options)
            .with_progress(ProgressHandle::disabled())
            .compare(left, right)
            .unwrap()
    };

    let left = fixture("dir/left");
    let right = fixture("dir/right");
    let sequential = compare(&left, &right, 1);
    let parallel = compare(&left, &right, 4);
    assert_eq!(sequential.status, parallel.status);
    assert_eq!(
        serde_json::to_value(&sequential.summary).unwrap(),
        serde_json::to_value(&parallel.summary).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&sequential.root).unwrap(),
        serde_json::to_value(&parallel.root).unwrap()
    );

    let left = fixture("zip/old.zip");
    let right = fixture("zip/new.zip");
    let sequential = compare(&left, &right, 1);
    let parallel = compare(&left, &right, 4);
    assert_eq!(sequential.status, parallel.status);
    assert_eq!(
        serde_json::to_value(&sequential.summary).unwrap(),
        serde_json::to_value(&parallel.summary).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&sequential.root).unwrap(),
        serde_json::to_value(&parallel.root).unwrap()
    );
}

fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = fs::File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let options =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, data) in entries {
        zip.start_file(*name, options).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}
