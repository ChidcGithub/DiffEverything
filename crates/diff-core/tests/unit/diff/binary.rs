//! Unit tests for this module, compiled as its child through `#[path]`.
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
    assert!(diff.rows.is_empty());
    let tail = diff.tail.expect("tail summary");
    assert_eq!(tail.side, crate::model::PresenceSide::RightOnly);
    assert_eq!(tail.start, 2);
    assert_eq!(tail.length, 2);
    assert_eq!(tail.repeated_byte, None);
}

#[test]
fn summarizes_large_uniform_tail_without_hex_rows() {
    let left = vec![0xff; 4 * 1024 * 1024];
    let right = left[..1024].to_vec();
    let diff = compare_binary(&left, &right, 10);
    assert_eq!(diff.differing_bytes, (left.len() - right.len()) as u64);
    assert!(diff.rows.is_empty());
    assert_eq!(diff.first_difference, Some(1024));
    let tail = diff.tail.expect("tail summary");
    assert_eq!(tail.side, crate::model::PresenceSide::LeftOnly);
    assert_eq!(tail.repeated_byte, Some(0xff));
    assert!(tail.description().contains("left has"));
    assert!(tail.description().contains("0xff"));
}

#[test]
fn identical_input_is_empty() {
    let diff = compare_binary(b"same", b"same", 10);
    assert_eq!(diff.differing_bytes, 0);
    assert!(diff.rows.is_empty());
}
