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
}

#[test]
fn identical_input_is_empty() {
    let diff = compare_binary(b"same", b"same", 10);
    assert_eq!(diff.differing_bytes, 0);
    assert!(diff.rows.is_empty());
}
