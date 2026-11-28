//! Unit tests for the comparison engine's private readers.
//!
//! Compiled as a child module of `src/engine.rs` through `#[path]`, so the
//! tests can call private archive readers directly. These paths sit behind the
//! whole-file size check in `compare_files`, so they cannot be reached through
//! the public API with the bundled fixtures.

use super::*;

fn fixture(relative: &str) -> std::path::PathBuf {
    let mut path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.join("fixtures").join(relative)
}

#[test]
fn cab_entries_respect_the_file_size_limit() {
    let bytes = fs::read(fixture("cab/old.cab")).unwrap();

    let mut ctx = Ctx::default();
    let entries = DiffEngine::new(DiffOptions::default())
        .read_cab_entries(&bytes, "old.cab", &mut ctx)
        .unwrap();
    assert!(!entries.is_empty(), "fixture must contain readable entries");
    assert!(
        ctx.warnings.is_empty(),
        "default limit must not warn: {:?}",
        ctx.warnings
    );

    // One byte per entry is below every fixture entry, so all of them are
    // skipped with a warning instead of being buffered without bound.
    let mut ctx = Ctx::default();
    let options = DiffOptions {
        max_file_bytes: 1,
        ..DiffOptions::default()
    };
    let entries = DiffEngine::new(options)
        .read_cab_entries(&bytes, "old.cab", &mut ctx)
        .unwrap();
    assert!(entries.is_empty(), "{:?}", entries.keys());
    assert!(
        ctx.warnings
            .iter()
            .any(|warning| warning.contains("exceeds the detailed-diff limit")),
        "{:?}",
        ctx.warnings
    );
}

#[test]
fn zip_lzma_compressed_payload_respects_the_limit() {
    // The `zip` writer cannot produce LZMA entries, so the archive is laid
    // out by hand: a method-14 entry declaring 10 uncompressed bytes but 100
    // compressed bytes. The payload is never decoded.
    let payload = vec![0x5au8; 100];
    let bytes = raw_zip("noise.bin", 14, 10, payload.len() as u32, &payload);

    let options = DiffOptions {
        max_file_bytes: 50,
        ..DiffOptions::default()
    };
    let engine = DiffEngine::new(options);

    // The declared uncompressed size (10) passes the limit; only the bound on
    // the compressed stream (100 > 50) can reject the entry.
    let mut ctx = Ctx::default();
    let entries = engine
        .read_zip_entries(&bytes, "noise.zip", &mut ctx)
        .unwrap();
    assert!(entries.is_empty(), "{:?}", entries.keys());
    assert!(
        ctx.warnings
            .iter()
            .any(|warning| warning.contains("more compressed bytes than the detailed-diff limit")),
        "{:?}",
        ctx.warnings
    );
}

/// Builds a one-entry ZIP with arbitrary header fields. Both headers are
/// consistent, so the `zip` reader accepts the file; the CRC is left zero
/// because the reader does not verify it and the payload is never inflated.
fn raw_zip(
    name: &str,
    method: u16,
    declared_size: u32,
    compressed_size: u32,
    data: &[u8],
) -> Vec<u8> {
    let name = name.as_bytes();
    let mut bytes = Vec::new();
    // Local file header.
    bytes.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    bytes.extend_from_slice(&63u16.to_le_bytes()); // version needed (LZMA)
    bytes.extend_from_slice(&0u16.to_le_bytes()); // general purpose flags
    bytes.extend_from_slice(&method.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes()); // modification time
    bytes.extend_from_slice(&0x0021u16.to_le_bytes()); // modification date
    bytes.extend_from_slice(&0u32.to_le_bytes()); // crc32
    bytes.extend_from_slice(&compressed_size.to_le_bytes());
    bytes.extend_from_slice(&declared_size.to_le_bytes());
    bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes()); // extra field length
    bytes.extend_from_slice(name);
    bytes.extend_from_slice(data);
    // Central directory header.
    let directory_offset = bytes.len() as u32;
    bytes.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    bytes.extend_from_slice(&63u16.to_le_bytes()); // version made by
    bytes.extend_from_slice(&63u16.to_le_bytes()); // version needed
    bytes.extend_from_slice(&0u16.to_le_bytes()); // general purpose flags
    bytes.extend_from_slice(&method.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes()); // modification time
    bytes.extend_from_slice(&0x0021u16.to_le_bytes()); // modification date
    bytes.extend_from_slice(&0u32.to_le_bytes()); // crc32
    bytes.extend_from_slice(&compressed_size.to_le_bytes());
    bytes.extend_from_slice(&declared_size.to_le_bytes());
    bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes()); // extra field length
    bytes.extend_from_slice(&0u16.to_le_bytes()); // comment length
    bytes.extend_from_slice(&0u16.to_le_bytes()); // disk number start
    bytes.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
    bytes.extend_from_slice(&0u32.to_le_bytes()); // external attributes
    bytes.extend_from_slice(&0u32.to_le_bytes()); // local header offset
    bytes.extend_from_slice(name);
    let directory_size = bytes.len() as u32 - directory_offset;
    // End of central directory.
    bytes.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes()); // disk number
    bytes.extend_from_slice(&0u16.to_le_bytes()); // central directory disk
    bytes.extend_from_slice(&1u16.to_le_bytes()); // entries on this disk
    bytes.extend_from_slice(&1u16.to_le_bytes()); // total entries
    bytes.extend_from_slice(&directory_size.to_le_bytes());
    bytes.extend_from_slice(&directory_offset.to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes()); // comment length
    bytes
}
