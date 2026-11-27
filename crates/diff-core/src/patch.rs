use crate::error::{DiffError, Result};
use std::io::Write as _;
use std::path::{Path, PathBuf};

pub fn create_patch(old: &[u8], new: &[u8]) -> Result<Vec<u8>> {
    let mut patch = Vec::new();
    bsdiff::diff(old, new, &mut patch).map_err(DiffError::Io)?;
    Ok(patch)
}

pub fn apply_patch(old: &[u8], patch: &[u8]) -> Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut reader = patch;
    bsdiff::patch(old, &mut reader, &mut output).map_err(DiffError::Io)?;
    Ok(output)
}

/// Create a valid uncompressed HDIFF13 patch with a single cover for the
/// common prefix and literal data for the rest. This is deliberately simple
/// and deterministic; it prioritizes compatibility over patch size.
pub fn create_pure_hdiff(old: &[u8], new: &[u8]) -> Vec<u8> {
    let (cover, rle_ctrl, rle_code, literal) = build_pure_hdiff_buffers(old, new);
    let mut patch = b"HDIFF13&\0".to_vec();
    write_hdiff_varint(&mut patch, new.len());
    write_hdiff_varint(&mut patch, old.len());
    write_hdiff_varint(&mut patch, usize::from(!cover.is_empty()));
    for buffer in [&cover, &rle_ctrl, &rle_code, &literal] {
        write_hdiff_varint(&mut patch, buffer.len());
        write_hdiff_varint(&mut patch, 0);
    }
    patch.extend_from_slice(&cover);
    patch.extend_from_slice(&rle_ctrl);
    patch.extend_from_slice(&rle_code);
    patch.extend_from_slice(&literal);
    patch
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HdiffCompression {
    None,
    Zstd,
    Zlib,
    Lzma2,
    Bzip2,
}

impl HdiffCompression {
    fn type_name(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Zstd => "zstd",
            Self::Zlib => "zlib",
            Self::Lzma2 => "lzma2",
            Self::Bzip2 => "bzip2",
        }
    }

    fn compress(self, data: &[u8]) -> Option<Vec<u8>> {
        if data.is_empty() || self == Self::None {
            return None;
        }
        let compressed = match self {
            Self::None => return None,
            Self::Zstd => zstd::stream::encode_all(data, 3).ok()?,
            Self::Zlib => {
                let mut encoder =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
                encoder.write_all(data).ok()?;
                encoder.finish().ok()?
            }
            Self::Lzma2 => compress_lzma2(data)?,
            Self::Bzip2 => {
                let mut encoder =
                    bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::default());
                encoder.write_all(data).ok()?;
                encoder.finish().ok()?
            }
        };
        (compressed.len() < data.len()).then_some(compressed)
    }
}

fn compress_lzma2(data: &[u8]) -> Option<Vec<u8>> {
    let mut options = liblzma::stream::LzmaOptions::new_preset(6).ok()?;
    options.dict_size(1 << 20);
    let mut filters = liblzma::stream::Filters::new();
    filters.lzma2(&options);
    let mut stream = liblzma::stream::Stream::new_raw_encoder(&filters).ok()?;
    let mut output = Vec::with_capacity(data.len() + 1024);
    let mut input_offset = 0usize;
    loop {
        let status = stream
            .process_vec(
                &data[input_offset..],
                &mut output,
                liblzma::stream::Action::Finish,
            )
            .ok()?;
        input_offset = stream.total_in() as usize;
        if status == liblzma::stream::Status::StreamEnd {
            break;
        }
        output.reserve(4096);
    }
    let mut checksummed = Vec::with_capacity(output.len() + 1);
    checksummed.push(0x10); // LZMA2 property byte for a 1 MiB dictionary
    checksummed.extend_from_slice(&output);
    Some(checksummed)
}

fn create_pure_hdiff_with_compression(
    old: &[u8],
    new: &[u8],
    compression: HdiffCompression,
) -> Vec<u8> {
    let (cover, rle_ctrl, rle_code, literal) = build_pure_hdiff_buffers(old, new);
    let buffers: Vec<(&[u8], Option<Vec<u8>>)> = [&cover, &rle_ctrl, &rle_code, &literal]
        .into_iter()
        .map(|buffer| (buffer.as_slice(), compression.compress(buffer.as_slice())))
        .collect();

    let mut patch = format!("HDIFF13&{}\0", compression.type_name()).into_bytes();
    write_hdiff_varint(&mut patch, new.len());
    write_hdiff_varint(&mut patch, old.len());
    write_hdiff_varint(&mut patch, usize::from(!cover.is_empty()));
    for (raw, compressed) in &buffers {
        write_hdiff_varint(&mut patch, raw.len());
        write_hdiff_varint(&mut patch, compressed.as_ref().map(Vec::len).unwrap_or(0));
    }
    for (raw, compressed) in &buffers {
        patch.extend_from_slice(compressed.as_deref().unwrap_or(raw));
    }
    patch
}

/// Like [`create_pure_hdiff`], but compresses each HDIFF13 buffer with zstd
/// when it makes the buffer smaller.
pub fn create_pure_hdiff_zstd(old: &[u8], new: &[u8]) -> Vec<u8> {
    create_pure_hdiff_with_compression(old, new, HdiffCompression::Zstd)
}

pub fn create_pure_hdiff_zlib(old: &[u8], new: &[u8]) -> Vec<u8> {
    create_pure_hdiff_with_compression(old, new, HdiffCompression::Zlib)
}

pub fn create_pure_hdiff_lzma2(old: &[u8], new: &[u8]) -> Vec<u8> {
    create_pure_hdiff_with_compression(old, new, HdiffCompression::Lzma2)
}

pub fn create_pure_hdiff_bzip2(old: &[u8], new: &[u8]) -> Vec<u8> {
    create_pure_hdiff_with_compression(old, new, HdiffCompression::Bzip2)
}

fn build_pure_hdiff_buffers(old: &[u8], new: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>) {
    let common = old.len().min(new.len());
    let tail = new.len() - common;

    let mut cover = Vec::new();
    if common > 0 {
        write_hdiff_tagged(&mut cover, 0, 1, 0);
        write_hdiff_tagged(&mut cover, 0, 0, 0);
        write_hdiff_varint(&mut cover, common);
    }

    let mut rle_ctrl = Vec::new();
    let mut rle_code = Vec::with_capacity(common);
    if common > 0 {
        write_hdiff_tagged(&mut rle_ctrl, common - 1, 2, 3);
        for index in 0..common {
            rle_code.push(new[index].wrapping_sub(old[index]));
        }
    }
    if tail > 0 {
        write_hdiff_tagged(&mut rle_ctrl, tail - 1, 2, 0);
    }

    (cover, rle_ctrl, rle_code, new[common..].to_vec())
}

pub fn create_pure_dir_hdiff(old_dir: &Path, new_dir: &Path) -> Result<Vec<u8>> {
    create_pure_dir_hdiff_inner(old_dir, new_dir, HdiffCompression::None)
}

/// Create a zstd-compressed HDIFF19 directory patch with fadler64 checksums.
pub fn create_pure_dir_hdiff_zstd(old_dir: &Path, new_dir: &Path) -> Result<Vec<u8>> {
    create_pure_dir_hdiff_inner(old_dir, new_dir, HdiffCompression::Zstd)
}

