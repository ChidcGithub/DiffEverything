use serde_json::{Map, Value, json};

const MAGIC: &[u8] = b"\0asm";

/// Parse a WebAssembly binary into a deterministic semantic model.
///
/// The model exposes types, imports, exports, function type indices, code body
/// digests and custom sections. This keeps diffs stable across harmless section
/// ordering changes while still locating code and export changes.
pub fn parse_wasm(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() < 8 || !bytes.starts_with(MAGIC) {
        return Err("invalid WebAssembly magic".to_string());
    }
    let version = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    if version != 1 {
        return Err(format!("unsupported WebAssembly version {version}"));
    }

    let mut reader = Reader::new(&bytes[8..]);
    let mut types = Vec::new();
    let mut imports = Vec::new();
    let mut functions = Vec::new();
    let mut tables = Vec::new();
    let mut memories = Vec::new();
    let mut globals = Vec::new();
    let mut exports = Vec::new();
    let mut code = Vec::new();
    let mut custom_sections = Vec::new();
    let mut start = None;
    let mut data_section = None;
    let mut element_section = None;

    while reader.remaining() > 0 {
        let id = reader.u8()?;
        let size = reader.uleb()? as usize;
        let payload = reader.bytes(size)?;
        let mut section = Reader::new(payload);
        match id {
            0 => {
                let name = section.name()?;
                let rest = section.remaining_bytes();
                custom_sections.push(json!({
                    "name": name,
                    "size": rest.len(),
                    "digest": format!("{:016x}", fnv1a64(rest)),
                }));
            }
            1 => {
                let count = section.uleb()?;
                for _ in 0..count {
                    let form = section.u8()?;
                    if form != 0x60 {
                        return Err(format!("unsupported WebAssembly type form {form:#x}"));
                    }
                    let params = section.valtypes()?;
                    let results = section.valtypes()?;
                    types.push(json!({"params": params, "results": results}));
                }
            }
            2 => {
                let count = section.uleb()?;
                for _ in 0..count {
                    let module = section.name()?;
                    let name = section.name()?;
                    let kind = section.u8()?;
                    let mut import = Map::new();
                    import.insert("module".to_string(), Value::String(module));
                    import.insert("name".to_string(), Value::String(name));
                    match kind {
                        0 => {
                            import.insert("kind".to_string(), Value::String("function".into()));
                            import.insert("type".to_string(), Value::from(section.uleb()?));
                        }
                        1 => {
                            import.insert("kind".to_string(), Value::String("table".into()));
                            section.u8()?;
                            let limits = section.limits()?;
                            import.insert("limits".to_string(), limits);
                        }
                        2 => {
                            import.insert("kind".to_string(), Value::String("memory".into()));
                            import.insert("limits".to_string(), section.limits()?);
                        }
                        3 => {
                            import.insert("kind".to_string(), Value::String("global".into()));
                            import.insert("type".to_string(), Value::String(section.valtype()?));
                            import.insert("mutable".to_string(), Value::Bool(section.u8()? != 0));
                        }
                        4 => {
                            import.insert("kind".to_string(), Value::String("tag".into()));
                            section.u8()?;
                            import.insert("type".to_string(), Value::from(section.uleb()?));
                        }
                        other => {
                            return Err(format!("unsupported WebAssembly import kind {other}"));
                        }
                    }
                    imports.push(Value::Object(import));
                }
            }
            3 => {
                let count = section.uleb()?;
                for _ in 0..count {
                    functions.push(Value::from(section.uleb()?));
                }
            }
            4 => {
                let count = section.uleb()?;
                for _ in 0..count {
                    section.u8()?;
                    tables.push(section.limits()?);
                }
            }
            5 => {
                let count = section.uleb()?;
                for _ in 0..count {
                    memories.push(section.limits()?);
                }
            }
            6 => {
                let count = section.uleb()?;
                for _ in 0..count {
                    let valtype = section.valtype()?;
                    let mutable = section.u8()? != 0;
                    globals.push(json!({"type": valtype, "mutable": mutable}));
                }
            }
            7 => {
                let count = section.uleb()?;
                for _ in 0..count {
                    let name = section.name()?;
                    let kind = section.u8()?;
                    let index = section.uleb()?;
                    exports.push(json!({
                        "name": name,
                        "kind": export_kind(kind)?,
                        "index": index,
                    }));
                }
            }
            8 => {
                start = Some(Value::from(section.uleb()?));
            }
            9 => {
                let digest = format!("{:016x}", fnv1a64(payload));
                element_section = Some(json!({"size": payload.len(), "digest": digest}));
            }
            10 => {
                let count = section.uleb()?;
                for _ in 0..count {
                    let size = section.uleb()? as usize;
                    let body = section.bytes(size)?;
                    code.push(json!({
                        "size": size,
                        "digest": format!("{:016x}", fnv1a64(body)),
                    }));
                }
            }
            11 => {
                data_section = Some(json!({
                    "size": payload.len(),
                    "digest": format!("{:016x}", fnv1a64(payload)),
                }));
            }
            12 => {
                // Data count section: only meaningful for bulk-memory checks.
                section.uleb()?;
            }
            _ => {}
        }
    }

    exports.sort_by(|left, right| {
        left["name"]
            .as_str()
            .cmp(&right["name"].as_str())
            .then(left["kind"].as_str().cmp(&right["kind"].as_str()))
    });
    custom_sections.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));

    Ok(json!({
        "format": "wasm",
        "version": version,
        "types": types,
        "imports": imports,
        "functions": functions,
        "tables": tables,
        "memories": memories,
        "globals": globals,
        "exports": exports,
        "start": start,
        "code": code,
        "custom_sections": custom_sections,
        "data": data_section,
        "elements": element_section,
    }))
}

