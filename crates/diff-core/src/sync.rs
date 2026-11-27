use crate::error::{DiffError, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SyncMode {
    LeftToRight,
    RightToLeft,
    MirrorLeftToRight,
    MirrorRightToLeft,
}

impl SyncMode {
    pub fn source_is_left(self) -> bool {
        matches!(self, Self::LeftToRight | Self::MirrorLeftToRight)
    }

    pub fn mirror(self) -> bool {
        matches!(self, Self::MirrorLeftToRight | Self::MirrorRightToLeft)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::LeftToRight => "left -> right",
            Self::RightToLeft => "right -> left",
            Self::MirrorLeftToRight => "mirror left -> right",
            Self::MirrorRightToLeft => "mirror right -> left",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum SyncAction {
    Copy {
        from: PathBuf,
        to: PathBuf,
        relative: String,
    },
    Delete {
        path: PathBuf,
        relative: String,
    },
}

impl SyncAction {
    pub fn relative(&self) -> &str {
        match self {
            Self::Copy { relative, .. } | Self::Delete { relative, .. } => relative,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SyncPlan {
    pub left: PathBuf,
    pub right: PathBuf,
    pub mode: SyncMode,
    pub actions: Vec<SyncAction>,
}

impl SyncPlan {
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SyncStats {
    pub copied: usize,
    pub deleted: usize,
    pub backed_up: usize,
}

pub fn plan_sync(
    left: &Path,
    right: &Path,
    mode: SyncMode,
    excludes: &[String],
    max_entries: usize,
) -> Result<SyncPlan> {
    let left = left
        .canonicalize()
        .map_err(|error| DiffError::Invalid(format!("left directory: {error}")))?;
    let right = right
        .canonicalize()
        .map_err(|error| DiffError::Invalid(format!("right directory: {error}")))?;
    if left == right {
        return Err(DiffError::Invalid(
            "source and destination are the same directory".to_string(),
        ));
    }
    if !left.is_dir() || !right.is_dir() {
        return Err(DiffError::Invalid(
            "sync requires two directories".to_string(),
        ));
    }

    let excludes = build_excludes(excludes)?;
    let left_files = collect_files(&left, &excludes, max_entries)?;
    let right_files = collect_files(&right, &excludes, max_entries)?;

    let (destination_root, source, destination) = if mode.source_is_left() {
        (&right, &left_files, &right_files)
    } else {
        (&left, &right_files, &left_files)
    };

    let keys: BTreeSet<String> = source.keys().chain(destination.keys()).cloned().collect();
    let mut actions = Vec::new();
    for key in keys {
        match (source.get(&key), destination.get(&key)) {
            (Some(source_path), Some(destination_path)) => {
                if !files_equal(source_path, destination_path)? {
                    actions.push(SyncAction::Copy {
                        from: source_path.clone(),
                        to: destination_path.clone(),
                        relative: key,
                    });
                }
            }
            (Some(source_path), None) => {
                let target = destination_root.join(relative_path(&key));
                actions.push(SyncAction::Copy {
                    from: source_path.clone(),
                    to: target,
                    relative: key,
                });
            }
            (None, Some(destination_path)) => {
                if mode.mirror() {
                    actions.push(SyncAction::Delete {
                        path: destination_path.clone(),
                        relative: key,
                    });
                }
            }
            (None, None) => {}
        }
    }

    Ok(SyncPlan {
        left,
        right,
        mode,
        actions,
    })
}

pub fn apply_sync(plan: &SyncPlan, backup_dir: Option<&Path>) -> Result<SyncStats> {
    let mut stats = SyncStats::default();
    if let Some(backup_dir) = backup_dir {
        fs::create_dir_all(backup_dir)?;
    }

    for action in &plan.actions {
        match action {
            SyncAction::Delete { path, relative } => {
                if path.exists() {
                    if let Some(backup_dir) = backup_dir {
                        backup_file(path, backup_dir, relative)?;
                        stats.backed_up += 1;
                    }
                    fs::remove_file(path)?;
                    stats.deleted += 1;
                }
            }
            SyncAction::Copy { from, to, relative } => {
                if let Some(parent) = to.parent() {
                    fs::create_dir_all(parent)?;
                }
                if to.exists()
                    && let Some(backup_dir) = backup_dir
                {
                    backup_file(to, backup_dir, relative)?;
                    stats.backed_up += 1;
                }
                fs::copy(from, to)?;
                stats.copied += 1;
            }
        }
    }

    if plan.mode.mirror() {
        let destination = if plan.mode.source_is_left() {
            &plan.right
        } else {
            &plan.left
        };
        remove_empty_dirs(destination)?;
    }

    Ok(stats)
}

pub(crate) fn build_excludes(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in patterns {
        let glob = Glob::new(pattern).map_err(|error| {
            DiffError::Invalid(format!("invalid exclude glob {pattern:?}: {error}"))
        })?;
        builder.add(glob);
    }
    builder
        .build()
        .map_err(|error| DiffError::Invalid(format!("exclude glob: {error}")))
}

pub(crate) fn collect_files(
    root: &Path,
    excludes: &GlobSet,
    max_entries: usize,
) -> Result<BTreeMap<String, PathBuf>> {
    let mut files = BTreeMap::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|error| DiffError::Invalid(format!("walk error: {error}")))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|error| DiffError::Invalid(format!("strip prefix: {error}")))?;
        let key = relative.to_string_lossy().replace('\\', "/");
        if excludes.is_match(key.as_str()) {
            continue;
        }
        if files.len() >= max_entries {
            return Err(DiffError::Invalid(format!(
                "sync file limit exceeded (max {max_entries})"
            )));
        }
        files.insert(key, entry.path().to_path_buf());
    }
    Ok(files)
}

fn relative_path(key: &str) -> PathBuf {
    PathBuf::from(key.replace('/', std::path::MAIN_SEPARATOR_STR))
}

fn backup_file(path: &Path, backup_dir: &Path, relative: &str) -> Result<()> {
    let target = backup_dir.join(relative_path(relative));
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(path, target)?;
    Ok(())
}

fn remove_empty_dirs(root: &Path) -> Result<()> {
    let mut directories: Vec<PathBuf> = WalkDir::new(root)
        .follow_links(false)
        .min_depth(1)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_dir())
        .map(|entry| entry.path().to_path_buf())
        .collect();
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    for directory in directories {
        if directory.read_dir()?.next().is_none() {
            fs::remove_dir(&directory)?;
        }
    }
    Ok(())
}

pub(crate) fn files_equal(left: &Path, right: &Path) -> Result<bool> {
    let left_meta = fs::metadata(left)?;
    let right_meta = fs::metadata(right)?;
    if left_meta.len() != right_meta.len() {
        return Ok(false);
    }
    let mut left_reader = BufReader::new(fs::File::open(left)?);
    let mut right_reader = BufReader::new(fs::File::open(right)?);
    let mut left_buffer = [0u8; 64 * 1024];
    let mut right_buffer = [0u8; 64 * 1024];
    loop {
        let left_len = read_chunk(&mut left_reader, &mut left_buffer)?;
        let right_len = read_chunk(&mut right_reader, &mut right_buffer)?;
        if left_len != right_len {
            return Ok(false);
        }
        if left_len == 0 {
            return Ok(true);
        }
        if left_buffer[..left_len] != right_buffer[..right_len] {
            return Ok(false);
        }
    }
}

fn read_chunk<R: Read>(reader: &mut R, buffer: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buffer.len() {
        match reader.read(&mut buffer[total..])? {
            0 => break,
            read => total += read,
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
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
}
