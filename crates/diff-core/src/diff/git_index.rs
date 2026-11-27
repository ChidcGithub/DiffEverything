use serde_json::{Map, Value, json};

const MAGIC: &[u8] = b"DIRC";

/// Parse a Git index (`.git/index`) into a semantic model.
///
/// Entries are sorted by path so index ordering changes do not create noise.
/// Paths, modes, stages, blob hashes and stat metadata are exposed; padding
/// and extension layout remain outside the model.
pub fn parse_git_index(bytes: &[u8]) -> Result<Value, String> {
    let mut reader = Reader::new(bytes);
    if reader.bytes(4)? != MAGIC {
        return Err("invalid Git index magic".to_string());
    }
    let version = reader.u32()?;
    if version < 2 || version == 4 {
        return Err(format!("unsupported Git index version {version}"));
    }
    let count = reader.u32()? as usize;
    if count > 1_000_000 {
        return Err("Git index entry count is too large".to_string());
    }

    let mut entries = Vec::with_capacity(count);
    for _ in 0..count {
        let start = reader.position;
        let ctime = reader.u32()?;
        let ctime_nano = reader.u32()?;
        let mtime = reader.u32()?;
        let mtime_nano = reader.u32()?;
        let dev = reader.u32()?;
        let inode = reader.u32()?;
        let mode = reader.u32()?;
        let uid = reader.u32()?;
        let gid = reader.u32()?;
        let size = reader.u32()?;
        let sha = reader.bytes(20)?;
        let flags = reader.u16()?;
        let extended_flags = if version >= 3 && flags & 0x4000 != 0 {
            Some(reader.u16()?)
        } else {
            None
        };
        let path_length = (flags & 0x0fff) as usize;
        let path = if path_length == 0x0fff {
            let mut path = Vec::new();
            loop {
                let byte = reader.u8()?;
                if byte == 0 {
                    break;
                }
                path.push(byte);
            }
            String::from_utf8_lossy(&path).into_owned()
        } else {
            let path = reader.bytes(path_length)?;
            String::from_utf8_lossy(path).into_owned()
        };
        let after_path = reader.position;
        let aligned = start
            .checked_add((after_path - start + 8) & !7)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| "truncated Git index entry padding".to_string())?;
        reader.position = aligned;

        let mut entry = Map::new();
        entry.insert("path".to_string(), Value::String(path));
        entry.insert("mode".to_string(), Value::String(format!("{mode:06o}")));
        entry.insert("stage".to_string(), Value::from((flags >> 12) & 3));
        entry.insert("flags".to_string(), Value::from(flags));
        if let Some(extended) = extended_flags {
            entry.insert("extended_flags".to_string(), Value::from(extended));
        }
        entry.insert("sha".to_string(), Value::String(hex(sha)));
        entry.insert("size".to_string(), Value::from(size));
        entry.insert("ctime".to_string(), Value::from(ctime));
        entry.insert("ctime_nano".to_string(), Value::from(ctime_nano));
        entry.insert("mtime".to_string(), Value::from(mtime));
        entry.insert("mtime_nano".to_string(), Value::from(mtime_nano));
        entry.insert("uid".to_string(), Value::from(uid));
        entry.insert("gid".to_string(), Value::from(gid));
        entry.insert("inode".to_string(), Value::from(inode));
        entry.insert("device".to_string(), Value::from(dev));
        entries.push(Value::Object(entry));
    }
    entries.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));

    Ok(json!({
        "format": "git-index",
        "version": version,
        "entries": entries,
    }))
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn u8(&mut self) -> Result<u8, String> {
        let byte = *self
            .data
            .get(self.position)
            .ok_or_else(|| "truncated Git index".to_string())?;
        self.position += 1;
        Ok(byte)
    }

    fn u16(&mut self) -> Result<u16, String> {
        let bytes = self.bytes(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&mut self) -> Result<u32, String> {
        let bytes = self.bytes(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn bytes(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| "Git index size overflow".to_string())?;
        let slice = self
            .data
            .get(self.position..end)
            .ok_or_else(|| "truncated Git index".to_string())?;
        self.position = end;
        Ok(slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(relative: &str) -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.pop();
        path.pop();
        path.join("fixtures").join(relative)
    }

    #[test]
    fn parses_entries_and_modes() {
        let bytes = std::fs::read(fixture("git/test1.git-index")).unwrap();
        let value = parse_git_index(&bytes).unwrap();
        assert_eq!(value["format"], "git-index");
        assert_eq!(value["version"], 2);
        let entries = value["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0]["path"], "a");
        assert_eq!(entries[0]["mode"], "100644");
        assert_eq!(entries[0]["sha"].as_str().unwrap().len(), 40);
    }

    #[test]
    fn parses_multiple_entries() {
        let bytes = std::fs::read(fixture("git/test2.git-index")).unwrap();
        let value = parse_git_index(&bytes).unwrap();
        let entries = value["entries"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1]["path"], "b");
    }

    #[test]
    fn rejects_truncated_index() {
        let bytes = std::fs::read(fixture("git/test1.git-index")).unwrap();
        assert!(parse_git_index(&bytes[..16]).is_err());
    }
}