pub fn create_pure_dir_hdiff_zlib(old_dir: &Path, new_dir: &Path) -> Result<Vec<u8>> {
    create_pure_dir_hdiff_inner(old_dir, new_dir, HdiffCompression::Zlib)
}

pub fn create_pure_dir_hdiff_lzma2(old_dir: &Path, new_dir: &Path) -> Result<Vec<u8>> {
    create_pure_dir_hdiff_inner(old_dir, new_dir, HdiffCompression::Lzma2)
}

pub fn create_pure_dir_hdiff_bzip2(old_dir: &Path, new_dir: &Path) -> Result<Vec<u8>> {
    create_pure_dir_hdiff_inner(old_dir, new_dir, HdiffCompression::Bzip2)
}

fn create_pure_dir_hdiff_inner(
    old_dir: &Path,
    new_dir: &Path,
    compression: HdiffCompression,
) -> Result<Vec<u8>> {
    let old_entries = collect_tree_entries(old_dir)?;
    let new_entries = collect_tree_entries(new_dir)?;
    let old_files: std::collections::BTreeMap<&str, (usize, &PathBuf)> = old_entries
        .iter()
        .enumerate()
        .filter_map(|(index, (name, path))| {
            path.as_ref().map(|path| (name.as_str(), (index, path)))
        })
        .collect();

    let mut new_stream = Vec::new();
    let mut sizes = Vec::new();
    let mut new_ref_indexes = Vec::new();
    let mut same_pairs = Vec::new();
    let mut same_stream = Vec::new();
    let mut same_file_size = 0usize;
    let mut new_path_sum_size = 1usize; // new root entry: empty C string
    for (index, (name, path)) in new_entries.iter().enumerate() {
        new_path_sum_size += name.len() + 1;
        if let Some(path) = path {
            let data = std::fs::read(path)
                .map_err(|error| DiffError::Invalid(format!("{}: {error}", path.display())))?;
            if let Some((old_index, old_path)) = old_files.get(name.as_str()) {
                let old_data = std::fs::read(old_path).map_err(|error| {
                    DiffError::Invalid(format!("{}: {error}", old_path.display()))
                })?;
                if old_data == data {
                    same_pairs.push((index + 1, old_index + 1));
                    same_file_size += data.len();
                    same_stream.extend_from_slice(&data);
                    continue;
                }
            }
            sizes.push(data.len());
            new_stream.extend_from_slice(&data);
            new_ref_indexes.push(index + 1); // index 0 is the root entry
        }
    }
    let old_path_sum_size = 1 + old_entries
        .iter()
        .map(|(name, _)| name.len() + 1)
        .sum::<usize>();

    let mut head = vec![0u8]; // old root entry
    for (name, _) in &old_entries {
        head.extend_from_slice(name.as_bytes());
        head.push(0);
    }
    head.push(0); // new root entry
    for (name, _) in &new_entries {
        head.extend_from_slice(name.as_bytes());
        head.push(0);
    }
    pack_hdiff_inc_list(&mut head, new_ref_indexes.iter().copied());
    for size in sizes {
        write_hdiff_varint(&mut head, size);
    }
    pack_hdiff_same_pairs(&mut head, &same_pairs);

    let mut patch = if compression == HdiffCompression::None {
        b"HDIFF19&&fadler64\0".to_vec()
    } else {
        format!("HDIFF19&{}&fadler64\0", compression.type_name()).into_bytes()
    };
    let head_wire = compression.compress(&head);
    write_hdiff_varint(&mut patch, 1); // old path is a directory
    write_hdiff_varint(&mut patch, 1); // new path is a directory
    write_hdiff_varint(&mut patch, old_entries.len() + 1); // old path count
    write_hdiff_varint(&mut patch, old_path_sum_size);
    write_hdiff_varint(&mut patch, new_entries.len() + 1); // new path count
    write_hdiff_varint(&mut patch, new_path_sum_size);
    write_hdiff_varint(&mut patch, 0); // old ref file count
    write_hdiff_varint(&mut patch, 0); // old ref stream size
    write_hdiff_varint(&mut patch, new_ref_indexes.len());
    write_hdiff_varint(&mut patch, new_stream.len());
    write_hdiff_varint(&mut patch, same_pairs.len()); // same-file pair count
    write_hdiff_varint(&mut patch, same_file_size); // same-file bytes
    write_hdiff_varint(&mut patch, 0); // execute list count
    write_hdiff_varint(&mut patch, 0); // private reserved size
    write_hdiff_varint(&mut patch, 0); // private extern size
    write_hdiff_varint(&mut patch, 0); // extern size
    write_hdiff_varint(&mut patch, head.len());
    write_hdiff_varint(&mut patch, head_wire.as_ref().map(Vec::len).unwrap_or(0)); // compressed head size
    write_hdiff_varint(&mut patch, 8); // checksum byte size
    write_fadler64(&mut patch, &[]); // old reference stream
    write_fadler64(&mut patch, &new_stream); // new reference stream
    write_fadler64(&mut patch, &same_stream); // same-file stream
    let checksum_offset = patch.len();
    patch.extend_from_slice(&[0u8; 8]); // diff checksum placeholder
    let checksum_start = patch.len();
    patch.extend_from_slice(head_wire.as_deref().unwrap_or(&head));
    patch.extend_from_slice(&create_pure_hdiff_with_compression(
        &[],
        &new_stream,
        compression,
    ));
    let diff_checksum = fadler64(&patch[checksum_start..]);
    patch[checksum_offset..checksum_offset + 8].copy_from_slice(&diff_checksum.to_le_bytes());
    Ok(patch)
}