fn export_kind(kind: u8) -> Result<&'static str, String> {
    Ok(match kind {
        0 => "function",
        1 => "table",
        2 => "memory",
        3 => "global",
        4 => "tag",
        other => return Err(format!("unsupported WebAssembly export kind {other}")),
    })
}

fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

struct Reader<'a> {
    data: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.position)
    }

    fn remaining_bytes(&self) -> &'a [u8] {
        &self.data[self.position..]
    }

    fn u8(&mut self) -> Result<u8, String> {
        let byte = *self
            .data
            .get(self.position)
            .ok_or_else(|| "truncated WebAssembly".to_string())?;
        self.position += 1;
        Ok(byte)
    }

    fn bytes(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| "WebAssembly size overflow".to_string())?;
        let slice = self
            .data
            .get(self.position..end)
            .ok_or_else(|| "truncated WebAssembly".to_string())?;
        self.position = end;
        Ok(slice)
    }

    fn uleb(&mut self) -> Result<u32, String> {
        let mut value = 0u32;
        let mut shift = 0;
        loop {
            let byte = self.u8()?;
            value |= u32::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
            shift += 7;
            if shift >= 35 {
                return Err("invalid WebAssembly LEB128".to_string());
            }
        }
    }

    fn name(&mut self) -> Result<String, String> {
        let length = self.uleb()? as usize;
        let bytes = self.bytes(length)?;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }

    fn valtypes(&mut self) -> Result<Vec<Value>, String> {
        let count = self.uleb()?;
        let mut value = Vec::with_capacity(count as usize);
        for _ in 0..count {
            value.push(Value::String(self.valtype()?));
        }
        Ok(value)
    }

    fn valtype(&mut self) -> Result<String, String> {
        let byte = self.u8()?;
        Ok(match byte {
            0x7f => "i32",
            0x7e => "i64",
            0x7d => "f32",
            0x7c => "f64",
            0x7b => "v128",
            0x70 => "funcref",
            0x6f => "externref",
            0x6e => "anyref",
            0x6d => "eqref",
            0x6c => "i31ref",
            0x6b => "structref",
            0x6a => "arrayref",
            other => return Ok(format!("0x{other:02x}")),
        }
        .to_string())
    }

    fn limits(&mut self) -> Result<Value, String> {
        let flags = self.u8()?;
        let min = self.uleb()?;
        let max = if flags & 1 != 0 {
            Some(self.uleb()?)
        } else {
            None
        };
        Ok(json!({"min": min, "max": max}))
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
    fn parses_exports_functions_and_code() {
        let bytes = std::fs::read(fixture("wasm/old.wasm")).unwrap();
        let value = parse_wasm(&bytes).unwrap();
        assert_eq!(value["format"], "wasm");
        assert_eq!(value["version"], 1);
        assert!(
            value["exports"]
                .as_array()
                .unwrap()
                .iter()
                .any(|export| export["name"] == "add")
        );
        assert!(!value["code"].as_array().unwrap().is_empty());
    }

    #[test]
    fn rejects_truncated_binary() {
        let bytes = std::fs::read(fixture("wasm/old.wasm")).unwrap();
        assert!(parse_wasm(&bytes[..12]).is_err());
    }
}
