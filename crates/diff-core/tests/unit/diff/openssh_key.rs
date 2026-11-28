//! Unit tests for this module, compiled as its child through `#[path]`.
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