const FAST_ADLER64_TABLE: [u32; 256] = [
    0xb5e6a199, 0xe9834671, 0x8c715a7f, 0x0914fc22, 0x62abb5af, 0x9d31fb39, 0x69f00344, 0xa3d3c035,
    0x64db3b1c, 0x399a5b72, 0x959bb01e, 0x0c036c4f, 0x344fbc39, 0x4dc75b77, 0x25be7198, 0x0ab026b3,
    0xbab2d5e6, 0xe6a1281e, 0x0eb13df7, 0x53346d23, 0x43437647, 0xb363ede9, 0x5783a9eb, 0xdea89be5,
    0x4a2e1c06, 0x7b58e553, 0xd3979639, 0xc1fafc95, 0xb5ec388c, 0xb0db4075, 0xc573fb3d, 0x4cde0617,
    0x4b86ee95, 0xd77edb52, 0x100140c5, 0x6527c8e8, 0x8b8e01cb, 0x3bb6faa9, 0x9f77583e, 0xd11925ac,
    0x8ae986f0, 0x59380e03, 0x3a9b560b, 0xad9eb601, 0x0f16648e, 0x36b3f894, 0xfe25d3b7, 0x14a50768,
    0xaaed25fd, 0x06cd0894, 0x667abac1, 0x5833a3c9, 0x92bfd5cf, 0xd9674e5e, 0x00b7f263, 0xa36fa10b,
    0x12338782, 0xcb11edb6, 0xca15bc13, 0xd10e177f, 0x4c60b16c, 0x762153c1, 0x059207f3, 0x6f8d3213,
    0xef298d69, 0xdba6f8b4, 0x15056856, 0x409f8150, 0x3b7609d6, 0xf15a0a04, 0x53b2fcc9, 0x8ad571be,
    0xa40dcc34, 0xe127ab04, 0xdd1a7d81, 0xfe33ae6b, 0x10e26f52, 0x45791ae1, 0x9208538c, 0xc5281840,
    0x444fee94, 0xa7b050a1, 0x0e053633, 0xaa159ddd, 0xc3c99a7b, 0x70e88e7a, 0x40f5c699, 0x5627f1ae,
    0x7c216685, 0x90550ccc, 0x5b565b88, 0x22ca3af3, 0x3f5c8372, 0x856d11ad, 0xd749378c, 0x70e5fc79,
    0x0ad88005, 0x5fbea93e, 0x9119cf5e, 0xb2108f9f, 0xd069b0e6, 0xd01b096b, 0x3bbbef86, 0x9d2e92ef,
    0x8b0b3b0b, 0xd853a1a8, 0x7773f1f2, 0x31450766, 0x96cefd47, 0x935a1c07, 0xb6f5245a, 0x4d75b877,
    0xc4e0ae38, 0xf291709d, 0x5d4a7395, 0xeaf6717b, 0x353013a2, 0x14ab729d, 0x20560099, 0x8d2f2776,
    0x7ce6ccb9, 0x4832d5cd, 0xd4519fa3, 0x0cac0dd3, 0x008ba27d, 0x0360ed98, 0x6fb2f7c0, 0x12f6b6ad,
    0x694823dd, 0x82441585, 0x2317f193, 0x09aa5ce1, 0x92dffce9, 0x4a06eddb, 0x80095313, 0x95613181,
    0x3a4b5d4b, 0x2dd5ab17, 0xb4ad20be, 0xd4297df4, 0x881cb3c5, 0x61036b5e, 0x0d271952, 0x5ca6be40,
    0x1c13ed9a, 0xd1a39288, 0x85d5c7ff, 0x76bd0c57, 0x900595da, 0x8ee9e2bc, 0xadeaeb12, 0x77b940bf,
    0x6589001c, 0x1e1a09de, 0x4c31dc7f, 0x3bd372be, 0x86af13eb, 0x37279e1b, 0x9329fcc0, 0xb9e4d0d0,
    0xd0d7f7b6, 0x00800dd5, 0xa9cd017f, 0xd9b49a97, 0xc6b9318c, 0x107a9974, 0x0e3d65c5, 0x1e030a54,
    0x71a2ae9a, 0x298db8ca, 0xdd7e2ea6, 0x7c7e43c4, 0x41d5b0d5, 0x5f35be61, 0xbc4a4198, 0x33b43e0c,
    0xa0e21f24, 0xd2cfb061, 0x60963539, 0x8c917984, 0xc86fc710, 0x08f07c80, 0x044d34fb, 0x1287cd98,
    0xbebea104, 0xa0aeab0b, 0xb775e218, 0xd75dc20e, 0xc79211cb, 0x88cab942, 0x19caa6e3, 0x69a032d1,
    0x3d9db18b, 0x3658c36c, 0x8deb277c, 0xaa2bd1e8, 0xa6ea28e4, 0xebd02f0c, 0x9de0ccd9, 0x0d6e8665,
    0xca028b95, 0x68189e89, 0x5b18e944, 0xf4c654ee, 0x03824598, 0x26d7a9ea, 0xbbb5de59, 0xc6b26613,
    0x88152a10, 0x5abd6d33, 0x8fb51b5c, 0x33a9fe83, 0x8b82720e, 0x4cc28094, 0x6687f674, 0x5442aac8,
    0x1369f26f, 0x99f94f7d, 0x49dd656e, 0xe3b5bb25, 0xe9d7524a, 0xceab6332, 0xa8d417d5, 0x5e730186,
    0x3ff82ff0, 0x38cda2e2, 0x9da9e9f2, 0xb1eb6f6f, 0xe2643584, 0xc3fe12ad, 0x6a6621ce, 0x18054b2e,
    0x880c75bb, 0x55c725b1, 0xd6e226a9, 0x04747a86, 0x3ba442af, 0xde31e4b6, 0x56b265b8, 0x90be6f06,
    0xd491b721, 0xe6004b58, 0x7e737445, 0xb1353152, 0x689bee09, 0xd9ae2437, 0x1e4160bc, 0x761828fc,
    0x9e290dc5, 0xdae6bc72, 0xb64afb06, 0x5198300d, 0x42d6d3f1, 0x08007323, 0x8cf682e4, 0xa1606c21,
];

fn fadler64(data: &[u8]) -> u64 {
    let mut adler = 1u32;
    let mut sum = 0u32;
    for byte in data {
        adler = adler.wrapping_add(FAST_ADLER64_TABLE[usize::from(*byte)]);
        sum = sum.wrapping_add(adler);
    }
    (u64::from(sum) << 32) | u64::from(adler)
}

fn write_fadler64(out: &mut Vec<u8>, data: &[u8]) {
    out.extend_from_slice(&fadler64(data).to_le_bytes());
}

fn collect_tree_entries(root: &Path) -> Result<Vec<(String, Option<PathBuf>)>> {
    let mut entries = Vec::new();
    for entry in walkdir::WalkDir::new(root).min_depth(1).sort_by_file_name() {
        let entry = entry.map_err(|error| DiffError::Invalid(error.to_string()))?;
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|error| DiffError::Invalid(error.to_string()))?;
        let mut name = relative.to_string_lossy().replace('\\', "/");
        if entry.file_type().is_dir() {
            name.push('/');
            entries.push((name, None));
        } else if entry.file_type().is_file() {
            entries.push((name, Some(entry.path().to_path_buf())));
        }
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(entries)
}

fn pack_hdiff_same_pairs(out: &mut Vec<u8>, pairs: &[(usize, usize)]) {
    let mut previous_new = usize::MAX;
    let mut previous_old = usize::MAX;
    for (new_index, old_index) in pairs {
        write_hdiff_varint(out, new_index.wrapping_sub(previous_new).wrapping_sub(1));
        let base = previous_old.wrapping_add(1);
        if *old_index >= base {
            write_hdiff_tagged(out, old_index - base, 1, 0);
        } else {
            write_hdiff_tagged(out, base - old_index, 1, 1);
        }
        previous_new = *new_index;
        previous_old = *old_index;
    }
}

fn pack_hdiff_inc_list(out: &mut Vec<u8>, values: impl Iterator<Item = usize>) {
    let mut previous: Option<usize> = None;
    for value in values {
        let delta = match previous {
            Some(previous) => value - previous - 1,
            None => value,
        };
        write_hdiff_varint(out, delta);
        previous = Some(value);
    }
}

