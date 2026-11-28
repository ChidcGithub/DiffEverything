use encoding_rs::Encoding;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;

const MAGIC: u32 = 0x9504_12de;

/// Parse a GNU gettext `.mo` message catalog into a deterministic JSON model.
///
/// The catalog header (the empty msgid entry) is exposed as `headers`, while
/// regular entries are sorted by context/msgid/plural so catalog ordering does
/// not create noise. Strings are decoded using the catalog's declared charset.
pub fn parse_mo(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() < 20 {
        return Err("truncated gettext header".to_string());
    }
    let little = if u32::from_le_bytes(bytes[0..4].try_into().unwrap()) == MAGIC {
        true
    } else if u32::from_be_bytes(bytes[0..4].try_into().unwrap()) == MAGIC {
        false
    } else {
        return Err("invalid gettext magic".to_string());
    };
    let revision = read_u32(bytes, 4, little)?;
    let count = read_u32(bytes, 8, little)? as usize;
    let originals_offset = read_u32(bytes, 12, little)? as usize;
    let translations_offset = read_u32(bytes, 16, little)? as usize;
    let originals_end = originals_offset
        .checked_add(count.saturating_mul(8))
        .ok_or_else(|| "gettext original table overflow".to_string())?;
    let translations_end = translations_offset
        .checked_add(count.saturating_mul(8))
        .ok_or_else(|| "gettext translation table overflow".to_string())?;
    if originals_end > bytes.len() || translations_end > bytes.len() {
        return Err("truncated gettext string table".to_string());
    }

    let mut raw_entries = Vec::with_capacity(count);
    for index in 0..count {
        let original = read_table_entry(bytes, originals_offset, index, little)?;
        let translation = read_table_entry(bytes, translations_offset, index, little)?;
        raw_entries.push((original, translation));
    }

    // The header is the entry with an empty msgid. Parse it first so we can
    // decode every other string with the declared charset.
    let mut headers = BTreeMap::new();
    let mut charset_label = "utf-8".to_string();
    let mut header_index = None;
    for (index, (original, translation)) in raw_entries.iter().enumerate() {
        if original.0 == 0 {
            let header_text = decode_bytes(translation, encoding_rs::UTF_8);
            headers = parse_headers(&header_text);
            if let Some(label) = declared_charset(&headers) {
                charset_label = label.to_ascii_lowercase();
            }
            header_index = Some(index);
            break;
        }
    }
    let encoding = Encoding::for_label(charset_label.as_bytes())
        .or_else(|| Encoding::for_label(b"utf-8"))
        .unwrap_or(encoding_rs::UTF_8);
    if let Some(index) = header_index {
        let header_text = decode_bytes(&raw_entries[index].1, encoding);
        headers = parse_headers(&header_text);
    }

    let mut messages = Vec::new();
    for (original, translation) in &raw_entries {
        if original.0 == 0 {
            continue;
        }
        let original = decode_bytes(original, encoding);
        let translation = decode_bytes(translation, encoding);
        let (context, original) = match original.split_once('\u{4}') {
            Some((context, original)) => (Some(context.to_string()), original),
            None => (None, original.as_str()),
        };
        let mut parts = original.split('\0');
        let msgid = parts.next().unwrap_or_default().to_string();
        let plural = parts.collect::<Vec<_>>().join("\u{0}");
        let msgstr: Vec<Value> = translation
            .split('\0')
            .map(|part| Value::String(part.to_string()))
            .collect();
        let mut message = Map::new();
        if let Some(context) = context {
            message.insert("context".to_string(), Value::String(context));
        }
        message.insert("msgid".to_string(), Value::String(msgid.clone()));
        if !plural.is_empty() {
            message.insert("msgid_plural".to_string(), Value::String(plural.clone()));
        }
        message.insert("msgstr".to_string(), Value::Array(msgstr));
        let key = format!(
            "{}\u{0}{}\u{0}{}",
            message.get("context").and_then(Value::as_str).unwrap_or(""),
            msgid,
            plural
        );
        messages.push((key, Value::Object(message)));
    }
    messages.sort_by(|left, right| left.0.cmp(&right.0));

    let headers_value = Value::Object(
        headers
            .into_iter()
            .map(|(key, value)| (key, Value::String(value)))
            .collect(),
    );
    Ok(json!({
        "format": "gettext",
        "revision": revision,
        "charset": charset_label,
        "headers": headers_value,
        "messages": messages.into_iter().map(|(_, value)| value).collect::<Vec<_>>(),
    }))
}

fn read_u32(bytes: &[u8], offset: usize, little: bool) -> Result<u32, String> {
    let raw = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "truncated gettext integer".to_string())?;
    let value = <[u8; 4]>::try_from(raw).unwrap();
    Ok(if little {
        u32::from_le_bytes(value)
    } else {
        u32::from_be_bytes(value)
    })
}

fn read_table_entry(
    bytes: &[u8],
    table_offset: usize,
    index: usize,
    little: bool,
) -> Result<(usize, &[u8]), String> {
    let entry = table_offset + index * 8;
    let length = read_u32(bytes, entry, little)? as usize;
    let offset = read_u32(bytes, entry + 4, little)? as usize;
    let end = offset
        .checked_add(length)
        .ok_or_else(|| "gettext string overflow".to_string())?;
    let data = bytes
        .get(offset..end)
        .ok_or_else(|| "truncated gettext string".to_string())?;
    Ok((length, data))
}

fn decode_bytes(entry: &(usize, &[u8]), encoding: &'static Encoding) -> String {
    let (_, data) = entry;
    let (text, _, _) = encoding.decode(data);
    text.into_owned()
}

fn parse_headers(text: &str) -> BTreeMap<String, String> {
    let mut headers = BTreeMap::new();
    let mut current: Option<String> = None;
    for line in text.split('\n') {
        let line = line.trim_end_matches('\r');
        if line.is_empty() {
            continue;
        }
        if (line.starts_with(' ') || line.starts_with('\t'))
            && let Some(key) = current.as_deref()
        {
            let value = headers.entry(key.to_string()).or_insert_with(String::new);
            if !value.is_empty() {
                value.push('\n');
            }
            value.push_str(line.trim_start());
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim().to_string();
            headers.insert(key.clone(), value.trim().to_string());
            current = Some(key);
        }
    }
    headers
}

fn declared_charset(headers: &BTreeMap<String, String>) -> Option<String> {
    let content_type = headers.get("Content-Type")?;
    let lower = content_type.to_ascii_lowercase();
    let start = lower.find("charset=")? + "charset=".len();
    let label = content_type[start..]
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches('"')
        .trim_matches('\\')
        .trim();
    if label.is_empty() {
        None
    } else {
        Some(label.to_string())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/diff/gettext.rs"]
mod tests;
