use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::{Field, Row};
use serde_json::{Map, Number, Value, json};
use sha2::{Digest, Sha256};

use crate::diff::serialization::parse_protobuf;

const MAX_ROWS: usize = 2000;

/// Parse a Parquet file into a JSON object containing the schema and rows.
pub fn parse_parquet(bytes: &[u8]) -> Result<Value, String> {
    let reader = SerializedFileReader::new(bytes::Bytes::copy_from_slice(bytes))
        .map_err(|error| format!("Parquet: {error}"))?;
    let schema = format!("{:?}", reader.metadata().file_metadata().schema());
    let mut rows = Vec::new();
    let mut truncated = false;
    let iter = reader
        .get_row_iter(None)
        .map_err(|error| format!("Parquet rows: {error}"))?;
    for row in iter {
        if rows.len() >= MAX_ROWS {
            truncated = true;
            break;
        }
        let row = row.map_err(|error| format!("Parquet row: {error}"))?;
        rows.push(row_to_json(&row));
    }
    Ok(json!({
        "schema": schema,
        "rows": rows,
        "rows_truncated": truncated,
    }))
}

fn row_to_json(row: &Row) -> Value {
    let mut map = Map::new();
    for (name, field) in row.get_column_iter() {
        map.insert(name.clone(), field_to_json(field));
    }
    Value::Object(map)
}

fn field_to_json(field: &Field) -> Value {
    match field {
        Field::Null => Value::Null,
        Field::Bool(value) => Value::Bool(*value),
        Field::Byte(value) => json!(value),
        Field::Short(value) => json!(value),
        Field::Int(value) => json!(value),
        Field::Long(value) => json!(value),
        Field::UByte(value) => json!(value),
        Field::UShort(value) => json!(value),
        Field::UInt(value) => json!(value),
        Field::ULong(value) => json!(value),
        Field::Float16(value) => Number::from_f64(value.to_f64())
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Field::Float(value) => Number::from_f64(f64::from(*value))
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Field::Double(value) => Number::from_f64(*value)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Field::Decimal(value) => Value::String(format!("{value:?}")),
        Field::Str(value) => Value::String(value.clone()),
        Field::Bytes(value) => json!({"$bytes": BASE64.encode(value.data())}),
        Field::Date(value) => json!({"$date_days": value}),
        Field::TimeMillis(value) => json!({"$time_millis": value}),
        Field::TimeMicros(value) => json!({"$time_micros": value}),
        Field::TimestampMillis(value) => json!({"$timestamp_millis": value}),
        Field::TimestampMicros(value) => json!({"$timestamp_micros": value}),
        Field::Group(row) => row_to_json(row),
        Field::ListInternal(list) => {
            Value::Array(list.elements().iter().map(field_to_json).collect())
        }
        Field::MapInternal(map) => Value::Array(
            map.entries()
                .iter()
                .map(|(key, value)| json!({"key": field_to_json(key), "value": field_to_json(value)}))
                .collect(),
        ),
    }
}

/// Parse an Avro container file into a JSON object containing the writer
/// schema and decoded records.
pub fn parse_avro(bytes: &[u8]) -> Result<Value, String> {
    let mut reader = apache_avro::Reader::new(bytes).map_err(|error| format!("Avro: {error}"))?;
    let schema = serde_json::to_value(reader.writer_schema())
        .unwrap_or_else(|_| Value::String(format!("{:?}", reader.writer_schema())));
    let mut rows = Vec::new();
    let mut truncated = false;
    for value in reader.by_ref() {
        if rows.len() >= MAX_ROWS {
            truncated = true;
            break;
        }
        let value = value.map_err(|error| format!("Avro record: {error}"))?;
        let row = apache_avro::from_value::<serde_json::Value>(&value)
            .map_err(|error| format!("Avro JSON: {error}"))?;
        rows.push(row);
    }
    Ok(json!({
        "schema": schema,
        "rows": rows,
        "rows_truncated": truncated,
    }))
}

