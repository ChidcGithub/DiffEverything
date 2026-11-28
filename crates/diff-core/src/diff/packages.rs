use crate::diff::documents_extra::{entry_inventory, hex, unzip};
use crate::diff::{serialization, structured};
use crate::model::FileKind;
use backhand::traits::filesystem::BackhandInnerNode;
use backhand::{FilesystemReader, FilesystemReaderTrait};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::io::{BufReader, Cursor, Read as _, Seek as _, Write as _};
use std::panic::{AssertUnwindSafe, catch_unwind};

const MAX_STORED_FILE_BYTES: usize = 8 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 256 * 1024;

pub fn parse_squashfs(bytes: &[u8]) -> Result<Value, String> {
    parse_squashfs_offset(bytes, "squashfs", 0)
}

pub fn parse_snap(bytes: &[u8]) -> Result<Value, String> {
    parse_squashfs_offset(bytes, "snap", 0)
}

pub fn parse_appimage(bytes: &[u8]) -> Result<Value, String> {
    let offset =
        appimage_offset(bytes).ok_or_else(|| "appimage: no SquashFS payload found".to_string())?;
    parse_squashfs_offset(bytes, "appimage", offset)
}

fn appimage_offset(bytes: &[u8]) -> Option<u64> {
    if !bytes.starts_with(b"\x7fELF") {
        return None;
    }
    let start = bytes.len().min(64);
    bytes[start..]
        .windows(4)
        .position(|window| window == b"hsqs")
        .map(|position| (start + position) as u64)
}