/// Apply an HDIFF19 directory patch. Uncompressed and zstd-compressed heads
/// are supported; checksums are skipped rather than verified.
pub fn apply_pure_dir_hdiff(old_dir: &Path, patch: &[u8], out_dir: &Path) -> Result<()> {
    if !patch.starts_with(b"HDIFF19&") {
        return Err(DiffError::Invalid(
            "not an HDIFF19 directory patch".to_string(),
        ));
    }
    let mut position = b"HDIFF19".len();
    if patch.get(position) != Some(&b'&') {
        return Err(DiffError::Invalid(
            "invalid HDIFF19 directory patch types".to_string(),
        ));
    }
    position += 1;
    let compress_type = read_type_until(patch, &mut position, b'&')?;
    let _checksum_type = read_type_until(patch, &mut position, 0)?;

    let old_is_dir = read_hdiff_varint(patch, &mut position)?;
    let new_is_dir = read_hdiff_varint(patch, &mut position)?;
    if old_is_dir != 1 || new_is_dir != 1 {
        return Err(DiffError::Invalid(
            "directory patch targets a non-directory path".to_string(),
        ));
    }
    std::fs::create_dir_all(out_dir)
        .map_err(|error| DiffError::Invalid(format!("{}: {error}", out_dir.display())))?;
    let old_path_count = read_hdiff_varint(patch, &mut position)?;
    let old_path_sum_size = read_hdiff_varint(patch, &mut position)?;
    let new_path_count = read_hdiff_varint(patch, &mut position)?;
    let new_path_sum_size = read_hdiff_varint(patch, &mut position)?;
    let old_ref_count = read_hdiff_varint(patch, &mut position)?;
    let old_ref_size = read_hdiff_varint(patch, &mut position)?;
    let new_ref_count = read_hdiff_varint(patch, &mut position)?;
    let new_ref_size = read_hdiff_varint(patch, &mut position)?;
    let same_pair_count = read_hdiff_varint(patch, &mut position)?;
    let _same_file_size = read_hdiff_varint(patch, &mut position)?;
    let execute_count = read_hdiff_varint(patch, &mut position)?;
    let private_reserved_size = read_hdiff_varint(patch, &mut position)?;
    let private_extern_size = read_hdiff_varint(patch, &mut position)?;
    let extern_size = read_hdiff_varint(patch, &mut position)?;
    let head_size = read_hdiff_varint(patch, &mut position)?;
    let head_compressed_size = read_hdiff_varint(patch, &mut position)?;
    let checksum_size = read_hdiff_varint(patch, &mut position)?;
    let checksum_bytes = checksum_size
        .checked_mul(4)
        .ok_or_else(|| DiffError::Invalid("directory patch checksum size overflow".to_string()))?;
    position = position.checked_add(checksum_bytes).ok_or_else(|| {
        DiffError::Invalid("directory patch checksum offset overflow".to_string())
    })?;
    let head_wire_size = if head_compressed_size > 0 {
        head_compressed_size
    } else {
        head_size
    };
    let head_end = position
        .checked_add(head_wire_size)
        .ok_or_else(|| DiffError::Invalid("directory patch head is too large".to_string()))?;
    let head_wire = patch
        .get(position..head_end)
        .ok_or_else(|| DiffError::Invalid("truncated directory patch head".to_string()))?;
    let head_storage = if head_compressed_size > 0 {
        let decompressed = decompress_hdiff_buffer(&compress_type, head_wire, head_size)?;
        if decompressed.len() != head_size {
            return Err(DiffError::Invalid(
                "directory patch head size mismatch".to_string(),
            ));
        }
        decompressed
    } else {
        head_wire.to_vec()
    };
    let head = head_storage.as_slice();
    position = head_end;
    let hdiff_start = position
        .checked_add(private_extern_size)
        .and_then(|value| value.checked_add(extern_size))
        .ok_or_else(|| DiffError::Invalid("directory patch offsets overflow".to_string()))?;
    let hdiff = patch
        .get(hdiff_start..)
        .ok_or_else(|| DiffError::Invalid("truncated directory patch data".to_string()))?;

    let mut head_position = 0usize;
    let old_paths = read_c_strings(head, &mut head_position, old_path_count, old_path_sum_size)?;
    let new_paths = read_c_strings(head, &mut head_position, new_path_count, new_path_sum_size)?;
    let old_ref_list = read_inc_list(head, &mut head_position, old_ref_count, old_path_count)?;
    let new_ref_list = read_inc_list(head, &mut head_position, new_ref_count, new_path_count)?;
    let mut new_ref_sizes = Vec::with_capacity(new_ref_count);
    for _ in 0..new_ref_count {
        new_ref_sizes.push(read_hdiff_varint(head, &mut head_position)?);
    }
    let same_pairs = read_same_pairs(head, &mut head_position, same_pair_count)?;
    let _execute_list = read_inc_list(head, &mut head_position, execute_count, new_path_count)?;
    if head_position + private_reserved_size > head.len() {
        return Err(DiffError::Invalid(
            "truncated directory patch private data".to_string(),
        ));
    }

    let mut old_ref_stream = Vec::new();
    for index in old_ref_list {
        let relative = old_paths
            .get(index)
            .ok_or_else(|| DiffError::Invalid("old reference index out of range".to_string()))?;
        let path = safe_join(old_dir, relative)?;
        let data = std::fs::read(&path)
            .map_err(|error| DiffError::Invalid(format!("{}: {error}", path.display())))?;
        old_ref_stream.extend_from_slice(&data);
    }
    if old_ref_stream.len() != old_ref_size {
        return Err(DiffError::Invalid(format!(
            "old reference stream size mismatch: {} != {old_ref_size}",
            old_ref_stream.len()
        )));
    }
    let new_ref_stream = apply_pure_hdiff(&old_ref_stream, hdiff)?;
    if new_ref_stream.len() != new_ref_size {
        return Err(DiffError::Invalid(format!(
            "new reference stream size mismatch: {} != {new_ref_size}",
            new_ref_stream.len()
        )));
    }

    let mut same_new_to_old = std::collections::BTreeMap::new();
    for (new_index, old_index) in same_pairs {
        if new_index >= new_path_count || old_index >= old_path_count {
            return Err(DiffError::Invalid(
                "same-file pair index out of range".to_string(),
            ));
        }
        same_new_to_old.insert(new_index, old_index);
    }
    let mut ref_offsets = std::collections::BTreeMap::new();
    let mut offset = 0usize;
    for (index, size) in new_ref_list.iter().zip(new_ref_sizes.iter()) {
        ref_offsets.insert(*index, (offset, *size));
        offset += *size;
    }

    for (index, relative) in new_paths.iter().enumerate() {
        if relative.is_empty() {
            continue;
        }
        let path = safe_join(out_dir, relative)?;
        if relative.ends_with('/') {
            std::fs::create_dir_all(&path)
                .map_err(|error| DiffError::Invalid(format!("{}: {error}", path.display())))?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| DiffError::Invalid(format!("{}: {error}", parent.display())))?;
        }
        if let Some(old_index) = same_new_to_old.get(&index) {
            let old_relative = old_paths.get(*old_index).ok_or_else(|| {
                DiffError::Invalid("same-file old path index out of range".to_string())
            })?;
            let old_path = safe_join(old_dir, old_relative)?;
            let data = std::fs::read(&old_path)
                .map_err(|error| DiffError::Invalid(format!("{}: {error}", old_path.display())))?;
            std::fs::write(&path, data)
                .map_err(|error| DiffError::Invalid(format!("{}: {error}", path.display())))?;
        } else if let Some((offset, size)) = ref_offsets.get(&index) {
            let end = offset.saturating_add(*size);
            let data = new_ref_stream.get(*offset..end).ok_or_else(|| {
                DiffError::Invalid("new reference stream is truncated".to_string())
            })?;
            std::fs::write(&path, data)
                .map_err(|error| DiffError::Invalid(format!("{}: {error}", path.display())))?;
        } else {
            std::fs::write(&path, [])
                .map_err(|error| DiffError::Invalid(format!("{}: {error}", path.display())))?;
        }
    }
    Ok(())
}

