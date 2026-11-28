use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Map, Number, Value, json};

/// Decode a CBOR document into a JSON value.
///
/// Byte strings become `{"$bytes": "<base64>"}` and tags become
/// `{"$tag": <number>, "value": ...}` so no information is silently lost.
pub fn parse_cbor(bytes: &[u8]) -> Result<Value, String> {
    let mut reader = CborReader { bytes, pos: 0 };
    let mut values = Vec::new();
    while !reader.is_empty() {
        values.push(reader.value()?);
    }
    match values.len() {
        0 => Err("CBOR input is empty".to_string()),
        1 => Ok(values.remove(0)),
        _ => Ok(Value::Array(values)),
    }
}

struct CborReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl CborReader<'_> {
    fn is_empty(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn byte(&mut self) -> Result<u8, String> {
        let value = *self
            .bytes
            .get(self.pos)
            .ok_or_else(|| "CBOR: unexpected end of input".to_string())?;
        self.pos += 1;
        Ok(value)
    }

    fn take(&mut self, len: usize) -> Result<&[u8], String> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| "CBOR: length overflow".to_string())?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| "CBOR: truncated payload".to_string())?;
        self.pos = end;
        Ok(slice)
    }

    fn argument(&mut self, info: u8) -> Result<Option<u64>, String> {
        Ok(match info {
            0..=23 => Some(u64::from(info)),
            24 => Some(u64::from(self.byte()?)),
            25 => Some(u64::from(u16::from_be_bytes(
                self.take(2)?.try_into().unwrap(),
            ))),
            26 => Some(u64::from(u32::from_be_bytes(
                self.take(4)?.try_into().unwrap(),
            ))),
            27 => Some(u64::from_be_bytes(self.take(8)?.try_into().unwrap())),
            31 => None,
            _ => return Err(format!("CBOR: reserved additional information {info}")),
        })
    }

    fn value(&mut self) -> Result<Value, String> {
        let initial = self.byte()?;
        let major = initial >> 5;
        let info = initial & 0x1f;
        match major {
            0 => {
                let value = self
                    .argument(info)?
                    .ok_or_else(|| "CBOR: indefinite integer".to_string())?;
                Ok(Value::Number(Number::from(value)))
            }
            1 => {
                let value = self
                    .argument(info)?
                    .ok_or_else(|| "CBOR: indefinite negative integer".to_string())?;
                if value <= i64::MAX as u64 {
                    Ok(Value::Number(Number::from(-1i64 - value as i64)))
                } else {
                    Ok(Value::String(format!("-{}", u128::from(value) + 1)))
                }
            }
            2 => self.byte_string(info),
            3 => self.text_string(info),
            4 => self.array(info),
            5 => self.map(info),
            6 => {
                let tag = self
                    .argument(info)?
                    .ok_or_else(|| "CBOR: indefinite tag".to_string())?;
                let value = self.value()?;
                Ok(json!({"$tag": tag, "value": value}))
            }
            7 => self.simple(info),
            _ => Err(format!("CBOR: unsupported major type {major}")),
        }
    }

    fn byte_string(&mut self, info: u8) -> Result<Value, String> {
        let data = match self.argument(info)? {
            Some(len) => self.take(usize::try_from(len).map_err(|_| "CBOR: length overflow")?)?,
            None => {
                let mut data = Vec::new();
                loop {
                    if self.peek() == Some(0xff) {
                        self.pos += 1;
                        break;
                    }
                    if self.byte()? >> 5 != 2 {
                        return Err("CBOR: mixed indefinite byte-string chunks".to_string());
                    }
                    let chunk_info = self.bytes[self.pos - 1] & 0x1f;
                    let len = self
                        .argument(chunk_info)?
                        .ok_or_else(|| "CBOR: nested indefinite byte string".to_string())?;
                    data.extend_from_slice(
                        self.take(usize::try_from(len).map_err(|_| "CBOR: length overflow")?)?,
                    );
                }
                return Ok(json!({"$bytes": BASE64.encode(data)}));
            }
        };
        Ok(json!({"$bytes": BASE64.encode(data)}))
    }

    fn text_string(&mut self, info: u8) -> Result<Value, String> {
        let data = match self.argument(info)? {
            Some(len) => self.take(usize::try_from(len).map_err(|_| "CBOR: length overflow")?)?,
            None => {
                let mut data = Vec::new();
                loop {
                    if self.peek() == Some(0xff) {
                        self.pos += 1;
                        break;
                    }
                    let chunk = self.byte()?;
                    if chunk >> 5 != 3 {
                        return Err("CBOR: mixed indefinite text-string chunks".to_string());
                    }
                    let len = self
                        .argument(chunk & 0x1f)?
                        .ok_or_else(|| "CBOR: nested indefinite text string".to_string())?;
                    data.extend_from_slice(
                        self.take(usize::try_from(len).map_err(|_| "CBOR: length overflow")?)?,
                    );
                }
                return String::from_utf8(data)
                    .map(Value::String)
                    .map_err(|error| format!("CBOR: invalid UTF-8: {error}"));
            }
        };
        String::from_utf8(data.to_vec())
            .map(Value::String)
            .map_err(|error| format!("CBOR: invalid UTF-8: {error}"))
    }

    fn array(&mut self, info: u8) -> Result<Value, String> {
        let mut values = Vec::new();
        match self.argument(info)? {
            Some(len) => {
                for _ in 0..len {
                    values.push(self.value()?);
                }
            }
            None => {
                while self.peek() != Some(0xff) {
                    values.push(self.value()?);
                }
                self.pos += 1;
            }
        }
        Ok(Value::Array(values))
    }

    fn map(&mut self, info: u8) -> Result<Value, String> {
        let mut map = Map::new();
        let read_entry = |reader: &mut Self, map: &mut Map<String, Value>| -> Result<(), String> {
            let key = reader.value()?;
            let value = reader.value()?;
            map.insert(cbor_key(&key), value);
            Ok(())
        };
        match self.argument(info)? {
            Some(len) => {
                for _ in 0..len {
                    read_entry(self, &mut map)?;
                }
            }
            None => {
                while self.peek() != Some(0xff) {
                    read_entry(self, &mut map)?;
                }
                self.pos += 1;
            }
        }
        Ok(Value::Object(map))
    }

    fn simple(&mut self, info: u8) -> Result<Value, String> {
        match info {
            20 => Ok(Value::Bool(false)),
            21 => Ok(Value::Bool(true)),
            22 | 23 => Ok(Value::Null),
            24 => Ok(json!({"$simple": self.byte()?})),
            25 => {
                let bits = u16::from_be_bytes(self.take(2)?.try_into().unwrap());
                Ok(Number::from_f64(f16_to_f32(bits) as f64)
                    .map(Value::Number)
                    .unwrap_or(Value::Null))
            }
            26 => {
                let value = f32::from_be_bytes(self.take(4)?.try_into().unwrap()) as f64;
                Ok(Number::from_f64(value)
                    .map(Value::Number)
                    .unwrap_or(Value::Null))
            }
            27 => {
                let value = f64::from_be_bytes(self.take(8)?.try_into().unwrap());
                Ok(Number::from_f64(value)
                    .map(Value::Number)
                    .unwrap_or(Value::Null))
            }
            _ => Err(format!("CBOR: unsupported simple value {info}")),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }
}