fn parse_squashfs_offset(bytes: &[u8], format: &str, offset: u64) -> Result<Value, String> {
    let reader =
        FilesystemReader::from_reader_with_offset(BufReader::new(Cursor::new(bytes)), offset)
            .map_err(|error| format!("{format}: {error}"))?;
    let mut entries: Vec<(String, Value)> = Vec::new();
    let mut snap_meta = Value::Null;
    for node in FilesystemReaderTrait::files(&reader) {
        let path = node.fullpath.to_string_lossy().replace('\\', "/");
        let header = node.header;
        let mut entry = Map::new();
        entry.insert(
            "mode".to_string(),
            Value::String(format!("{:04o}", header.permissions & 0o7777)),
        );
        entry.insert("uid".to_string(), json!(header.uid));
        entry.insert("gid".to_string(), json!(header.gid));
        entry.insert("mtime".to_string(), json!(header.mtime));
        match node.inner {
            BackhandInnerNode::File(file) => {
                let size = file.file_len();
                entry.insert("type".to_string(), Value::String("file".to_string()));
                entry.insert("size".to_string(), json!(size));
                if size <= MAX_STORED_FILE_BYTES {
                    let data = reader
                        .file_data(&file)
                        .map_err(|error| format!("{format}: read {path}: {error}"))?;
                    entry.insert(
                        "sha256".to_string(),
                        Value::String(hex(&Sha256::digest(&data))),
                    );
                    if let Some(text) = text_payload(&data) {
                        if path.ends_with("meta/snap.yaml")
                            && let Ok(yaml) = serde_yaml::from_slice::<serde_yaml::Value>(&data)
                        {
                            snap_meta = serde_json::to_value(yaml).unwrap_or(Value::Null);
                        }
                        entry.insert("text".to_string(), Value::String(text));
                    }
                } else {
                    entry.insert("data_omitted".to_string(), Value::Bool(true));
                }
            }
            BackhandInnerNode::Symlink { link } => {
                entry.insert("type".to_string(), Value::String("symlink".to_string()));
                entry.insert(
                    "target".to_string(),
                    Value::String(link.to_string_lossy().into_owned()),
                );
            }
            BackhandInnerNode::Dir => {
                entry.insert("type".to_string(), Value::String("dir".to_string()));
            }
            BackhandInnerNode::CharacterDevice { device_number } => {
                entry.insert("type".to_string(), Value::String("char-device".to_string()));
                entry.insert("device".to_string(), json!(device_number));
            }
            BackhandInnerNode::BlockDevice { device_number } => {
                entry.insert(
                    "type".to_string(),
                    Value::String("block-device".to_string()),
                );
                entry.insert("device".to_string(), json!(device_number));
            }
            BackhandInnerNode::NamedPipe => {
                entry.insert("type".to_string(), Value::String("fifo".to_string()));
            }
            BackhandInnerNode::Socket => {
                entry.insert("type".to_string(), Value::String("socket".to_string()));
            }
        }
        entries.push((path, Value::Object(entry)));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(json!({
        "format": format,
        "image_offset": offset,
        "snap_meta": snap_meta,
        "entries": entries.into_iter().map(|(_, value)| value).collect::<Vec<_>>(),
    }))
}

pub fn parse_flatpak(bytes: &[u8]) -> Result<Value, String> {
    if let Ok(text) = std::str::from_utf8(bytes)
        && text.trim_start().starts_with('[')
    {
        return Ok(json!({
            "format": "flatpak",
            "kind": "reference",
            "ini": parse_ini_value(text),
            "sha256": hex(&Sha256::digest(bytes)),
            "size": bytes.len(),
        }));
    }
    let strings = printable_strings(bytes);
    let refs: Vec<&String> = strings
        .iter()
        .filter(|value| {
            value.starts_with("app/")
                || value.starts_with("runtime/")
                || value.contains("/x86_64/")
                || value.contains("/aarch64/")
        })
        .collect();
    let commits: Vec<&String> = strings
        .iter()
        .filter(|value| value.len() == 64 && value.chars().all(|ch| ch.is_ascii_hexdigit()))
        .collect();
    let metadata: Vec<&String> = strings
        .iter()
        .filter(|value| {
            value.contains("name:")
                || value.contains("runtime=")
                || value.contains("metadata=")
                || value.contains("commit=")
        })
        .collect();
    Ok(json!({
        "format": "flatpak",
        "kind": "bundle",
        "refs": refs,
        "commits": commits,
        "metadata": metadata,
        "strings": strings,
        "sha256": hex(&Sha256::digest(bytes)),
        "size": bytes.len(),
    }))
}

pub fn parse_msix(bytes: &[u8]) -> Result<Value, String> {
    let files = unzip(bytes)?;
    let manifest = files
        .get("AppxManifest.xml")
        .ok_or_else(|| "msix: missing AppxManifest.xml".to_string())?;
    let manifest = structured::parse_structured(FileKind::Xml, manifest)?;
    let block_map = files
        .get("AppxBlockMap.xml")
        .map(|data| structured::parse_structured(FileKind::Xml, data))
        .transpose()?;
    let content_types = files
        .get("[Content_Types].xml")
        .map(|data| structured::parse_structured(FileKind::Xml, data))
        .transpose()?;
    Ok(json!({
        "format": "msix",
        "manifest": manifest,
        "block_map": block_map,
        "content_types": content_types,
        "entries": entry_inventory(&files),
    }))
}

pub fn parse_aab(bytes: &[u8]) -> Result<Value, String> {
    let files = unzip(bytes)?;
    let bundle_config = files
        .get("BundleConfig.pb")
        .map(|data| serialization::parse_protobuf(data))
        .transpose()?;
    let mut manifests = Map::new();
    for (name, data) in &files {
        if name.ends_with("manifest/AndroidManifest.xml") {
            manifests.insert(
                name.clone(),
                serialization::parse_protobuf(data)
                    .unwrap_or_else(|error| json!({"$parse_error": error, "size": data.len()})),
            );
        }
    }
    Ok(json!({
        "format": "aab",
        "bundle_config": bundle_config,
        "manifests": manifests,
        "entries": entry_inventory(&files),
    }))
}

pub fn parse_ext4(bytes: &[u8]) -> Result<Value, String> {
    let result = catch_unwind(AssertUnwindSafe(
        || -> Result<Vec<(String, Value)>, String> {
            let mut file =
                tempfile::tempfile().map_err(|error| format!("ext4 temp file: {error}"))?;
            file.write_all(bytes)
                .map_err(|error| format!("ext4 temp write: {error}"))?;
            file.flush()
                .map_err(|error| format!("ext4 temp flush: {error}"))?;
            let options = ext4::Options {
                checksums: ext4::Checksums::Enabled,
            };
            let volume = ext4::SuperBlock::new_with_options(file, &options)
                .map_err(|error| format!("ext4 superblock: {error}"))?;
            let root = volume
                .root()
                .map_err(|error| format!("ext4 root: {error}"))?;
            let mut entries = Vec::new();
            volume
                .walk(&root, "/", &mut |volume, path, inode, enhanced| {
                    let mut entry = Map::new();
                    entry.insert(
                        "mode".to_string(),
                        Value::String(format!("{:04o}", inode.stat.file_mode & 0o7777)),
                    );
                    entry.insert("uid".to_string(), json!(inode.stat.uid));
                    entry.insert("gid".to_string(), json!(inode.stat.gid));
                    entry.insert("size".to_string(), json!(inode.stat.size));
                    entry.insert("mtime".to_string(), json!(inode.stat.mtime.epoch_secs));
                    if !inode.stat.xattrs.is_empty() {
                        let mut xattrs = Map::new();
                        for (name, value) in inode.stat.xattrs.iter().take(32) {
                            xattrs.insert(name.clone(), Value::String(hex(value)));
                        }
                        entry.insert("xattrs".to_string(), Value::Object(xattrs));
                    }
                    let kind = match enhanced {
                        ext4::Enhanced::RegularFile => "file",
                        ext4::Enhanced::SymbolicLink(target) => {
                            entry.insert("target".to_string(), Value::String(target.clone()));
                            "symlink"
                        }
                        ext4::Enhanced::CharacterDevice(major, minor) => {
                            entry.insert("device".to_string(), json!([major, minor]));
                            "char-device"
                        }
                        ext4::Enhanced::BlockDevice(major, minor) => {
                            entry.insert("device".to_string(), json!([major, minor]));
                            "block-device"
                        }
                        ext4::Enhanced::Directory(_) => "dir",
                        ext4::Enhanced::Fifo => "fifo",
                        ext4::Enhanced::Socket => "socket",
                    };
                    entry.insert("type".to_string(), Value::String(kind.to_string()));
                    if kind == "file" {
                        if inode.stat.size > MAX_STORED_FILE_BYTES as u64 {
                            entry.insert("data_omitted".to_string(), Value::Bool(true));
                        } else {
                            let mut data = Vec::new();
                            let mut reader = volume
                                .open(inode)
                                .map_err(|error| anyhow::anyhow!("ext4 read {path}: {error}"))?;
                            reader
                                .read_to_end(&mut data)
                                .map_err(|error| anyhow::anyhow!("ext4 read {path}: {error}"))?;
                            entry.insert("actual_size".to_string(), json!(data.len()));
                            entry.insert(
                                "sha256".to_string(),
                                Value::String(hex(&Sha256::digest(&data))),
                            );
                            if let Some(text) = text_payload(&data) {
                                entry.insert("text".to_string(), Value::String(text));
                            }
                        }
                    }
                    entries.push((path.to_string(), Value::Object(entry)));
                    Ok(true)
                })
                .map_err(|error| format!("ext4 walk: {error}"))?;
            Ok(entries)
        },
    ));
    match result {
        Ok(Ok(mut entries)) => {
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            Ok(json!({
                "format": "ext4",
                "entries": entries.into_iter().map(|(_, value)| value).collect::<Vec<_>>(),
                "file_size": bytes.len(),
                "sha256": hex(&Sha256::digest(bytes)),
            }))
        }
        Ok(Err(error)) => Err(error),
        Err(_) => Err("ext4: parser panicked on this image".to_string()),
    }
}

pub fn parse_dmg(bytes: &[u8]) -> Result<Value, String> {
    let mut temp = tempfile::Builder::new()
        .suffix(".dmg")
        .tempfile()
        .map_err(|error| format!("dmg temp file: {error}"))?;
    temp.write_all(bytes)
        .map_err(|error| format!("dmg temp write: {error}"))?;
    temp.flush()
        .map_err(|error| format!("dmg temp flush: {error}"))?;
    let path = temp.path();
    let mut file = std::fs::File::open(path).map_err(|error| format!("dmg open: {error}"))?;
    let koly = udif::KolyHeader::read(&mut file).map_err(|error| format!("dmg koly: {error}"))?;
    let archive = udif::DmgArchive::open(path).map_err(|error| format!("dmg open: {error}"))?;
    let stats = archive.stats();
    let compression = archive.compression_info();
    let partitions: Vec<Value> = archive
        .partitions()
        .into_iter()
        .map(|partition| {
            json!({
                "name": partition.name,
                "id": partition.id,
                "sectors": partition.sectors,
                "size": partition.size,
                "compressed_size": partition.compressed_size,
                "partition_type": format!("{:?}", partition.partition_type),
            })
        })
        .collect();
    let block_maps = dmg_block_maps(path, &koly).unwrap_or_default();
    Ok(json!({
        "format": "dmg",
        "koly": {
            "version": koly.version,
            "sector_count": koly.sector_count,
            "data_fork_length": koly.data_fork_length,
            "data_checksum": hex(&koly.data_checksum[..usize::try_from(koly.data_checksum_size).unwrap_or(0).min(128)]),
            "master_checksum": hex(&koly.master_checksum[..usize::try_from(koly.master_checksum_size).unwrap_or(0).min(128)]),
        },
        "stats": {
            "version": stats.version,
            "sector_count": stats.sector_count,
            "partition_count": stats.partition_count,
            "total_uncompressed": stats.total_uncompressed,
            "total_compressed": stats.total_compressed,
            "data_fork_length": stats.data_fork_length,
        },
        "compression": {
            "zero_fill_blocks": compression.zero_fill_blocks,
            "raw_blocks": compression.raw_blocks,
            "zlib_blocks": compression.zlib_blocks,
            "bzip2_blocks": compression.bzip2_blocks,
            "lzfse_blocks": compression.lzfse_blocks,
            "xz_blocks": compression.xz_blocks,
            "adc_blocks": compression.adc_blocks,
        },
        "partitions": partitions,
        "block_maps": block_maps,
        "sha256": hex(&Sha256::digest(bytes)),
        "size": bytes.len(),
    }))
}

fn dmg_block_maps(path: &std::path::Path, koly: &udif::KolyHeader) -> Result<Vec<Value>, String> {
    if koly.plist_length == 0 {
        return Ok(Vec::new());
    }
    let mut file = std::fs::File::open(path).map_err(|error| format!("dmg plist open: {error}"))?;
    file.seek(std::io::SeekFrom::Start(koly.plist_offset))
        .map_err(|error| format!("dmg plist seek: {error}"))?;
    let mut data =
        vec![0u8; usize::try_from(koly.plist_length).map_err(|_| "dmg plist too large")?];
    file.read_exact(&mut data)
        .map_err(|error| format!("dmg plist read: {error}"))?;
    let Ok(plist) = plist::from_bytes::<plist::Value>(&data) else {
        return Ok(Vec::new());
    };
    let Some(resource_fork) = plist
        .as_dictionary()
        .and_then(|dict| dict.get("resource-fork"))
        .and_then(plist::Value::as_dictionary)
    else {
        return Ok(Vec::new());
    };
    let Some(blkx) = resource_fork.get("blkx").and_then(plist::Value::as_array) else {
        return Ok(Vec::new());
    };
    let mut maps = Vec::new();
    for entry in blkx {
        let Some(dict) = entry.as_dictionary() else {
            continue;
        };
        let name = dict
            .get("Name")
            .and_then(plist::Value::as_string)
            .unwrap_or("block-map")
            .to_string();
        let Some(data) = dict.get("Data").and_then(plist::Value::as_data) else {
            continue;
        };
        let Ok(mish) = udif::MishHeader::from_bytes(data) else {
            continue;
        };
        let runs: Vec<Value> = mish
            .block_runs
            .iter()
            .map(|run| {
                json!({
                    "type": format!("{:?}", run.block_type),
                    "sector": run.sector_number,
                    "sectors": run.sector_count,
                    "compressed_offset": run.compressed_offset,
                    "compressed_length": run.compressed_length,
                })
            })
            .collect();
        maps.push(json!({
            "name": name,
            "sectors": mish.sector_count,
            "checksum_type": mish.checksum_type,
            "checksum": hex(&mish.checksum[..usize::try_from(mish.checksum_size).unwrap_or(0).min(128)]),
            "block_runs": runs,
        }));
    }
    Ok(maps)
}

fn text_payload(data: &[u8]) -> Option<String> {
    if data.len() > MAX_TEXT_BYTES || data.contains(&0) {
        return None;
    }
    std::str::from_utf8(data).ok().map(str::to_string)
}

fn parse_ini_value(text: &str) -> Value {
    let mut root = Map::new();
    let mut current = String::new();
    let mut section = Map::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            if !current.is_empty() {
                root.insert(current.clone(), Value::Object(std::mem::take(&mut section)));
            }
            current = line[1..line.len() - 1].trim().to_string();
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            section.insert(
                key.trim().to_string(),
                Value::String(value.trim().to_string()),
            );
        }
    }
    if !current.is_empty() {
        root.insert(current, Value::Object(section));
    }
    Value::Object(root)
}

fn printable_strings(bytes: &[u8]) -> Vec<String> {
    let mut strings = Vec::new();
    let mut start = None;
    for (index, byte) in bytes.iter().copied().enumerate() {
        if (0x20..=0x7e).contains(&byte) {
            start.get_or_insert(index);
        } else if let Some(begin) = start.take()
            && index - begin >= 4
        {
            strings.push(String::from_utf8_lossy(&bytes[begin..index]).into_owned());
            if strings.len() >= 512 {
                break;
            }
        }
    }
    if let Some(begin) = start
        && bytes.len() - begin >= 4
        && strings.len() < 512
    {
        strings.push(String::from_utf8_lossy(&bytes[begin..]).into_owned());
    }
    strings
}

#[cfg(test)]
#[path = "../../tests/unit/diff/packages.rs"]
mod tests;