fn read_type_until(bytes: &[u8], position: &mut usize, terminator: u8) -> Result<String> {
    let start = *position;
    let offset = bytes
        .get(start..)
        .ok_or_else(|| DiffError::Invalid("truncated directory patch types".to_string()))?
        .iter()
        .position(|byte| *byte == terminator)
        .ok_or_else(|| DiffError::Invalid("unterminated directory patch type".to_string()))?;
    *position = start + offset + 1;
    Ok(String::from_utf8_lossy(&bytes[start..start + offset]).into_owned())
}

fn read_c_strings(
    bytes: &[u8],
    position: &mut usize,
    count: usize,
    expected_size: usize,
) -> Result<Vec<String>> {
    let start = *position;
    let end = start
        .checked_add(expected_size)
        .ok_or_else(|| DiffError::Invalid("path list size overflow".to_string()))?;
    let block = bytes
        .get(start..end)
        .ok_or_else(|| DiffError::Invalid("truncated directory path list".to_string()))?;
    *position = end;
    let mut names = Vec::with_capacity(count);
    let mut cursor = 0usize;
    for _ in 0..count {
        let terminator = block[cursor..]
            .iter()
            .position(|byte| *byte == 0)
            .ok_or_else(|| DiffError::Invalid("unterminated directory path".to_string()))?;
        let name = String::from_utf8_lossy(&block[cursor..cursor + terminator]).into_owned();
        names.push(name);
        cursor += terminator + 1;
    }
    Ok(names)
}

fn read_inc_list(
    bytes: &[u8],
    position: &mut usize,
    count: usize,
    limit: usize,
) -> Result<Vec<usize>> {
    let mut values = Vec::with_capacity(count);
    let mut previous = usize::MAX;
    for _ in 0..count {
        let increment = read_hdiff_varint(bytes, position)?;
        previous = previous.wrapping_add(1).wrapping_add(increment);
        if previous >= limit {
            return Err(DiffError::Invalid(
                "directory path index out of range".to_string(),
            ));
        }
        values.push(previous);
    }
    Ok(values)
}

fn read_same_pairs(
    bytes: &[u8],
    position: &mut usize,
    count: usize,
) -> Result<Vec<(usize, usize)>> {
    let mut pairs = Vec::with_capacity(count);
    let mut previous_new = usize::MAX;
    let mut previous_old = usize::MAX;
    for _ in 0..count {
        let increment_new = read_hdiff_varint(bytes, position)?;
        previous_new = previous_new.wrapping_add(1).wrapping_add(increment_new);
        let (increment_old, sign) = read_hdiff_tagged(bytes, position, 1)?;
        previous_old = if sign == 0 {
            previous_old.wrapping_add(1).wrapping_add(increment_old)
        } else {
            previous_old.wrapping_add(1).wrapping_sub(increment_old)
        };
        pairs.push((previous_new, previous_old));
    }
    Ok(pairs)
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf> {
    let relative = relative.replace('\\', "/");
    let mut path = PathBuf::from(root);
    for component in relative.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." || component.contains(':') {
            return Err(DiffError::Invalid(format!(
                "unsafe directory patch path: {relative:?}"
            )));
        }
        path.push(component);
    }
    Ok(path)
}

fn write_hdiff_varint(out: &mut Vec<u8>, value: usize) {
    write_hdiff_tagged(out, value, 0, 0);
}

fn write_hdiff_tagged(out: &mut Vec<u8>, value: usize, tag_bits: u32, tag: u8) {
    let value_bits = 7 - tag_bits;
    let max_first = (1usize << value_bits) - 1;
    let mut count = 0usize;
    while value.checked_shr(7 * count as u32).unwrap_or(0) > max_first {
        count += 1;
    }
    let mut first = (value.checked_shr(7 * count as u32).unwrap_or(0) & max_first) as u8;
    if tag_bits > 0 {
        first |= tag << (8 - tag_bits);
    }
    if count > 0 {
        first |= 1 << value_bits;
    }
    out.push(first);
    while count > 0 {
        count -= 1;
        let mut byte = ((value >> (7 * count)) & 0x7f) as u8;
        if count > 0 {
            byte |= 0x80;
        }
        out.push(byte);
    }
}

/// Returns true when the patch is an HDIFF13 file whose buffers can be
/// decompressed by this crate (none, zstd, zlib, bzip2 or lzma2).
pub fn is_pure_hdiff(patch: &[u8]) -> bool {
    parse_hdiff(patch).is_ok()
}

/// Returns true when the HDIFF13 buffers are stored without compression.
pub fn is_uncompressed_hdiff(patch: &[u8]) -> bool {
    parse_hdiff(patch)
        .map(|header| !header.compressed)
        .unwrap_or(false)
}

struct HdiffBuffers {
    new_size: usize,
    old_size: usize,
    cover_count: usize,
    cover_buf: Vec<u8>,
    rle_ctrl: Vec<u8>,
    rle_code: Vec<u8>,
    new_data_diff: Vec<u8>,
    compressed: bool,
}

