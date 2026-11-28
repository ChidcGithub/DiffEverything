use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

const RES_XML_TYPE: u16 = 0x0003;
const RES_STRING_POOL_TYPE: u16 = 0x0001;
const RES_XML_START_NAMESPACE_TYPE: u16 = 0x0100;
const RES_XML_START_ELEMENT_TYPE: u16 = 0x0102;
const RES_XML_END_ELEMENT_TYPE: u16 = 0x0103;
const NO_INDEX: u32 = u32::MAX;

/// Parse Android binary XML (`AndroidManifest.xml`) into a semantic tree.
///
/// Elements, namespaces and attributes are exposed as JSON; typed values are
/// formatted as strings so two manifests can be compared without depending on
/// the binary string-pool layout.
pub fn parse_android_xml(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() < 8 || u16::from_le_bytes([bytes[0], bytes[1]]) != RES_XML_TYPE {
        return Err("invalid Android binary XML header".to_string());
    }
    let mut strings: Vec<String> = Vec::new();
    let mut prefix_by_uri: BTreeMap<u32, String> = BTreeMap::new();
    let mut namespaces: BTreeMap<String, String> = BTreeMap::new();
    let mut stack: Vec<Map<String, Value>> = Vec::new();
    let mut roots: Vec<Value> = Vec::new();

    let mut position = 8usize;
    while position + 8 <= bytes.len() {
        let chunk_type = u16_at(bytes, position)?;
        let header_size = u16_at(bytes, position + 2)? as usize;
        let size = u32_at(bytes, position + 4)? as usize;
        if size < 8 || position + size > bytes.len() {
            return Err("truncated Android binary XML chunk".to_string());
        }
        let chunk = &bytes[position..position + size];
        match chunk_type {
            RES_STRING_POOL_TYPE => {
                strings = parse_string_pool(chunk)?;
            }
            RES_XML_START_NAMESPACE_TYPE => {
                let prefix_index = u32_at(chunk, 16)?;
                let uri_index = u32_at(chunk, 20)?;
                let prefix = string_at(&strings, prefix_index);
                let uri = string_at(&strings, uri_index);
                if !prefix.is_empty() && !uri.is_empty() {
                    namespaces.insert(prefix, uri);
                    prefix_by_uri.insert(uri_index, string_at(&strings, prefix_index));
                }
            }
            RES_XML_START_ELEMENT_TYPE => {
                if header_size < 16 {
                    return Err("invalid Android element header".to_string());
                }
                let ns_index = u32_at(chunk, 16)?;
                let name_index = u32_at(chunk, 20)?;
                let attribute_start = u16_at(chunk, 24)? as usize;
                let attribute_size = u16_at(chunk, 26)? as usize;
                let attribute_count = u16_at(chunk, 28)? as usize;
                let name = string_at(&strings, name_index);
                if name.is_empty() {
                    return Err("Android element without a name".to_string());
                }
                let mut attributes = Map::new();
                let attributes_start = 16 + attribute_start;
                for index in 0..attribute_count {
                    let offset = attributes_start + index * attribute_size.max(20);
                    if offset + 20 > chunk.len() {
                        return Err("truncated Android attribute".to_string());
                    }
                    let attribute_ns = u32_at(chunk, offset)?;
                    let attribute_name = string_at(&strings, u32_at(chunk, offset + 4)?);
                    let raw_value = u32_at(chunk, offset + 8)?;
                    let value_size = u16_at(chunk, offset + 12)? as usize;
                    if value_size < 8 {
                        continue;
                    }
                    let data_type = chunk[offset + 15];
                    let data = u32_at(chunk, offset + 16)?;
                    let value = if raw_value != NO_INDEX {
                        string_at(&strings, raw_value)
                    } else {
                        typed_value(&strings, data_type, data)
                    };
                    let key = match prefix_by_uri.get(&attribute_ns) {
                        Some(prefix) if !prefix.is_empty() => {
                            format!("{prefix}:{attribute_name}")
                        }
                        _ => attribute_name,
                    };
                    attributes.insert(key, Value::String(value));
                }
                let mut element = Map::new();
                element.insert("name".to_string(), Value::String(name));
                if ns_index != NO_INDEX {
                    element.insert(
                        "namespace".to_string(),
                        Value::String(string_at(&strings, ns_index)),
                    );
                }
                element.insert("attributes".to_string(), Value::Object(attributes));
                element.insert("children".to_string(), Value::Array(Vec::new()));
                stack.push(element);
            }
            RES_XML_END_ELEMENT_TYPE => {
                let Some(element) = stack.pop() else {
                    return Err("unbalanced Android XML element".to_string());
                };
                if let Some(parent) = stack.last_mut() {
                    if let Some(children) = parent.get_mut("children").and_then(Value::as_array_mut)
                    {
                        children.push(Value::Object(element));
                    }
                } else {
                    roots.push(Value::Object(element));
                }
            }
            _ => {}
        }
        position += size;
    }

    if !stack.is_empty() {
        return Err("unclosed Android XML element".to_string());
    }
    let root = match roots.len() {
        0 => return Err("Android XML has no root element".to_string()),
        1 => roots.pop().unwrap(),
        _ => Value::Array(roots),
    };
    Ok(json!({
        "format": "android-xml",
        "namespaces": namespaces,
        "root": root,
    }))
}

