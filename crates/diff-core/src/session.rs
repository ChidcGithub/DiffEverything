use crate::engine::DiffOptions;
use crate::error::{DiffError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const SESSION_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    pub left: PathBuf,
    pub right: PathBuf,
    pub options: DiffOptions,
}

impl Session {
    pub fn new(left: PathBuf, right: PathBuf, options: DiffOptions) -> Self {
        Self {
            version: SESSION_VERSION,
            left,
            right,
            options,
        }
    }
}

pub fn save_session(path: &Path, session: &Session) -> Result<()> {
    let json = serde_json::to_string_pretty(session)?;
    std::fs::write(path, json)?;
    Ok(())
}

pub fn load_session(path: &Path) -> Result<Session> {
    let json = std::fs::read_to_string(path)?;
    let session: Session = serde_json::from_str(&json)?;
    if session.version != SESSION_VERSION {
        return Err(DiffError::Invalid(format!(
            "unsupported session version {} (expected {SESSION_VERSION})",
            session.version
        )));
    }
    Ok(session)
}

#[cfg(test)]
mod tests {
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
}