fn parse_hdiff(patch: &[u8]) -> Result<HdiffBuffers> {
    const MAGIC: &[u8] = b"HDIFF13&";
    if !patch.starts_with(MAGIC) {
        return Err(DiffError::Invalid("not an HDIFF13 patch".to_string()));
    }
    let mut position = MAGIC.len();
    let type_end = patch[position..]
        .iter()
        .position(|byte| *byte == 0)
        .map(|index| position + index)
        .ok_or_else(|| DiffError::Invalid("HDIFF13 compress type is not terminated".to_string()))?;
    let compress_type = String::from_utf8_lossy(&patch[position..type_end]).into_owned();
    position = type_end + 1;

    let new_size = read_hdiff_varint(patch, &mut position)?;
    let old_size = read_hdiff_varint(patch, &mut position)?;
    let cover_count = read_hdiff_varint(patch, &mut position)?;
    let cover_size = read_hdiff_varint(patch, &mut position)?;
    let compressed_cover_size = read_hdiff_varint(patch, &mut position)?;
    let rle_ctrl_size = read_hdiff_varint(patch, &mut position)?;
    let compressed_rle_ctrl_size = read_hdiff_varint(patch, &mut position)?;
    let rle_code_size = read_hdiff_varint(patch, &mut position)?;
    let compressed_rle_code_size = read_hdiff_varint(patch, &mut position)?;
    let new_data_diff_size = read_hdiff_varint(patch, &mut position)?;
    let compressed_new_data_diff_size = read_hdiff_varint(patch, &mut position)?;

    let mut compressed = false;
    let mut take_buffer =
        |size: usize, compressed_size: usize, expected: usize| -> Result<Vec<u8>> {
            let stored_size = if compressed_size > 0 {
                compressed = true;
                compressed_size
            } else {
                size
            };
            let end = position
                .checked_add(stored_size)
                .ok_or_else(|| DiffError::Invalid("HDIFF13 buffer sizes overflow".to_string()))?;
            if end > patch.len() {
                return Err(DiffError::Invalid(
                    "HDIFF13 buffers exceed the patch file".to_string(),
                ));
            }
            let data = &patch[position..end];
            position = end;
            if compressed_size > 0 {
                decompress_hdiff_buffer(&compress_type, data, expected)
            } else {
                if data.len() != expected {
                    return Err(DiffError::Invalid(
                        "HDIFF13 buffer size mismatch".to_string(),
                    ));
                }
                Ok(data.to_vec())
            }
        };

    let cover_buf = take_buffer(cover_size, compressed_cover_size, cover_size)?;
    let rle_ctrl = take_buffer(rle_ctrl_size, compressed_rle_ctrl_size, rle_ctrl_size)?;
    let rle_code = take_buffer(rle_code_size, compressed_rle_code_size, rle_code_size)?;
    let new_data_diff = take_buffer(
        new_data_diff_size,
        compressed_new_data_diff_size,
        new_data_diff_size,
    )?;
    Ok(HdiffBuffers {
        new_size,
        old_size,
        cover_count,
        cover_buf,
        rle_ctrl,
        rle_code,
        new_data_diff,
        compressed,
    })
}

fn decompress_hdiff_buffer(compress_type: &str, data: &[u8], expected: usize) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut decoded = Vec::new();
    match compress_type {
        "zstd" => {
            decoded = zstd::stream::decode_all(std::io::Cursor::new(data))
                .map_err(|error| DiffError::Invalid(format!("HDIFF13 zstd: {error}")))?;
        }
        "zlib" => {
            flate2::read::ZlibDecoder::new(data)
                .read_to_end(&mut decoded)
                .map_err(DiffError::Io)?;
        }
        "bzip2" => {
            bzip2::read::BzDecoder::new(data)
                .read_to_end(&mut decoded)
                .map_err(DiffError::Io)?;
        }
        "lzma2" => {
            use liblzma::stream::{Action, Filters, Status, Stream};
            if data.is_empty() {
                return Err(DiffError::Invalid("empty HDIFF13 lzma2 buffer".to_string()));
            }
            let mut filters = Filters::new();
            filters
                .lzma2_properties(&data[..1])
                .map_err(|error| DiffError::Invalid(format!("HDIFF13 lzma2: {error}")))?;
            let mut stream = Stream::new_raw_decoder(&filters)
                .map_err(|error| DiffError::Invalid(format!("HDIFF13 lzma2: {error}")))?;
            decoded.reserve(expected.max(1024));
            let mut input = &data[1..];
            loop {
                decoded.reserve(64 * 1024);
                let before_in = stream.total_in();
                let before_out = decoded.len();
                let status = stream
                    .process_vec(input, &mut decoded, Action::Finish)
                    .map_err(|error| DiffError::Invalid(format!("HDIFF13 lzma2: {error}")))?;
                let consumed = (stream.total_in() - before_in) as usize;
                if consumed > input.len() {
                    return Err(DiffError::Invalid(
                        "HDIFF13 lzma2 consumed beyond input".to_string(),
                    ));
                }
                input = &input[consumed..];
                if status == Status::StreamEnd {
                    break;
                }
                if decoded.len() > expected {
                    return Err(DiffError::Invalid(
                        "HDIFF13 lzma2 output exceeds expected size".to_string(),
                    ));
                }
                if input.is_empty() && decoded.len() == before_out {
                    return Err(DiffError::Invalid(
                        "HDIFF13 lzma2 decoder made no progress".to_string(),
                    ));
                }
            }
        }
        other => {
            return Err(DiffError::Invalid(format!(
                "compressed HDIFF13 ({other}) requires hpatchz"
            )));
        }
    }
    if decoded.len() != expected {
        return Err(DiffError::Invalid(format!(
            "HDIFF13 decompressed size {} does not match {expected}",
            decoded.len()
        )));
    }
    Ok(decoded)
}

fn read_hdiff_varint(bytes: &[u8], position: &mut usize) -> Result<usize> {
    let (value, _) = read_hdiff_tagged(bytes, position, 0)?;
    Ok(value)
}

fn read_hdiff_tagged(bytes: &[u8], position: &mut usize, tag_bits: u32) -> Result<(usize, u8)> {
    let first = *bytes
        .get(*position)
        .ok_or_else(|| DiffError::Invalid("truncated HDIFF13 integer".to_string()))?;
    *position += 1;
    let value_bits = 7 - tag_bits;
    let value_mask = (1u64 << value_bits) - 1;
    let continuation_mask = 1u64 << value_bits;
    let tag = if tag_bits == 0 {
        0
    } else {
        first >> (8 - tag_bits)
    };
    let mut value = (first as u64) & value_mask;
    let mut continuation = first as u64 & continuation_mask != 0;
    while continuation {
        let byte = *bytes
            .get(*position)
            .ok_or_else(|| DiffError::Invalid("truncated HDIFF13 integer".to_string()))?;
        *position += 1;
        value = value
            .checked_shl(7)
            .ok_or_else(|| DiffError::Invalid("HDIFF13 integer too large".to_string()))?
            | (byte as u64 & 0x7f);
        continuation = byte & 0x80 != 0;
    }
    usize::try_from(value)
        .map(|value| (value, tag))
        .map_err(|_| DiffError::Invalid("HDIFF13 integer does not fit in usize".to_string()))
}

struct HdiffRle<'a> {
    ctrl: &'a [u8],
    code: &'a [u8],
    ctrl_pos: usize,
    code_pos: usize,
    run_type: u8,
    run_left: usize,
    run_value: u8,
}

impl<'a> HdiffRle<'a> {
    fn new(ctrl: &'a [u8], code: &'a [u8]) -> Self {
        Self {
            ctrl,
            code,
            ctrl_pos: 0,
            code_pos: 0,
            run_type: 0,
            run_left: 0,
            run_value: 0,
        }
    }

    fn next(&mut self) -> Result<u8> {
        if self.run_left == 0 {
            let (length, kind) = read_hdiff_tagged(self.ctrl, &mut self.ctrl_pos, 2)?;
            self.run_left = length
                .checked_add(1)
                .ok_or_else(|| DiffError::Invalid("HDIFF13 RLE length overflow".to_string()))?;
            self.run_type = kind;
            match kind {
                0 => self.run_value = 0,
                1 => self.run_value = 255,
                2 => {
                    self.run_value = *self.code.get(self.code_pos).ok_or_else(|| {
                        DiffError::Invalid("truncated HDIFF13 RLE value".to_string())
                    })?;
                    self.code_pos += 1;
                }
                3 => {}
                _ => unreachable!(),
            }
        }
        let value = match self.run_type {
            3 => {
                let value = *self
                    .code
                    .get(self.code_pos)
                    .ok_or_else(|| DiffError::Invalid("truncated HDIFF13 RLE data".to_string()))?;
                self.code_pos += 1;
                value
            }
            _ => self.run_value,
        };
        self.run_left -= 1;
        Ok(value)
    }

