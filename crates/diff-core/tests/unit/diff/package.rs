//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;
use sha2::{Digest, Sha256};
use std::io::Write as _;

#[test]
fn hashes_entries_without_materializing_and_reuses_the_buffer() {
    let mut buffer = std::io::Cursor::new(Vec::new());
    {
        let mut writer = zip::ZipWriter::new(&mut buffer);
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("media/a.bin", options).unwrap();
        writer.write_all(b"first payload").unwrap();
        writer.start_file("media/b.bin", options).unwrap();
        writer.write_all(b"second payload").unwrap();
        writer.finish().unwrap();
    }
    let bytes = buffer.into_inner();
    let package = Package::new(&bytes).unwrap();

    let (first_hash, first_size) = package.hash_entry("media/a.bin").unwrap().unwrap();
    let (second_hash, second_size) = package.hash_entry("media/b.bin").unwrap().unwrap();
    assert_eq!(first_size, 13);
    assert_eq!(second_size, 14);
    let expected: String = Sha256::digest(b"first payload")
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    assert_eq!(first_hash, expected);
    assert_ne!(first_hash, second_hash);
    assert!(package.hash_entry("missing.bin").unwrap().is_none());

    // A second pass must produce the same result after the scratch buffer was
    // reused by the previous entry.
    assert_eq!(
        package.hash_entry("media/a.bin").unwrap().unwrap(),
        (first_hash, first_size)
    );
}
