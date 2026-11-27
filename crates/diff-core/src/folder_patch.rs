use crate::error::{DiffError, Result};
use crate::folder_merge::prepare_output;
use crate::patch::{apply_patch, create_patch};
use crate::sync::{build_excludes, collect_files, files_equal};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

const PATCH_FORMAT: &str = "diff-everything-folder-patch";
const PATCH_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, Serialize)]
pub struct FolderPatchStats {
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
    pub unchanged: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct FolderPatchApplyStats {
    pub copied: usize,
    pub added: usize,
    pub modified: usize,
    pub deleted: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct FolderPatchManifest {
    format: String,
    version: u32,
    operations: Vec<FolderPatchOperation>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum FolderPatchOperation {
    Add {
        path: String,
        entry: String,
    },
    Modify {
        path: String,
        entry: String,
        source_size: u64,
        source_hash: u64,
    },
    Delete {
        path: String,
    },
}

/// Create a directory-level patch container. Modified files are stored as
/// bsdiff patches; added files are stored whole; deleted files are recorded
/// only by path.
pub fn create_folder_patch(
    left: &Path,
    right: &Path,
    patch_path: &Path,
    excludes: &[String],
    max_entries: usize,
) -> Result<FolderPatchStats> {
    let left = canonical_dir(left, "left")?;
    let right = canonical_dir(right, "right")?;
    let excludes = build_excludes(excludes)?;
    let left_files = collect_files(&left, &excludes, max_entries)?;
    let right_files = collect_files(&right, &excludes, max_entries)?;
    let keys: BTreeSet<String> = left_files
        .keys()
        .chain(right_files.keys())
        .cloned()
        .collect();

    let mut stats = FolderPatchStats::default();
    let mut operations = Vec::new();
    let mut payloads: Vec<(String, Vec<u8>)> = Vec::new();
    for key in keys {
        match (left_files.get(&key), right_files.get(&key)) {
            (Some(left_path), Some(right_path)) => {
                if files_equal(left_path, right_path)? {
                    stats.unchanged += 1;
                    continue;
                }
                let old = fs::read(left_path)?;
                let new = fs::read(right_path)?;
                let entry = format!("patches/{}.bsdiff", payloads.len());
                payloads.push((entry.clone(), create_patch(&old, &new)?));
                operations.push(FolderPatchOperation::Modify {
                    path: key,
                    entry,
                    source_size: old.len() as u64,
                    source_hash: content_hash(&old),
                });
                stats.modified += 1;
            }
            (Some(_), None) => {
                operations.push(FolderPatchOperation::Delete { path: key });
                stats.deleted += 1;
            }
            (None, Some(right_path)) => {
                let data = fs::read(right_path)?;
                let entry = format!("files/{}.bin", payloads.len());
                payloads.push((entry.clone(), data));
                operations.push(FolderPatchOperation::Add { path: key, entry });
                stats.added += 1;
            }
            (None, None) => {}
        }
    }

    let manifest = FolderPatchManifest {
        format: PATCH_FORMAT.to_string(),
        version: PATCH_VERSION,
        operations,
    };
    let file = fs::File::create(patch_path)?;
    let mut writer = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    writer
        .start_file("manifest.json", options)
        .map_err(|error| DiffError::Invalid(format!("folder patch: {error}")))?;
    writer
        .write_all(
            &serde_json::to_vec_pretty(&manifest)
                .map_err(|error| DiffError::Invalid(format!("folder patch manifest: {error}")))?,
        )
        .map_err(DiffError::Io)?;
    for (entry, data) in payloads {
        writer
            .start_file(&entry, options)
            .map_err(|error| DiffError::Invalid(format!("folder patch: {error}")))?;
        writer.write_all(&data).map_err(DiffError::Io)?;
    }
    writer
        .finish()
        .map_err(|error| DiffError::Invalid(format!("folder patch: {error}")))?;
    Ok(stats)
}

/// Apply a directory patch to `source`, writing a complete result tree to
/// `output` (which must be empty or absent). The source directory is not
/// modified.
pub fn apply_folder_patch(
    source: &Path,
    patch_path: &Path,
    output: &Path,
    max_entries: usize,
) -> Result<FolderPatchApplyStats> {
    let source = canonical_dir(source, "source")?;
    let output = prepare_output(output)?;
    if output == source || output.starts_with(&source) {
        return Err(DiffError::Invalid(
            "patch output must not be inside the source directory".to_string(),
        ));
    }

    let file = fs::File::open(patch_path)?;
    let mut archive = zip::ZipArchive::new(BufReader::new(file))
        .map_err(|error| DiffError::Invalid(format!("folder patch: {error}")))?;
    let manifest: FolderPatchManifest = {
        let mut entry = archive
            .by_name("manifest.json")
            .map_err(|error| DiffError::Invalid(format!("folder patch manifest: {error}")))?;
        let mut data = Vec::new();
        entry.read_to_end(&mut data).map_err(DiffError::Io)?;
        serde_json::from_slice(&data)
            .map_err(|error| DiffError::Invalid(format!("folder patch manifest: {error}")))?
    };
    if manifest.format != PATCH_FORMAT || manifest.version != PATCH_VERSION {
        return Err(DiffError::Invalid(format!(
            "unsupported folder patch format {} v{}",
            manifest.format, manifest.version
        )));
    }

    let mut stats = FolderPatchApplyStats::default();
    let excludes = build_excludes(&[])?;
    for (relative, path) in collect_files(&source, &excludes, max_entries)? {
        let destination = output.join(&relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(&path, &destination)?;
        stats.copied += 1;
    }

    for operation in manifest.operations {
        match operation {
            FolderPatchOperation::Delete { path } => {
                if !safe_relative_path(&path) {
                    return Err(DiffError::Invalid(format!(
                        "unsafe path in folder patch: {path}"
                    )));
                }
                let destination = output.join(&path);
                if destination.is_file() {
                    fs::remove_file(&destination)?;
                    stats.deleted += 1;
                }
            }
            FolderPatchOperation::Add { path, entry } => {
                if !safe_relative_path(&path) || !safe_archive_entry(&entry) {
                    return Err(DiffError::Invalid(format!(
                        "unsafe add operation in folder patch: {path}"
                    )));
                }
                let data = read_archive_entry(&mut archive, &entry)?;
                write_output_file(&output, &path, &data)?;
                stats.added += 1;
            }
            FolderPatchOperation::Modify {
                path,
                entry,
                source_size,
                source_hash,
            } => {
                if !safe_relative_path(&path) || !safe_archive_entry(&entry) {
                    return Err(DiffError::Invalid(format!(
                        "unsafe modify operation in folder patch: {path}"
                    )));
                }
                let source_file = source.join(&path);
                let old = fs::read(&source_file).map_err(|error| {
                    DiffError::Invalid(format!(
                        "patch source {} is missing: {error}",
                        source_file.display()
                    ))
                })?;
                if old.len() as u64 != source_size || content_hash(&old) != source_hash {
                    return Err(DiffError::Invalid(format!(
                        "patch source {} does not match the recorded base",
                        source_file.display()
                    )));
                }
                let patch = read_archive_entry(&mut archive, &entry)?;
                let new = apply_patch(&old, &patch)?;
                write_output_file(&output, &path, &new)?;
                stats.modified += 1;
            }
        }
    }
    Ok(stats)
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

fn content_hash(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

fn safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn safe_archive_entry(entry: &str) -> bool {
    safe_relative_path(entry) && (entry.starts_with("patches/") || entry.starts_with("files/"))
}

fn read_archive_entry(
    archive: &mut zip::ZipArchive<BufReader<fs::File>>,
    entry: &str,
) -> Result<Vec<u8>> {
    let mut file = archive
        .by_name(entry)
        .map_err(|error| DiffError::Invalid(format!("folder patch entry {entry}: {error}")))?;
    let mut data = Vec::new();
    file.read_to_end(&mut data).map_err(DiffError::Io)?;
    Ok(data)
}

fn write_output_file(root: &Path, relative: &str, data: &[u8]) -> Result<()> {
    let destination = root.join(relative);
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    if destination.is_dir() {
        return Err(DiffError::Invalid(format!(
            "folder patch path is a directory: {}",
            destination.display()
        )));
    }
    fs::write(destination, data)?;
    Ok(())
}
