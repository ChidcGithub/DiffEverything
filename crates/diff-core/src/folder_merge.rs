use crate::error::{DiffError, Result};
use crate::sync::{build_excludes, collect_files, files_equal};
use serde::Serialize;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
pub struct FolderMergeResult {
    pub output: PathBuf,
    pub written: usize,
    pub deleted: usize,
    pub conflicts: Vec<String>,
    pub clean: bool,
}

/// Conservative three-way file-tree merge.
///
/// Files changed on only one side are taken from that side. Files changed on
/// both sides are written as conflict-marked text (or as "ours" for binary
/// data) and reported in `conflicts`. A delete/modify conflict keeps the
/// modified side and is also reported.
pub fn merge_folders(
    ours: &Path,
    theirs: &Path,
    base: &Path,
    output: &Path,
    excludes: &[String],
    max_entries: usize,
) -> Result<FolderMergeResult> {
    let ours = canonical_dir(ours, "ours")?;
    let theirs = canonical_dir(theirs, "theirs")?;
    let base = canonical_dir(base, "base")?;
    let output = prepare_output(output)?;
    for (label, source) in [("ours", &ours), ("theirs", &theirs), ("base", &base)] {
        if output == *source || output.starts_with(source) {
            return Err(DiffError::Invalid(format!(
                "merge output must not be inside the {label} directory"
            )));
        }
    }

    let excludes = build_excludes(excludes)?;
    let ours_files = collect_files(&ours, &excludes, max_entries)?;
    let theirs_files = collect_files(&theirs, &excludes, max_entries)?;
    let base_files = collect_files(&base, &excludes, max_entries)?;
    let keys: BTreeSet<String> = ours_files
        .keys()
        .chain(theirs_files.keys())
        .chain(base_files.keys())
        .cloned()
        .collect();

    let mut written = 0usize;
    let mut deleted = 0usize;
    let mut conflicts = Vec::new();
    for key in keys {
        if output_path_blocked(&output, &key) {
            conflicts.push(key);
            continue;
        }
        let ours_path = ours_files.get(&key);
        let theirs_path = theirs_files.get(&key);
        let base_path = base_files.get(&key);
        let equal_ours_theirs = equal_optional(ours_path, theirs_path)?;
        let equal_ours_base = equal_optional(ours_path, base_path)?;
        let equal_theirs_base = equal_optional(theirs_path, base_path)?;

        if equal_ours_theirs {
            if let Some(path) = ours_path {
                write_copy(path, &output.join(&key))?;
                written += 1;
            } else if base_path.is_some() {
                deleted += 1;
            }
        } else if equal_ours_base {
            if let Some(path) = theirs_path {
                write_copy(path, &output.join(&key))?;
                written += 1;
            } else {
                if base_path.is_some() {
                    deleted += 1;
                }
            }
        } else if equal_theirs_base {
            if let Some(path) = ours_path {
                write_copy(path, &output.join(&key))?;
                written += 1;
            } else if base_path.is_some() {
                deleted += 1;
            }
        } else {
            conflicts.push(key.clone());
            let destination = output.join(&key);
            match (ours_path, theirs_path) {
                (Some(ours_path), Some(theirs_path)) => {
                    let ours_data = fs::read(ours_path)?;
                    let theirs_data = fs::read(theirs_path)?;
                    if is_text(&ours_data) && is_text(&theirs_data) {
                        fs::create_dir_all(destination.parent().unwrap_or(&output))?;
                        fs::write(&destination, conflict_markers(&ours_data, &theirs_data))?;
                    } else {
                        write_copy(ours_path, &destination)?;
                    }
                    written += 1;
                }
                (Some(ours_path), None) => {
                    write_copy(ours_path, &destination)?;
                    written += 1;
                }
                (None, Some(theirs_path)) => {
                    write_copy(theirs_path, &destination)?;
                    written += 1;
                }
                (None, None) => {}
            }
        }
    }

    Ok(FolderMergeResult {
        output,
        written,
        deleted,
        clean: conflicts.is_empty(),
        conflicts,
    })
}

fn canonical_dir(path: &Path, label: &str) -> Result<PathBuf> {
    let canonical = path
        .canonicalize()
        .map_err(|error| DiffError::Invalid(format!("{label} directory: {error}")))?;
    if !canonical.is_dir() {
        return Err(DiffError::Invalid(format!(
            "{label} is not a directory: {}",
            path.display()
        )));
    }
    Ok(canonical)
}

pub(crate) fn prepare_output(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        if !path.is_dir() {
            return Err(DiffError::Invalid(format!(
                "merge output is not a directory: {}",
                path.display()
            )));
        }
        if fs::read_dir(path)?.next().is_some() {
            return Err(DiffError::Invalid(format!(
                "merge output directory must be empty: {}",
                path.display()
            )));
        }
    } else {
        fs::create_dir_all(path)?;
    }
    path.canonicalize()
        .map_err(|error| DiffError::Invalid(format!("merge output: {error}")))
}

fn equal_optional(left: Option<&PathBuf>, right: Option<&PathBuf>) -> Result<bool> {
    match (left, right) {
        (Some(left), Some(right)) => files_equal(left, right),
        (None, None) => Ok(true),
        _ => Ok(false),
    }
}

fn output_path_blocked(root: &Path, relative: &str) -> bool {
    let destination = root.join(relative);
    if destination.is_dir() {
        return true;
    }
    let mut parent = destination.parent();
    while let Some(path) = parent {
        if path == root {
            break;
        }
        if path.is_file() {
            return true;
        }
        parent = path.parent();
    }
    false
}

fn write_copy(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination)?;
    Ok(())
}

fn is_text(bytes: &[u8]) -> bool {
    !bytes.contains(&0) && std::str::from_utf8(bytes).is_ok()
}

fn conflict_markers(ours: &[u8], theirs: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(ours.len() + theirs.len() + 64);
    output.extend_from_slice(b"<<<<<<< ours\n");
    output.extend_from_slice(ours);
    if !ours.ends_with(b"\n") {
        output.push(b'\n');
    }
    output.extend_from_slice(b"=======\n");
    output.extend_from_slice(theirs);
    if !theirs.ends_with(b"\n") {
        output.push(b'\n');
    }
    output.extend_from_slice(b">>>>>>> theirs\n");
    output
}
