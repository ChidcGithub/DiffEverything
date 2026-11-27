use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde_json::{Map, Value, json};

/// Parse an OpenSSH public key line into a semantic key model.
///
/// The base64 wire blob is decoded so RSA/DSA/ECDSA/Ed25519 fields can be
/// compared individually instead of as opaque base64 text.
pub fn parse_public_key(bytes: &[u8]) -> Result<Value, String> {
    let text = std::str::from_utf8(bytes).map_err(|error| error.to_string())?;
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| "empty OpenSSH public key".to_string())?;
    let mut parts = line.splitn(3, char::is_whitespace);
    let key_type = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| "missing OpenSSH key type".to_string())?;
    let encoded = parts
        .next()
        .filter(|part| !part.is_empty())
        .ok_or_else(|| "missing OpenSSH key blob".to_string())?;
    let comment = parts.next().unwrap_or("").trim().to_string();
    let blob = BASE64
        .decode(encoded)
        .map_err(|error| format!("invalid OpenSSH base64: {error}"))?;
    let mut reader = Reader::new(&blob);
    let inner_type = String::from_utf8_lossy(&reader.string()?).into_owned();
    if inner_type != key_type {
        return Err(format!(
            "OpenSSH key type mismatch: {key_type} != {inner_type}"
        ));
    }

    let mut fields = Map::new();
    let bits;
    match key_type {
        "ssh-rsa" => {
            let e = reader.mpint()?;
            let n = reader.mpint()?;
            bits = mpint_bits(&n);
            fields.insert("e".to_string(), Value::String(hex(&e)));
            fields.insert("n".to_string(), Value::String(hex(&n)));
        }
        "ssh-dss" => {
            let p = reader.mpint()?;
            let q = reader.mpint()?;
            let g = reader.mpint()?;
            let y = reader.mpint()?;
            bits = mpint_bits(&p);
            fields.insert("p".to_string(), Value::String(hex(&p)));
            fields.insert("q".to_string(), Value::String(hex(&q)));
            fields.insert("g".to_string(), Value::String(hex(&g)));
            fields.insert("y".to_string(), Value::String(hex(&y)));
        }
        "ssh-ed25519" => {
            let key = reader.string()?;
            bits = key.len() * 8;
            fields.insert("key".to_string(), Value::String(hex(&key)));
        }
        kind if kind.starts_with("ecdsa-sha2-") => {
            let curve = String::from_utf8_lossy(&reader.string()?).into_owned();
            let point = reader.string()?;
            bits = match curve.as_str() {
                "nistp256" => 256,
                "nistp384" => 384,
                "nistp521" => 521,
                _ => 0,
            };
            fields.insert("curve".to_string(), Value::String(curve));
            fields.insert("point".to_string(), Value::String(hex(&point)));
        }
        kind if kind.starts_with("sk-ssh-ed25519@") => {
            let key = reader.string()?;
            let application = String::from_utf8_lossy(&reader.string()?).into_owned();
            bits = key.len() * 8;
            fields.insert("key".to_string(), Value::String(hex(&key)));
            fields.insert("application".to_string(), Value::String(application));
        }
        _ => {
            bits = 0;
            fields.insert(
                "remaining".to_string(),
                Value::String(hex(reader.remaining())),
            );
        }
    }

    Ok(json!({
        "format": "openssh-key",
        "key_type": key_type,
        "bits": bits,
        "comment": comment,
        "fields": fields,
        "blob_digest": format!("{:016x}", fnv1a64(&blob)),
    }))
}

fn mpint_bits(value: &[u8]) -> usize {
    let first = value.iter().position(|byte| *byte != 0);
    match first {
        Some(index) => (value.len() - index) * 8,
        None => 0,
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push_str(&format!("{byte:02x}"));
    }
    output
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

    fn remaining(&self) -> &'a [u8] {
        &self.data[self.position.min(self.data.len())..]
    }

    fn bytes(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self
            .position
            .checked_add(count)
            .ok_or_else(|| "OpenSSH key size overflow".to_string())?;
        let slice = self
            .data
            .get(self.position..end)
            .ok_or_else(|| "truncated OpenSSH key".to_string())?;
        self.position = end;
        Ok(slice)
    }

    fn u32(&mut self) -> Result<u32, String> {
        let bytes = self.bytes(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn string(&mut self) -> Result<Vec<u8>, String> {
        let length = self.u32()? as usize;
        Ok(self.bytes(length)?.to_vec())
    }

    fn mpint(&mut self) -> Result<Vec<u8>, String> {
        self.string()
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
    fn parses_dsa_and_rsa_keys() {
        let dsa = std::fs::read(fixture("ssh/test_openssh_pub_key1.pub")).unwrap();
        let dsa = parse_public_key(&dsa).unwrap();
        assert_eq!(dsa["key_type"], "ssh-dss");
        assert_eq!(dsa["comment"], "Test1");
        assert!(dsa["fields"]["p"].as_str().unwrap().len() > 10);

        let rsa = std::fs::read(fixture("ssh/test_openssh_pub_key2.pub")).unwrap();
        let rsa = parse_public_key(&rsa).unwrap();
        assert_eq!(rsa["key_type"], "ssh-rsa");
        assert!(rsa["bits"].as_u64().unwrap() >= 2048);
        assert_eq!(rsa["fields"]["e"], "010001");
    }

    #[test]
    fn rejects_invalid_key_blob() {
        assert!(parse_public_key(b"ssh-rsa !!! comment").is_err());
    }
}