    fn skip(&mut self, mut length: usize) -> Result<()> {
        while length > 0 {
            if self.run_left == 0 {
                self.next()?;
                length -= 1;
            } else {
                let step = self.run_left.min(length);
                if self.run_type == 3 {
                    self.code_pos = self
                        .code_pos
                        .checked_add(step)
                        .ok_or_else(|| DiffError::Invalid("HDIFF13 RLE overflow".to_string()))?;
                    if self.code_pos > self.code.len() {
                        return Err(DiffError::Invalid("truncated HDIFF13 RLE data".to_string()));
                    }
                }
                self.run_left -= step;
                length -= step;
            }
        }
        Ok(())
    }

    fn is_finished(&self) -> bool {
        self.run_left == 0 && self.ctrl_pos == self.ctrl.len() && self.code_pos == self.code.len()
    }
}

pub fn apply_pure_hdiff(old: &[u8], patch: &[u8]) -> Result<Vec<u8>> {
    let header = parse_hdiff(patch)?;
    if header.old_size != old.len() {
        return Err(DiffError::Invalid(format!(
            "HDIFF13 old size {} does not match input {}",
            header.old_size,
            old.len()
        )));
    }

    let mut cover_pos = 0usize;
    let mut old_pos = 0usize;
    let mut new_pos_back = 0usize;
    let mut diff_pos = 0usize;
    let mut rle = HdiffRle::new(&header.rle_ctrl, &header.rle_code);
    let mut new_data = Vec::with_capacity(header.new_size);

    for _ in 0..header.cover_count {
        let (old_delta, sign) = read_hdiff_tagged(&header.cover_buf, &mut cover_pos, 1)?;
        let (new_gap, _) = read_hdiff_tagged(&header.cover_buf, &mut cover_pos, 0)?;
        let (length, _) = read_hdiff_tagged(&header.cover_buf, &mut cover_pos, 0)?;
        old_pos = if sign == 1 {
            old_pos
                .checked_sub(old_delta)
                .ok_or_else(|| DiffError::Invalid("HDIFF13 old position underflow".to_string()))?
        } else {
            old_pos
                .checked_add(old_delta)
                .ok_or_else(|| DiffError::Invalid("HDIFF13 old position overflow".to_string()))?
        };
        let new_pos = new_pos_back
            .checked_add(new_gap)
            .ok_or_else(|| DiffError::Invalid("HDIFF13 new position overflow".to_string()))?;
        if new_pos < new_pos_back || new_pos + length > header.new_size {
            return Err(DiffError::Invalid(
                "HDIFF13 cover exceeds new data".to_string(),
            ));
        }
        if old_pos + length > old.len() {
            return Err(DiffError::Invalid(
                "HDIFF13 cover exceeds old data".to_string(),
            ));
        }

        let literal_length = new_pos - new_pos_back;
        if diff_pos + literal_length > header.new_data_diff.len() {
            return Err(DiffError::Invalid(
                "HDIFF13 literal data is truncated".to_string(),
            ));
        }
        new_data.extend_from_slice(&header.new_data_diff[diff_pos..diff_pos + literal_length]);
        diff_pos += literal_length;
        rle.skip(literal_length)?;

        for offset in 0..length {
            let old_byte = old[old_pos + offset];
            let delta = rle.next()?;
            new_data.push(old_byte.wrapping_add(delta));
        }
        new_pos_back = new_pos + length;
        old_pos += length;
    }

    if new_pos_back > header.new_size {
        return Err(DiffError::Invalid(
            "HDIFF13 covers exceed new data".to_string(),
        ));
    }
    let tail = header.new_size - new_pos_back;
    if diff_pos + tail > header.new_data_diff.len() {
        return Err(DiffError::Invalid(
            "HDIFF13 literal tail is truncated".to_string(),
        ));
    }
    new_data.extend_from_slice(&header.new_data_diff[diff_pos..diff_pos + tail]);
    diff_pos += tail;
    rle.skip(tail)?;
    if diff_pos != header.new_data_diff.len() || !rle.is_finished() {
        return Err(DiffError::Invalid(
            "HDIFF13 patch has trailing or unconsumed data".to_string(),
        ));
    }
    if new_data.len() != header.new_size {
        return Err(DiffError::Invalid(
            "HDIFF13 output size mismatch".to_string(),
        ));
    }
    Ok(new_data)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zlib_lzma2_bzip2_hdiff_encoders_roundtrip() {
        let large = vec![b'B'; 50_000];
        for (name, patch) in [
            ("zlib", create_pure_hdiff_zlib(&[], &large)),
            ("lzma2", create_pure_hdiff_lzma2(&[], &large)),
            ("bzip2", create_pure_hdiff_bzip2(&[], &large)),
        ] {
            assert!(
                patch.starts_with(format!("HDIFF13&{name}\0").as_bytes()),
                "{name}"
            );
            assert!(patch.len() < 10_000, "{name}: {}", patch.len());
            assert_eq!(apply_pure_hdiff(&[], &patch).unwrap(), large, "{name}");
        }
    }

    #[test]
    fn zlib_lzma2_bzip2_dir_hdiff_roundtrip() {
        let workspace = tempfile::tempdir().unwrap();
        let old = workspace.path().join("old");
        let new = workspace.path().join("new");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("keep.txt"), b"old").unwrap();
        std::fs::write(new.join("keep.txt"), b"new").unwrap();
        std::fs::write(new.join("large.txt"), vec![b'C'; 50_000]).unwrap();

        for (name, create) in [
            (
                "zlib",
                create_pure_dir_hdiff_zlib as fn(&Path, &Path) -> Result<Vec<u8>>,
            ),
            (
                "lzma2",
                create_pure_dir_hdiff_lzma2 as fn(&Path, &Path) -> Result<Vec<u8>>,
            ),
            (
                "bzip2",
                create_pure_dir_hdiff_bzip2 as fn(&Path, &Path) -> Result<Vec<u8>>,
            ),
        ] {
            let out = workspace.path().join(format!("out-{name}"));
            let patch = create(&old, &new).unwrap();
            assert!(
                patch.starts_with(format!("HDIFF19&{name}&fadler64\0").as_bytes()),
                "{name}"
            );
            apply_pure_dir_hdiff(&old, &patch, &out).unwrap();
            assert_eq!(std::fs::read(out.join("keep.txt")).unwrap(), b"new");
            assert_eq!(std::fs::read(out.join("large.txt")).unwrap().len(), 50_000);
        }
    }

    #[test]
    fn zstd_hdiff_encoder_roundtrip() {
        let old = b"the quick brown fox jumps over the lazy dog\nwith a second line\n";
        let new = b"the quick red fox jumps over a very lazy dog\nwith a changed second line\nand a tail\n";
        let patch = create_pure_hdiff_zstd(old, new);
        assert!(patch.starts_with(b"HDIFF13&zstd\0"));
        assert_eq!(apply_pure_hdiff(old, &patch).unwrap(), new);

        let large_new = vec![b'A'; 50_000];
        let large_patch = create_pure_hdiff_zstd(&[], &large_new);
        assert!(large_patch.len() < 10_000, "{}", large_patch.len());
        assert_eq!(apply_pure_hdiff(&[], &large_patch).unwrap(), large_new);
    }

    #[test]
    fn zstd_dir_hdiff_roundtrip() {
        let workspace = tempfile::tempdir().unwrap();
        let old = workspace.path().join("old");
        let new = workspace.path().join("new");
        let out = workspace.path().join("out");
        std::fs::create_dir_all(old.join("nested")).unwrap();
        std::fs::create_dir_all(new.join("sub")).unwrap();
        std::fs::write(old.join("keep.txt"), b"old").unwrap();
        std::fs::write(old.join("same.txt"), b"same").unwrap();
        std::fs::write(old.join("remove.txt"), b"remove").unwrap();
        std::fs::write(old.join("nested/old.txt"), b"old nested").unwrap();
        std::fs::write(new.join("keep.txt"), b"new").unwrap();
        std::fs::write(new.join("same.txt"), b"same").unwrap();
        std::fs::write(new.join("added.txt"), b"added").unwrap();
        std::fs::write(new.join("sub/deep.txt"), b"deep").unwrap();
        std::fs::write(new.join("large.txt"), vec![b'x'; 50_000]).unwrap();

        let plain = create_pure_dir_hdiff(&old, &new).unwrap();
        let patch = create_pure_dir_hdiff_zstd(&old, &new).unwrap();
        assert!(patch.starts_with(b"HDIFF19&zstd&fadler64\0"));
        assert!(
            patch.len() < plain.len(),
            "{} !< {}",
            patch.len(),
            plain.len()
        );
        apply_pure_dir_hdiff(&old, &patch, &out).unwrap();
        assert_eq!(std::fs::read(out.join("keep.txt")).unwrap(), b"new");
        assert_eq!(std::fs::read(out.join("same.txt")).unwrap(), b"same");
        assert_eq!(std::fs::read(out.join("sub/deep.txt")).unwrap(), b"deep");
        assert!(!out.join("remove.txt").exists());
    }

    #[test]
    fn fadler64_matches_official_directory_checksums() {
        assert_eq!(fadler64(b""), 1);
        assert_eq!(fadler64(b"old"), 0xe668_d4dc_b4ae_66a5);
        assert_eq!(fadler64(b"newadded"), 0xb9be_39c5_b556_1a97);
        assert_eq!(fadler64(b"same"), 0xc1e5_0323_ae2a_402c);
    }

    #[test]
    fn pure_dir_hdiff_roundtrip() {
        let workspace = tempfile::tempdir().unwrap();
        let old = workspace.path().join("old");
        let new = workspace.path().join("new");
        let out = workspace.path().join("out");
        std::fs::create_dir_all(old.join("nested")).unwrap();
        std::fs::create_dir_all(new.join("sub")).unwrap();
        std::fs::write(old.join("keep.txt"), b"old").unwrap();
        std::fs::write(old.join("same.txt"), b"same").unwrap();
        std::fs::write(old.join("remove.txt"), b"remove").unwrap();
        std::fs::write(old.join("nested/old.txt"), b"old nested").unwrap();
        std::fs::write(new.join("keep.txt"), b"new").unwrap();
        std::fs::write(new.join("same.txt"), b"same").unwrap();
        std::fs::write(new.join("added.txt"), b"added").unwrap();
        std::fs::write(new.join("empty.txt"), b"").unwrap();
        std::fs::write(new.join("sub/deep.txt"), b"deep").unwrap();

        let patch = create_pure_dir_hdiff(&old, &new).unwrap();
        assert!(patch.starts_with(b"HDIFF19&&fadler64\0"));
        apply_pure_dir_hdiff(&old, &patch, &out).unwrap();

        assert_eq!(std::fs::read(out.join("keep.txt")).unwrap(), b"new");
        assert_eq!(std::fs::read(out.join("same.txt")).unwrap(), b"same");
        assert_eq!(std::fs::read(out.join("added.txt")).unwrap(), b"added");
        assert_eq!(std::fs::read(out.join("empty.txt")).unwrap(), b"");
        assert_eq!(std::fs::read(out.join("sub/deep.txt")).unwrap(), b"deep");
        assert!(!out.join("remove.txt").exists());
        assert!(!out.join("nested/old.txt").exists());
    }

    #[test]
    fn uncompressed_hdiff_roundtrip() {
        let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let old = std::fs::read(workspace.join("fixtures/binary/old.bin")).unwrap();
        let expected = std::fs::read(workspace.join("fixtures/binary/new.bin")).unwrap();
        let patch = std::fs::read(workspace.join("fixtures/hdiff/change.hdiff")).unwrap();
        assert!(is_uncompressed_hdiff(&patch));
        let actual = apply_pure_hdiff(&old, &patch).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn compressed_zstd_hdiff_roundtrip() {
        let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let old = std::fs::read(workspace.join("fixtures/hdiff/large-old.bin")).unwrap();
        let expected = std::fs::read(workspace.join("fixtures/hdiff/large-new.bin")).unwrap();
        let patch = std::fs::read(workspace.join("fixtures/hdiff/large-zstd.hdiff")).unwrap();
        assert!(is_pure_hdiff(&patch));
        assert!(!is_uncompressed_hdiff(&patch));
        let actual = apply_pure_hdiff(&old, &patch).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn compressed_lzma2_hdiff_roundtrip() {
        let workspace = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let old = std::fs::read(workspace.join("fixtures/hdiff/large-old.bin")).unwrap();
        let expected = std::fs::read(workspace.join("fixtures/hdiff/large-new.bin")).unwrap();
        let patch = std::fs::read(workspace.join("fixtures/hdiff/large-lzma2.hdiff")).unwrap();
        assert!(is_pure_hdiff(&patch));
        assert!(!is_uncompressed_hdiff(&patch));
        let actual = apply_pure_hdiff(&old, &patch).unwrap();
        assert_eq!(actual, expected);
    }

    #[test]
    fn pure_hdiff_encoder_roundtrip() {
        let old = b"the quick brown fox jumps over the lazy dog\nwith a second line\n";
        let new = b"the quick red fox jumps over a very lazy dog\nwith a changed second line\nand an extra tail\n";
        let patch = create_pure_hdiff(old, new);
        assert!(is_uncompressed_hdiff(&patch));
        assert_eq!(apply_pure_hdiff(old, &patch).unwrap(), new);

        let empty = create_pure_hdiff(b"", b"new file\n");
        assert_eq!(apply_pure_hdiff(b"", &empty).unwrap(), b"new file\n");
        let removed = create_pure_hdiff(b"removed file\n", b"");
        assert_eq!(apply_pure_hdiff(b"removed file\n", &removed).unwrap(), b"");
    }

    #[test]
    fn patch_roundtrip() {
        let old = b"the quick brown fox jumps over the lazy dog";
        let new = b"the quick red fox jumps over the very lazy dog";
        let patch = create_patch(old, new).unwrap();
        let restored = apply_patch(old, &patch).unwrap();
        assert_eq!(restored, new);
    }
}