fn parse_string_pool(chunk: &[u8]) -> Result<Vec<String>, String> {
    let string_count = u32_at(chunk, 8)? as usize;
    let flags = u32_at(chunk, 16)?;
    let strings_start = u32_at(chunk, 20)? as usize;
    let offset_start = u16_at(chunk, 2)? as usize;
    let utf8 = flags & 0x100 != 0;
    let mut strings = Vec::with_capacity(string_count);
    for index in 0..string_count {
        let offset = u32_at(chunk, offset_start + index * 4)? as usize;
        let data = chunk
            .get(strings_start + offset..)
            .ok_or_else(|| "Android string offset out of range".to_string())?;
        strings.push(if utf8 {
            decode_utf8_string(data)?
        } else {
            decode_utf16_string(data)?
        });
    }
    Ok(strings)
}

fn decode_utf8_string(data: &[u8]) -> Result<String, String> {
    let (_, first) = decode_length(data, 1)?;
    let (length, second) = decode_length(&data[first..], 1)?;
    let start = first + second;
    let end = start
        .checked_add(length)
        .filter(|end| *end <= data.len())
        .ok_or_else(|| "truncated Android UTF-8 string".to_string())?;
    Ok(String::from_utf8_lossy(&data[start..end]).into_owned())
}

fn decode_utf16_string(data: &[u8]) -> Result<String, String> {
    let (length, skip) = decode_length(data, 2)?;
    let start = skip;
    let bytes = length
        .checked_mul(2)
        .and_then(|bytes| data.get(start..start + bytes))
        .ok_or_else(|| "truncated Android UTF-16 string".to_string())?;
    let mut units = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.chunks(2) {
        if let [first, second] = pair {
            units.push(u16::from_le_bytes([*first, *second]));
        }
    }
    Ok(char::decode_utf16(units)
        .map(|value| value.unwrap_or('\u{fffd}'))
        .collect())
}

fn decode_length(data: &[u8], width: usize) -> Result<(usize, usize), String> {
    match width {
        1 => {
            let first = *data
                .first()
                .ok_or_else(|| "truncated Android string length".to_string())?;
            if first & 0x80 != 0 {
                let second = *data
                    .get(1)
                    .ok_or_else(|| "truncated Android string length".to_string())?;
                Ok((((usize::from(first & 0x7f)) << 8) | usize::from(second), 2))
            } else {
                Ok((usize::from(first), 1))
            }
        }
        2 => {
            let first = u16_at(data, 0)?;
            if first & 0x8000 != 0 {
                let second = u16_at(data, 2)?;
                Ok((
                    ((usize::from(first & 0x7fff)) << 16) | usize::from(second),
                    4,
                ))
            } else {
                Ok((usize::from(first), 2))
            }
        }
        _ => Err("invalid Android string width".to_string()),
    }
}

fn typed_value(strings: &[String], data_type: u8, data: u32) -> String {
    match data_type {
        0x00 => "null".to_string(),
        0x01 => format!("@0x{data:08x}"),
        0x02 => format!("?0x{data:08x}"),
        0x03 => string_at(strings, data),
        0x04 => f32::from_bits(data).to_string(),
        0x05 | 0x06 => format!("0x{data:08x}"),
        0x10 => data.to_string(),
        0x11 => format!("0x{data:x}"),
        0x12 => (data != 0).to_string(),
        0x1c..=0x1f => format!("#{data:08x}"),
        _ => format!("0x{data:08x}"),
    }
}

fn string_at(strings: &[String], index: u32) -> String {
    strings.get(index as usize).cloned().unwrap_or_default()
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "truncated Android XML".to_string())?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "truncated Android XML".to_string())?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[cfg(test)]
#[path = "../../tests/unit/diff/android_xml.rs"]
mod tests;
