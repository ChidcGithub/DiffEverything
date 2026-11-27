use crate::model::{BinaryDiff, HexRow};

const BYTES_PER_ROW: usize = 16;

/// Produce a human-readable hex diff. Only rows with differences are kept;
/// the scan still counts every differing byte so the summary is exact.
pub fn compare_binary(left: &[u8], right: &[u8], max_rows: usize) -> BinaryDiff {
    let max_len = left.len().max(right.len());
    let mut differing_bytes = 0u64;
    let mut first_difference = None;
    let mut rows = Vec::new();
    let mut truncated = false;

    let mut offset = 0usize;
    while offset < max_len {
        let end = (offset + BYTES_PER_ROW).min(max_len);
        let row_len = end - offset;
        let left_row = left.get(offset..end.min(left.len())).unwrap_or_default();
        let right_row = right.get(offset..end.min(right.len())).unwrap_or_default();

        if left_row != right_row {
            for index in 0..row_len {
                let left_byte = left.get(offset + index);
                let right_byte = right.get(offset + index);
                if left_byte != right_byte {
                    differing_bytes += 1;
                    if first_difference.is_none() {
                        first_difference = Some((offset + index) as u64);
                    }
                }
            }

            if rows.len() < max_rows {
                rows.push(HexRow {
                    offset: offset as u64,
                    left_hex: format_hex(left_row, row_len),
                    right_hex: format_hex(right_row, row_len),
                    left_ascii: format_ascii(left_row, row_len),
                    right_ascii: format_ascii(right_row, row_len),
                });
            } else {
                truncated = true;
            }
        }

        offset = end;
    }

    BinaryDiff {
        left_len: left.len() as u64,
        right_len: right.len() as u64,
        differing_bytes,
        first_difference,
        rows,
        truncated,
    }
}

fn format_hex(bytes: &[u8], len: usize) -> String {
    let mut out = String::with_capacity(len * 3);
    for index in 0..len {
        match bytes.get(index) {
            Some(byte) => out.push_str(&format!("{byte:02x} ")),
            None => out.push_str("-- "),
        }
    }
    out.trim_end().to_string()
}

fn format_ascii(bytes: &[u8], len: usize) -> String {
    let mut out = String::with_capacity(len);
    for index in 0..len {
        match bytes.get(index) {
            Some(byte) if byte.is_ascii_graphic() || *byte == b' ' => out.push(*byte as char),
            Some(_) => out.push('.'),
            None => out.push(' '),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_differing_bytes_and_first_offset() {
        let diff = compare_binary(&[0, 1, 2, 3], &[0, 1, 9, 3], 10);
        assert_eq!(diff.differing_bytes, 1);
        assert_eq!(diff.first_difference, Some(2));
        assert_eq!(diff.rows.len(), 1);
    }

    #[test]
    fn handles_length_change() {
        let diff = compare_binary(&[0, 1], &[0, 1, 2, 3], 10);
        assert_eq!(diff.differing_bytes, 2);
        assert_eq!(diff.first_difference, Some(2));
    }

    #[test]
    fn identical_input_is_empty() {
        let diff = compare_binary(b"same", b"same", 10);
        assert_eq!(diff.differing_bytes, 0);
        assert!(diff.rows.is_empty());
    }
}
