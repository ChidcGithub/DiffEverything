use serde_json::{Map, Value, json};

const MAGIC: u32 = 0xcafe_babe;

/// Parse a JVM `.class` file into a deterministic semantic model.
///
/// The model exposes the class hierarchy, fields, methods, access flags and
/// each method's bytecode digest. Constant-pool layout and attribute padding
/// are intentionally not part of the model, so harmless compiler layout
/// changes do not create noise.
pub fn parse_class(bytes: &[u8]) -> Result<Value, String> {
    let mut reader = Reader::new(bytes);
    if reader.u32()? != MAGIC {
        return Err("invalid Java class magic".to_string());
    }
    let minor_version = reader.u16()?;
    let major_version = reader.u16()?;
    let constant_pool_count = reader.u16()? as usize;
    if constant_pool_count == 0 {
        return Err("empty Java constant pool".to_string());
    }

    let mut constant_pool = vec![None; constant_pool_count];
    let mut index = 1;
    while index < constant_pool_count {
        let tag = reader.u8()?;
        match tag {
            1 => {
                let length = reader.u16()? as usize;
                let text = String::from_utf8_lossy(reader.bytes(length)?).into_owned();
                constant_pool[index] = Some(CpEntry::Utf8(text));
            }
            7 => {
                let name_index = reader.u16()?;
                constant_pool[index] = Some(CpEntry::Class(name_index));
            }
            12 | 9 | 10 | 11 | 17 | 18 => {
                reader.bytes(4)?;
                constant_pool[index] = Some(CpEntry::Other);
            }
            3 | 4 => {
                reader.bytes(4)?;
                constant_pool[index] = Some(CpEntry::Other);
            }
            5 | 6 => {
                reader.bytes(8)?;
                constant_pool[index] = Some(CpEntry::Other);
                index += 1;
            }
            8 | 16 | 19 | 20 => {
                reader.bytes(2)?;
                constant_pool[index] = Some(CpEntry::Other);
            }
            15 => {
                reader.bytes(3)?;
                constant_pool[index] = Some(CpEntry::Other);
            }
            other => return Err(format!("unsupported Java constant pool tag {other}")),
        }
        index += 1;
    }

    let access_flags = reader.u16()?;
    let this_class = reader.u16()?;
    let super_class = reader.u16()?;
    let interface_count = reader.u16()?;
    let mut interfaces = Vec::with_capacity(interface_count as usize);
    for _ in 0..interface_count {
        if let Some(name) = class_name(&constant_pool, reader.u16()?) {
            interfaces.push(Value::String(name));
        }
    }

    let field_count = reader.u16()?;
    let mut fields = Vec::with_capacity(field_count as usize);
    for _ in 0..field_count {
        let access = reader.u16()?;
        let name = cp_utf8(&constant_pool, reader.u16()?)?;
        let descriptor = cp_utf8(&constant_pool, reader.u16()?)?;
        skip_attributes(&mut reader)?;
        fields.push(json!({
            "name": name,
            "descriptor": descriptor,
            "access_flags": access,
        }));
    }

    let method_count = reader.u16()?;
    let mut methods = Vec::with_capacity(method_count as usize);
    for _ in 0..method_count {
        let access = reader.u16()?;
        let name = cp_utf8(&constant_pool, reader.u16()?)?;
        let descriptor = cp_utf8(&constant_pool, reader.u16()?)?;
        let attribute_count = reader.u16()?;
        let mut code = Value::Null;
        for _ in 0..attribute_count {
            let attribute_name = cp_utf8(&constant_pool, reader.u16()?)?;
            let length = reader.u32()? as usize;
            let payload = reader.bytes(length)?;
            if attribute_name == "Code" {
                let mut code_reader = Reader::new(payload);
                code_reader.u16()?; // max_stack
                code_reader.u16()?; // max_locals
                let code_length = code_reader.u32()? as usize;
                let bytecode = code_reader.bytes(code_length)?;
                code = json!({
                    "size": code_length,
                    "digest": format!("{:016x}", fnv1a64(bytecode)),
                });
            }
        }
        methods.push(json!({
            "name": name,
            "descriptor": descriptor,
            "access_flags": access,
            "code": code,
        }));
    }

    fields.sort_by(|left, right| {
        left["name"].as_str().cmp(&right["name"].as_str()).then(
            left["descriptor"]
                .as_str()
                .cmp(&right["descriptor"].as_str()),
        )
    });
    methods.sort_by(|left, right| {
        left["name"].as_str().cmp(&right["name"].as_str()).then(
            left["descriptor"]
                .as_str()
                .cmp(&right["descriptor"].as_str()),
        )
    });

    let mut root = Map::new();
    root.insert("format".to_string(), Value::String("java-class".into()));
    root.insert("minor_version".to_string(), Value::from(minor_version));
    root.insert("major_version".to_string(), Value::from(major_version));
    root.insert("access_flags".to_string(), Value::from(access_flags));
    root.insert(
        "class".to_string(),
        class_name(&constant_pool, this_class)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    root.insert(
        "super".to_string(),
        class_name(&constant_pool, super_class)
            .map(Value::String)
            .unwrap_or(Value::Null),
    );
    root.insert("interfaces".to_string(), Value::Array(interfaces));
    root.insert("fields".to_string(), Value::Array(fields));
    root.insert("methods".to_string(), Value::Array(methods));
    root.insert(
        "constant_pool_size".to_string(),
        Value::from(constant_pool_count),
    );
    Ok(Value::Object(root))
}

fn skip_attributes(reader: &mut Reader<'_>) -> Result<(), String> {
    let count = reader.u16()?;
    for _ in 0..count {
        reader.u16()?;
        let length = reader.u32()? as usize;
        reader.bytes(length)?;
    }
    Ok(())
}

fn cp_utf8(pool: &[Option<CpEntry>], index: u16) -> Result<String, String> {
    match pool.get(index as usize).and_then(Option::as_ref) {
        Some(CpEntry::Utf8(text)) => Ok(text.clone()),
        _ => Err(format!("invalid Java UTF-8 constant #{index}")),
    }
}

fn class_name(pool: &[Option<CpEntry>], index: u16) -> Option<String> {
    if index == 0 {
        return None;
    }
    let name_index = match pool.get(index as usize).and_then(Option::as_ref) {
        Some(CpEntry::Class(name_index)) => *name_index,
        _ => return None,
    };
    cp_utf8(pool, name_index)
        .ok()
        .map(|name| name.replace('/', "."))
}

fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[derive(Clone)]
enum CpEntry {
    Utf8(String),
    Class(u16),
    Other,
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
            .ok_or_else(|| "truncated Java class".to_string())?;
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
            .ok_or_else(|| "Java class size overflow".to_string())?;
        let slice = self
            .data
            .get(self.position..end)
            .ok_or_else(|| "truncated Java class".to_string())?;
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
    fn parses_class_methods_and_code() {
        let bytes = std::fs::read(fixture("java/Test1.class")).unwrap();
        let value = parse_class(&bytes).unwrap();
        assert_eq!(value["format"], "java-class");
        assert_eq!(value["major_version"], 50);
        assert_eq!(value["class"], "Test");
        let methods = value["methods"].as_array().unwrap();
        assert_eq!(methods.len(), 2);
        let main = methods
            .iter()
            .find(|method| method["name"] == "main")
            .expect("main method");
        assert_eq!(main["code"]["size"], 3);
    }

    #[test]
    fn rejects_truncated_class() {
        let bytes = std::fs::read(fixture("java/Test1.class")).unwrap();
        assert!(parse_class(&bytes[..16]).is_err());
    }
}
