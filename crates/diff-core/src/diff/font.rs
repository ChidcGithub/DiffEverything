use serde_json::{Map, Value, json};
use std::io::Read as _;
use ttf_parser::name_id;
use ttf_parser::{Face, PlatformId};

pub fn parse_font(bytes: &[u8]) -> Result<Value, String> {
    let owned;
    let sfnt: &[u8] = if bytes.starts_with(b"wOFF") {
        owned = decode_woff(bytes)?;
        &owned
    } else if bytes.starts_with(b"wOF2") {
        owned = woff2_patched::convert_woff2_to_ttf(&mut &bytes[..])
            .map_err(|error| format!("WOFF2: {error}"))?;
        &owned
    } else {
        bytes
    };
    let face = Face::parse(sfnt, 0).map_err(|error| error.to_string())?;
    let mut root = Map::new();
    root.insert(
        "family".to_string(),
        Value::String(font_name(&face, name_id::FAMILY).unwrap_or_default()),
    );
    root.insert(
        "subfamily".to_string(),
        Value::String(font_name(&face, name_id::SUBFAMILY).unwrap_or_default()),
    );
    root.insert(
        "full_name".to_string(),
        Value::String(font_name(&face, name_id::FULL_NAME).unwrap_or_default()),
    );
    root.insert(
        "post_script_name".to_string(),
        Value::String(font_name(&face, name_id::POST_SCRIPT_NAME).unwrap_or_default()),
    );
    root.insert(
        "version".to_string(),
        Value::String(font_name(&face, name_id::VERSION).unwrap_or_default()),
    );
    root.insert("units_per_em".to_string(), Value::from(face.units_per_em()));
    root.insert("ascender".to_string(), Value::from(face.ascender()));
    root.insert("descender".to_string(), Value::from(face.descender()));
    root.insert("line_gap".to_string(), Value::from(face.line_gap()));
    root.insert(
        "number_of_glyphs".to_string(),
        Value::from(face.number_of_glyphs()),
    );
    root.insert("weight".to_string(), Value::from(face.weight().to_number()));
    root.insert("width".to_string(), Value::from(face.width().to_number()));
    root.insert("bold".to_string(), Value::Bool(face.is_bold()));
    root.insert("italic".to_string(), Value::Bool(face.is_italic()));
    root.insert("monospaced".to_string(), Value::Bool(face.is_monospaced()));
    root.insert("italic_angle".to_string(), json!(face.italic_angle()));

    let advances: Vec<u16> = (32u8..=126)
        .map(|code| {
            face.glyph_index(char::from(code))
                .and_then(|glyph| face.glyph_hor_advance(glyph))
                .unwrap_or(0)
        })
        .collect();
    root.insert(
        "ascii_advances".to_string(),
        Value::Array(advances.into_iter().map(Value::from).collect()),
    );
    Ok(Value::Object(root))
}

fn decode_woff(bytes: &[u8]) -> Result<Vec<u8>, String> {
    if bytes.len() < 44 {
        return Err("truncated WOFF header".to_string());
    }
    let flavor = be_u32(bytes, 4)?;
    let table_count = usize::from(be_u16(bytes, 12)?);
    if table_count == 0 {
        return Err("WOFF contains no tables".to_string());
    }
    let mut tables = Vec::with_capacity(table_count);
    for index in 0..table_count {
        let base = 44 + index * 20;
        if base + 20 > bytes.len() {
            return Err("truncated WOFF table directory".to_string());
        }
        let tag = [
            bytes[base],
            bytes[base + 1],
            bytes[base + 2],
            bytes[base + 3],
        ];
        let offset = be_u32(bytes, base + 4)? as usize;
        let compressed_length = be_u32(bytes, base + 8)? as usize;
        let original_length = be_u32(bytes, base + 12)? as usize;
        let checksum = be_u32(bytes, base + 16)?;
        let end = offset
            .checked_add(compressed_length)
            .ok_or_else(|| "WOFF table offset overflow".to_string())?;
        let raw = bytes
            .get(offset..end)
            .ok_or_else(|| "truncated WOFF table data".to_string())?;
        let data = if compressed_length < original_length {
            let mut decoder = flate2::read::ZlibDecoder::new(raw);
            let mut decoded = Vec::with_capacity(original_length);
            decoder
                .read_to_end(&mut decoded)
                .map_err(|error| format!("WOFF zlib: {error}"))?;
            decoded
        } else {
            raw.to_vec()
        };
        if data.len() != original_length {
            return Err("WOFF table size mismatch".to_string());
        }
        tables.push((tag, checksum, original_length as u32, data));
    }
    tables.sort_by_key(|table| table.0);

    let data_start = 12 + table_count * 16;
    let mut offset = data_start;
    let mut offsets = Vec::with_capacity(table_count);
    for (_, _, _, data) in &tables {
        offsets.push(offset as u32);
        offset += (data.len() + 3) & !3;
    }
    let mut sfnt = Vec::with_capacity(offset);
    sfnt.extend_from_slice(&flavor.to_be_bytes());
    sfnt.extend_from_slice(&(table_count as u16).to_be_bytes());
    let mut search_range = 1usize;
    let mut entry_selector = 0u16;
    while search_range * 2 <= table_count {
        search_range *= 2;
        entry_selector += 1;
    }
    search_range *= 16;
    sfnt.extend_from_slice(&(search_range as u16).to_be_bytes());
    sfnt.extend_from_slice(&entry_selector.to_be_bytes());
    sfnt.extend_from_slice(&((table_count * 16 - search_range) as u16).to_be_bytes());
    for (index, (tag, checksum, length, _)) in tables.iter().enumerate() {
        sfnt.extend_from_slice(tag);
        sfnt.extend_from_slice(&checksum.to_be_bytes());
        sfnt.extend_from_slice(&offsets[index].to_be_bytes());
        sfnt.extend_from_slice(&length.to_be_bytes());
    }
    for (_, _, _, data) in &tables {
        sfnt.extend_from_slice(data);
        while sfnt.len() % 4 != 0 {
            sfnt.push(0);
        }
    }
    Ok(sfnt)
}

fn be_u16(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "truncated WOFF header".to_string())?;
    Ok(u16::from_be_bytes([slice[0], slice[1]]))
}

fn be_u32(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "truncated WOFF header".to_string())?;
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn font_name(face: &Face<'_>, wanted: u16) -> Option<String> {
    let mut fallback = None;
    for name in face.names() {
        if name.name_id != wanted || name.name.is_empty() {
            continue;
        }
        let Some(value) = name.to_string() else {
            continue;
        };
        if name.platform_id == PlatformId::Windows && name.language_id == 0x0409 {
            return Some(value);
        }
        if fallback.is_none() {
            fallback = Some(value);
        }
    }
    fallback
}

#[cfg(test)]
#[path = "../../tests/unit/diff/font.rs"]
mod tests;
