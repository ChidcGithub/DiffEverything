use serde_json::{Map, Value, json};

/// Parse an ICO/CUR container into a semantic image model.
///
/// Each directory entry is described by its declared size, bit depth, embedded
/// format (PNG or BMP/DIB) and payload digest. Pixel-level comparison for PNG
/// entries is still handled by the image comparator when ICO is used as an
/// archive; this model focuses on the container metadata.
pub fn parse_ico(bytes: &[u8]) -> Result<Value, String> {
    if bytes.len() < 6 {
        return Err("truncated ICO header".to_string());
    }
    let reserved = u16_at(bytes, 0)?;
    let image_type = u16_at(bytes, 2)?;
    let count = u16_at(bytes, 4)? as usize;
    if reserved != 0 || !(1..=2).contains(&image_type) {
        return Err("invalid ICO header".to_string());
    }
    let directory_end = 6usize
        .checked_add(count.saturating_mul(16))
        .ok_or_else(|| "ICO directory overflow".to_string())?;
    if directory_end > bytes.len() {
        return Err("truncated ICO directory".to_string());
    }

    let mut images = Vec::with_capacity(count);
    for index in 0..count {
        let entry = 6 + index * 16;
        let width = match bytes[entry] {
            0 => 256,
            value => u32::from(value),
        };
        let height = match bytes[entry + 1] {
            0 => 256,
            value => u32::from(value),
        };
        let color_count = bytes[entry + 2];
        let planes = u16_at(bytes, entry + 4)?;
        let bit_count = u16_at(bytes, entry + 6)?;
        let size = u32_at(bytes, entry + 8)? as usize;
        let offset = u32_at(bytes, entry + 12)? as usize;
        let end = offset
            .checked_add(size)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| "ICO image data is out of range".to_string())?;
        let data = &bytes[offset..end];

        let embedded_format = if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            "png"
        } else if data.len() >= 12 && u32::from_le_bytes([data[0], data[1], data[2], data[3]]) >= 12
        {
            "bmp"
        } else {
            "raw"
        };
        let mut image = Map::new();
        image.insert("width".to_string(), Value::from(width));
        image.insert("height".to_string(), Value::from(height));
        image.insert("color_count".to_string(), Value::from(color_count));
        image.insert("planes".to_string(), Value::from(planes));
        image.insert("bit_count".to_string(), Value::from(bit_count));
        image.insert("format".to_string(), Value::String(embedded_format.into()));
        image.insert("size".to_string(), Value::from(size));
        image.insert(
            "digest".to_string(),
            Value::String(format!("{:016x}", fnv1a64(data))),
        );
        if embedded_format == "bmp" && data.len() >= 20 {
            let dib_width = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
            let dib_height = i32::from_le_bytes([data[8], data[9], data[10], data[11]]);
            image.insert("dib_width".to_string(), Value::from(dib_width));
            image.insert(
                "dib_height".to_string(),
                Value::from(if dib_height > 0 {
                    dib_height / 2
                } else {
                    dib_height
                }),
            );
            image.insert(
                "dib_bit_count".to_string(),
                Value::from(u16::from_le_bytes([data[14], data[15]])),
            );
        }
        images.push(Value::Object(image));
    }
    images.sort_by(|left, right| {
        left["width"]
            .as_u64()
            .cmp(&right["width"].as_u64())
            .then(left["height"].as_u64().cmp(&right["height"].as_u64()))
            .then(left["bit_count"].as_u64().cmp(&right["bit_count"].as_u64()))
    });

    Ok(json!({
        "format": "ico",
        "type": if image_type == 1 { "icon" } else { "cursor" },
        "image_count": count,
        "images": images,
    }))
}

fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| "truncated ICO".to_string())?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| "truncated ICO".to_string())?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
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
    fn parses_icon_directory() {
        let bytes = std::fs::read(fixture("images/diffoscope-test1.ico")).unwrap();
        let value = parse_ico(&bytes).unwrap();
        assert_eq!(value["format"], "ico");
        assert_eq!(value["type"], "icon");
        assert_eq!(value["image_count"], 1);
        assert_eq!(value["images"][0]["width"], 16);
        assert_eq!(value["images"][0]["bit_count"], 0);
        assert_eq!(value["images"][0]["color_count"], 16);
        assert_eq!(value["images"][0]["dib_bit_count"], 4);
        assert_eq!(value["images"][0]["format"], "bmp");
        assert_eq!(value["images"][0]["dib_height"], 16);
    }

    #[test]
    fn rejects_truncated_ico() {
        let bytes = std::fs::read(fixture("images/diffoscope-test1.ico")).unwrap();
        assert!(parse_ico(&bytes[..10]).is_err());
    }
}
