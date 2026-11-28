//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn counts_insertions_and_deletions() {
    let diff = compare_text("a\nb\nc\n", "a\nB\nc\nd\n", 3, 100);
    assert_eq!(diff.added_lines, 2);
    assert_eq!(diff.removed_lines, 1);
    assert_eq!(diff.hunks.len(), 1);
}

#[test]
fn equal_text_has_no_hunks() {
    let diff = compare_text("same\n", "same\n", 3, 100);
    assert_eq!(diff.added_lines, 0);
    assert_eq!(diff.removed_lines, 0);
    assert!(diff.hunks.is_empty());
}

#[test]
fn can_ignore_whitespace_and_case() {
    let left = "Hello\n    value = 1\n";
    let right = "hello\n\tvalue = 1\n";
    let diff = compare_text_options(left, right, 3, 100, true, true);
    assert_eq!(diff.added_lines, 0);
    assert_eq!(diff.removed_lines, 0);

    let diff = compare_text_options(left, right, 3, 100, false, false);
    assert!(diff.added_lines > 0 || diff.removed_lines > 0);
}

#[test]
fn keeps_original_text_when_normalizing() {
    let left = "Keep Case\nvalue = 1\n";
    let right = "keep case\nvalue = 2\n";
    let diff = compare_text_options(left, right, 3, 100, false, true);
    let hunk = &diff.hunks[0];
    assert!(
        hunk.lines
            .iter()
            .any(|line| line.tag == TextTag::Equal && line.text == "Keep Case")
    );
}

#[test]
fn annotates_changed_words() {
    let diff = compare_text("let value = 1;\n", "let value = 2;\n", 3, 100);
    let hunk = &diff.hunks[0];
    let deleted = hunk
        .lines
        .iter()
        .find(|line| line.tag == TextTag::Delete)
        .unwrap();
    let inserted = hunk
        .lines
        .iter()
        .find(|line| line.tag == TextTag::Insert)
        .unwrap();
    assert!(
        deleted
            .spans
            .iter()
            .any(|span| span.changed && span.text.contains('1'))
    );
    assert!(
        inserted
            .spans
            .iter()
            .any(|span| span.changed && span.text.contains('2'))
    );
}

#[test]
fn decodes_utf16le_bom() {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in "hello".encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let decoded = decode_text(&bytes);
    assert_eq!(decoded.text, "hello");
    assert_eq!(decoded.encoding, "UTF-16LE");
}

#[test]
fn byte_difference_ratio_averages_both_sides() {
    assert!((byte_difference_ratio("abcd", "ab") - 0.75).abs() < 0.001);
    assert!((byte_difference_ratio("same", "same") - 0.0).abs() < 0.001);
    assert!((byte_difference_ratio("", "x") - 1.0).abs() < 0.001);
    assert!((byte_difference_ratio("", "") - 0.0).abs() < 0.001);
}