/// Compare ORC metadata and stripe payloads without decoding every column.
///
/// ORC's footer and metadata are protobuf messages. They contain the schema,
/// row counts, stripe statistics and writer information; the stripe region
/// itself is hashed so row-data changes are still visible. This keeps the
/// implementation dependency-free and comfortably bounded for large files.
pub fn parse_orc(bytes: &[u8]) -> Result<Value, String> {
    if !bytes.starts_with(b"ORC") {
        return Err("ORC: missing ORC magic".to_string());
    }
    let postscript_length = *bytes.last().ok_or_else(|| "ORC: empty file".to_string())? as usize;
    if bytes.len() < postscript_length + 4 {
        return Err("ORC: truncated postscript".to_string());
    }
    let postscript_start = bytes.len() - 1 - postscript_length;
    let postscript = parse_protobuf(&bytes[postscript_start..bytes.len() - 1])
        .map_err(|error| format!("ORC postscript: {error}"))?;
    let compression = field_u64(&postscript, "field_2").unwrap_or(0);
    let footer_length = field_u64(&postscript, "field_1")
        .ok_or_else(|| "ORC: postscript has no footer length".to_string())?
        as usize;
    let metadata_length = field_u64(&postscript, "field_5")
        .ok_or_else(|| "ORC: postscript has no metadata length".to_string())?
        as usize;
    let stripe_statistics_length = field_u64(&postscript, "field_7").unwrap_or(0) as usize;
    let footer_start = postscript_start
        .checked_sub(footer_length)
        .ok_or_else(|| "ORC: footer length exceeds file".to_string())?;
    let metadata_start = footer_start
        .checked_sub(metadata_length)
        .ok_or_else(|| "ORC: metadata length exceeds file".to_string())?;
    let stripe_end = metadata_start
        .checked_sub(stripe_statistics_length)
        .unwrap_or(metadata_start);
    let footer_bytes = decompress_orc(&bytes[footer_start..postscript_start], compression)?;
    let metadata_bytes = decompress_orc(&bytes[metadata_start..footer_start], compression)?;
    let footer = parse_protobuf(&footer_bytes).map_err(|error| format!("ORC footer: {error}"))?;
    let metadata =
        parse_protobuf(&metadata_bytes).map_err(|error| format!("ORC metadata: {error}"))?;
    let stripe_region = &bytes[3..stripe_end];
    let stripe_hash = hex(&Sha256::digest(stripe_region));
    Ok(json!({
        "postscript": rename_fields(postscript, &[
            ("field_1", "footer_length"),
            ("field_2", "compression"),
            ("field_3", "compression_block_size"),
            ("field_4", "version"),
            ("field_5", "metadata_length"),
            ("field_6", "writer_version"),
            ("field_7", "stripe_statistics_length"),
            ("field_8000", "magic"),
        ]),
        "footer": rename_fields(footer, &[
            ("field_1", "header_length"),
            ("field_2", "content_length"),
            ("field_3", "stripes"),
            ("field_4", "types"),
            ("field_5", "metadata"),
            ("field_6", "number_of_rows"),
            ("field_7", "statistics"),
            ("field_8", "row_index_stride"),
            ("field_9", "writer"),
            ("field_10", "encryption"),
            ("field_11", "calendar"),
        ]),
        "metadata": rename_fields(metadata, &[
            ("field_1", "stripe_statistics"),
        ]),
        "compression_kind": compression,
        "stripe_region_bytes": stripe_region.len(),
        "stripe_region_sha256": stripe_hash,
        "file_size": bytes.len(),
    }))
}

fn field_u64(value: &Value, key: &str) -> Option<u64> {
    value.get(key)?.as_u64()
}

fn decompress_orc(data: &[u8], compression: u64) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    if compression == 0 {
        return Ok(data.to_vec());
    }
    let mut output = Vec::new();
    let mut offset = 0usize;
    while offset < data.len() {
        if offset + 3 > data.len() {
            return Err("ORC: truncated compression header".to_string());
        }
        let header = u32::from(data[offset])
            | (u32::from(data[offset + 1]) << 8)
            | (u32::from(data[offset + 2]) << 16);
        offset += 3;
        let original = header & 1 != 0;
        let length = (header >> 1) as usize;
        if offset + length > data.len() {
            return Err("ORC: truncated compression block".to_string());
        }
        let block = &data[offset..offset + length];
        offset += length;
        if original {
            output.extend_from_slice(block);
            continue;
        }
        match compression {
            1 => {
                let mut decoded = Vec::new();
                flate2::read::DeflateDecoder::new(block)
                    .read_to_end(&mut decoded)
                    .map_err(|error| format!("ORC zlib: {error}"))?;
                output.extend_from_slice(&decoded);
            }
            2 => {
                let decoded = snap::raw::Decoder::new()
                    .decompress_vec(block)
                    .map_err(|error| format!("ORC snappy: {error}"))?;
                output.extend_from_slice(&decoded);
            }
            5 => {
                let decoded = zstd::stream::decode_all(block)
                    .map_err(|error| format!("ORC zstd: {error}"))?;
                output.extend_from_slice(&decoded);
            }
            other => return Err(format!("ORC: unsupported compression kind {other}")),
        }
    }
    Ok(output)
}

fn rename_fields(value: Value, mapping: &[(&str, &str)]) -> Value {
    let Value::Object(mut object) = value else {
        return value;
    };
    for (old, new) in mapping {
        if let Some(value) = object.remove(*old) {
            object.insert((*new).to_string(), value);
        }
    }
    Value::Object(object)
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
#[path = "../../tests/unit/diff/tabular.rs"]
mod tests;
