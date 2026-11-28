//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn session_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session.json");
    let session = Session::new(
        PathBuf::from("left"),
        PathBuf::from("right"),
        DiffOptions::default(),
    );
    save_session(&path, &session).unwrap();
    let loaded = load_session(&path).unwrap();
    assert_eq!(loaded.left, PathBuf::from("left"));
    assert_eq!(loaded.right, PathBuf::from("right"));
    assert_eq!(loaded.version, SESSION_VERSION);
}