fn cbor_key(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn f16_to_f32(bits: u16) -> f32 {
    let sign = if bits & 0x8000 == 0 { 1.0 } else { -1.0 };
    let exponent = (bits >> 10) & 0x1f;
    let fraction = bits & 0x03ff;
    match exponent {
        0 => sign * (fraction as f32) * 2f32.powi(-24),
        31 if fraction == 0 => sign * f32::INFINITY,
        31 => f32::NAN,
        _ => sign * (1.0 + fraction as f32 / 1024.0) * 2f32.powi(i32::from(exponent) - 15),
    }
}

/// Decode a MessagePack document into a JSON value.
pub fn parse_messagepack(bytes: &[u8]) -> Result<Value, String> {
    let mut reader = MsgPackReader { bytes, pos: 0 };
    let mut values = Vec::new();
    while !reader.is_empty() {
        values.push(reader.value()?);
    }
    match values.len() {
        0 => Err("MessagePack input is empty".to_string()),
        1 => Ok(values.remove(0)),
        _ => Ok(Value::Array(values)),
    }
}

struct MsgPackReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl MsgPackReader<'_> {
    fn is_empty(&self) -> bool {
        self.pos >= self.bytes.len()
    }

    fn take(&mut self, len: usize) -> Result<&[u8], String> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| "MessagePack: length overflow".to_string())?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| "MessagePack: truncated payload".to_string())?;
        self.pos = end;
        Ok(slice)
    }

    fn byte(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }

    fn value(&mut self) -> Result<Value, String> {
        let marker = self.byte()?;
        match marker {
            0x00..=0x7f => Ok(Value::Number(Number::from(marker))),
            0x80..=0x8f => self.map(usize::from(marker & 0x0f)),
            0x90..=0x9f => self.array(usize::from(marker & 0x0f)),
            0xa0..=0xbf => self.string(usize::from(marker & 0x1f)),
            0xc0 => Ok(Value::Null),
            0xc2 => Ok(Value::Bool(false)),
            0xc3 => Ok(Value::Bool(true)),
            0xc4 => {
                let len = usize::from(self.byte()?);
                self.binary(len)
            }
            0xc5 => {
                let len = usize::from(u16::from_be_bytes(self.take(2)?.try_into().unwrap()));
                self.binary(len)
            }
            0xc6 => {
                let len = usize::try_from(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
                    .map_err(|_| "MessagePack: length overflow")?;
                self.binary(len)
            }
            0xc7 => {
                let len = usize::from(self.byte()?);
                self.extension(len)
            }
            0xc8 => {
                let len = usize::from(u16::from_be_bytes(self.take(2)?.try_into().unwrap()));
                self.extension(len)
            }
            0xc9 => {
                let len = usize::try_from(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
                    .map_err(|_| "MessagePack: length overflow")?;
                self.extension(len)
            }
            0xca => Ok(float32(f32::from_be_bytes(
                self.take(4)?.try_into().unwrap(),
            ))),
            0xcb => Ok(float64(f64::from_be_bytes(
                self.take(8)?.try_into().unwrap(),
            ))),
            0xcc => Ok(Value::Number(Number::from(self.byte()?))),
            0xcd => Ok(Value::Number(Number::from(u16::from_be_bytes(
                self.take(2)?.try_into().unwrap(),
            )))),
            0xce => Ok(Value::Number(Number::from(u32::from_be_bytes(
                self.take(4)?.try_into().unwrap(),
            )))),
            0xcf => Ok(Value::Number(Number::from(u64::from_be_bytes(
                self.take(8)?.try_into().unwrap(),
            )))),
            0xd0 => Ok(Value::Number(Number::from(i64::from(i8::from_be_bytes(
                self.take(1)?.try_into().unwrap(),
            ))))),
            0xd1 => Ok(Value::Number(Number::from(i64::from(i16::from_be_bytes(
                self.take(2)?.try_into().unwrap(),
            ))))),
            0xd2 => Ok(Value::Number(Number::from(i64::from(i32::from_be_bytes(
                self.take(4)?.try_into().unwrap(),
            ))))),
            0xd3 => Ok(Value::Number(Number::from(i64::from_be_bytes(
                self.take(8)?.try_into().unwrap(),
            )))),
            0xd4..=0xd8 => {
                let len = 1usize << (marker - 0xd4);
                self.extension(len)
            }
            0xd9 => {
                let len = usize::from(self.byte()?);
                self.string(len)
            }
            0xda => {
                let len = usize::from(u16::from_be_bytes(self.take(2)?.try_into().unwrap()));
                self.string(len)
            }
            0xdb => {
                let len = usize::try_from(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
                    .map_err(|_| "MessagePack: length overflow")?;
                self.string(len)
            }
            0xdc => {
                let len = usize::from(u16::from_be_bytes(self.take(2)?.try_into().unwrap()));
                self.array(len)
            }
            0xdd => {
                let len = usize::try_from(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
                    .map_err(|_| "MessagePack: length overflow")?;
                self.array(len)
            }
            0xde => {
                let len = usize::from(u16::from_be_bytes(self.take(2)?.try_into().unwrap()));
                self.map(len)
            }
            0xdf => {
                let len = usize::try_from(u32::from_be_bytes(self.take(4)?.try_into().unwrap()))
                    .map_err(|_| "MessagePack: length overflow")?;
                self.map(len)
            }
            0xe0..=0xff => Ok(Value::Number(Number::from(i64::from(marker as i8)))),
            _ => Err(format!("MessagePack: unsupported marker 0x{marker:02x}")),
        }
    }

    fn string(&mut self, len: usize) -> Result<Value, String> {
        let data = self.take(len)?;
        String::from_utf8(data.to_vec())
            .map(Value::String)
            .map_err(|error| format!("MessagePack: invalid UTF-8: {error}"))
    }

    fn binary(&mut self, len: usize) -> Result<Value, String> {
        Ok(json!({"$bytes": BASE64.encode(self.take(len)?)}))
    }

    fn extension(&mut self, len: usize) -> Result<Value, String> {
        let kind = i8::from_be_bytes(self.take(1)?.try_into().unwrap());
        let data = self.take(len)?;
        Ok(json!({"$ext": kind, "data": BASE64.encode(data)}))
    }

    fn array(&mut self, len: usize) -> Result<Value, String> {
        let mut values = Vec::with_capacity(len.min(1024));
        for _ in 0..len {
            values.push(self.value()?);
        }
        Ok(Value::Array(values))
    }

    fn map(&mut self, len: usize) -> Result<Value, String> {
        let mut map = Map::new();
        for _ in 0..len {
            let key = self.value()?;
            let value = self.value()?;
            map.insert(msgpack_key(&key), value);
        }
        Ok(Value::Object(map))
    }
}

fn msgpack_key(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(value) => value.to_string(),
        Value::Null => "null".to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn float32(value: f32) -> Value {
    Number::from_f64(f64::from(value))
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

fn float64(value: f64) -> Value {
    Number::from_f64(value)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// Decode protobuf wire data without a schema.
///
/// Field numbers are exposed as `field_N`; nested length-delimited messages are
/// recursively decoded when they look like protobuf data, otherwise printable
/// payloads become strings and binary payloads become base64 objects.
pub fn parse_protobuf(bytes: &[u8]) -> Result<Value, String> {
    let mut reader = ProtoReader { bytes, pos: 0 };
    let fields = reader.fields(None)?;
    if reader.pos != bytes.len() {
        return Err("protobuf: trailing bytes".to_string());
    }
    Ok(Value::Object(fields))
}

struct ProtoReader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl ProtoReader<'_> {
    fn varint(&mut self) -> Result<u64, String> {
        let mut value = 0u64;
        for shift in (0..70).step_by(7) {
            let byte = *self
                .bytes
                .get(self.pos)
                .ok_or_else(|| "protobuf: truncated varint".to_string())?;
            self.pos += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("protobuf: varint too long".to_string())
    }

    fn take(&mut self, len: usize) -> Result<&[u8], String> {
        let end = self
            .pos
            .checked_add(len)
            .ok_or_else(|| "protobuf: length overflow".to_string())?;
        let slice = self
            .bytes
            .get(self.pos..end)
            .ok_or_else(|| "protobuf: truncated field".to_string())?;
        self.pos = end;
        Ok(slice)
    }

    fn fields(&mut self, end_group: Option<u64>) -> Result<Map<String, Value>, String> {
        let mut map = Map::new();
        while self.pos < self.bytes.len() {
            let key = self.varint()?;
            let field = key >> 3;
            let wire = (key & 7) as u8;
            if field == 0 {
                return Err("protobuf: field number 0".to_string());
            }
            if wire == 4 {
                return match end_group {
                    Some(expected) if expected == field => Ok(map),
                    Some(expected) => Err(format!(
                        "protobuf: end group {field} does not match {expected}"
                    )),
                    None => Err("protobuf: unexpected end group".to_string()),
                };
            }
            let value = match wire {
                0 => Value::Number(Number::from(self.varint()?)),
                1 => {
                    let bytes = self.take(8)?;
                    Value::String(format!("0x{}", hex(bytes)))
                }
                2 => {
                    let len = usize::try_from(self.varint()?)
                        .map_err(|_| "protobuf: length overflow".to_string())?;
                    self.length_delimited(len)?
                }
                3 => {
                    let nested = self.fields(Some(field))?;
                    Value::Object(nested)
                }
                5 => {
                    let bytes = self.take(4)?;
                    Value::String(format!("0x{}", hex(bytes)))
                }
                other => return Err(format!("protobuf: unsupported wire type {other}")),
            };
            insert_field(&mut map, format!("field_{field}"), value);
        }
        if end_group.is_some() {
            Err("protobuf: unterminated group".to_string())
        } else {
            Ok(map)
        }
    }

    fn length_delimited(&mut self, len: usize) -> Result<Value, String> {
        let data = self.take(len)?;
        if let Ok(text) = std::str::from_utf8(data)
            && is_printable(text)
        {
            return Ok(Value::String(text.to_string()));
        }
        if len > 0 {
            let mut nested = ProtoReader {
                bytes: data,
                pos: 0,
            };
            if let Ok(fields) = nested.fields(None)
                && nested.pos == data.len()
                && !fields.is_empty()
            {
                return Ok(Value::Object(fields));
            }
        }
        Ok(json!({"$bytes": BASE64.encode(data)}))
    }
}

fn insert_field(map: &mut Map<String, Value>, name: String, value: Value) {
    match map.remove(&name) {
        None => {
            map.insert(name, value);
        }
        Some(Value::Array(mut values)) => {
            values.push(value);
            map.insert(name, Value::Array(values));
        }
        Some(existing) => {
            map.insert(name, Value::Array(vec![existing, value]));
        }
    }
}

fn is_printable(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|ch| !ch.is_control() || matches!(ch, '\n' | '\r' | '\t'))
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
#[path = "../../tests/unit/diff/serialization.rs"]
mod tests;
