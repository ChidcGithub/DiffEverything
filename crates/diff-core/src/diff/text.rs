use crate::model::{InlineSpan, TextDiff, TextHunk, TextLine, TextTag};
use similar::{ChangeTag, TextDiff as SimilarDiff};

/// Compare two decoded strings line-by-line, keeping a few lines of context
/// around every changed region.
pub fn compare_text(left: &str, right: &str, context: usize, max_hunks: usize) -> TextDiff {
    compare_text_impl(left, right, left, right, context, max_hunks)
}

/// Compare with whitespace/case normalization. The normalized text is used
/// only for matching lines; the report still displays the original lines.
pub fn compare_text_options(
    left: &str,
    right: &str,
    context: usize,
    max_hunks: usize,
    ignore_whitespace: bool,
    ignore_case: bool,
) -> TextDiff {
    let normalized_left = normalize_for_compare(left, ignore_whitespace, ignore_case);
    let normalized_right = normalize_for_compare(right, ignore_whitespace, ignore_case);
    compare_text_impl(
        &normalized_left,
        &normalized_right,
        left,
        right,
        context,
        max_hunks,
    )
}

fn normalize_for_compare(text: &str, ignore_whitespace: bool, ignore_case: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\n', '\r']);
        let mut body = if ignore_case {
            body.to_lowercase()
        } else {
            body.to_string()
        };
        if ignore_whitespace {
            body = body.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        out.push_str(&body);
        out.push('\n');
    }
    out
}

fn compare_text_impl(
    diff_left: &str,
    diff_right: &str,
    original_left: &str,
    original_right: &str,
    context: usize,
    max_hunks: usize,
) -> TextDiff {
    let diff = SimilarDiff::from_lines(diff_left, diff_right);
    let changes: Vec<_> = diff.iter_all_changes().collect();
    let original_left_lines: Vec<&str> = original_left.split_inclusive('\n').collect();
    let original_right_lines: Vec<&str> = original_right.split_inclusive('\n').collect();

    let added_lines = changes
        .iter()
        .filter(|change| change.tag() == ChangeTag::Insert)
        .count();
    let removed_lines = changes
        .iter()
        .filter(|change| change.tag() == ChangeTag::Delete)
        .count();

    let mut ranges: Vec<(usize, usize)> = Vec::new();
    let mut current: Option<(usize, usize)> = None;
    for (index, change) in changes.iter().enumerate() {
        if change.tag() == ChangeTag::Equal {
            continue;
        }
        match current {
            Some((start, end)) if index <= end + context * 2 + 1 => {
                current = Some((start, index));
            }
            Some((start, end)) => {
                ranges.push((start, end));
                current = Some((index, index));
            }
            None => current = Some((index, index)),
        }
    }
    if let Some(range) = current {
        ranges.push(range);
    }

    let mut expanded: Vec<(usize, usize)> = ranges
        .into_iter()
        .map(|(start, end)| {
            (
                start.saturating_sub(context),
                (end + context).min(changes.len().saturating_sub(1)),
            )
        })
        .collect();

    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (start, end) in expanded.drain(..) {
        if let Some(last) = merged.last_mut()
            && start <= last.1.saturating_add(1)
        {
            last.1 = last.1.max(end);
            continue;
        }
        merged.push((start, end));
    }

    let mut hunks: Vec<TextHunk> = Vec::new();
    let mut truncated = false;
    for (start, end) in merged {
        if hunks.len() >= max_hunks {
            truncated = true;
            break;
        }
        let slice = &changes[start..=end];
        let old_start = slice
            .iter()
            .find_map(|change| change.old_index())
            .map(|index| index + 1)
            .unwrap_or(1);
        let new_start = slice
            .iter()
            .find_map(|change| change.new_index())
            .map(|index| index + 1)
            .unwrap_or(1);
        let old_len = slice
            .iter()
            .filter(|change| change.old_index().is_some())
            .count();
        let new_len = slice
            .iter()
            .filter(|change| change.new_index().is_some())
            .count();

        let lines = slice
            .iter()
            .map(|change| {
                let tag = match change.tag() {
                    ChangeTag::Equal => TextTag::Equal,
                    ChangeTag::Delete => TextTag::Delete,
                    ChangeTag::Insert => TextTag::Insert,
                };
                TextLine {
                    tag,
                    old_lineno: change.old_index().map(|index| index + 1),
                    new_lineno: change.new_index().map(|index| index + 1),
                    text: original_line(
                        change.tag(),
                        change.old_index(),
                        change.new_index(),
                        &original_left_lines,
                        &original_right_lines,
                    ),
                    spans: Vec::new(),
                }
            })
            .collect();

        hunks.push(TextHunk {
            old_start,
            old_len,
            new_start,
            new_len,
            lines,
        });
    }

    annotate_word_changes(&mut hunks);

    TextDiff {
        format: String::new(),
        encoding_left: String::new(),
        encoding_right: String::new(),
        left_lines: original_left_lines.len(),
        right_lines: original_right_lines.len(),
        added_lines,
        removed_lines,
        hunks,
        truncated,
    }
}

/// Pair consecutive deleted/inserted lines inside a hunk and mark the words
/// that actually changed. This mirrors the inline highlighting found in
/// difftastic and Open Diff without requiring a full syntax tree.
fn original_line(
    tag: ChangeTag,
    old_index: Option<usize>,
    new_index: Option<usize>,
    left_lines: &[&str],
    right_lines: &[&str],
) -> String {
    let (index, lines) = match tag {
        ChangeTag::Equal => (old_index.or(new_index), left_lines),
        ChangeTag::Delete => (old_index, left_lines),
        ChangeTag::Insert => (new_index, right_lines),
    };
    index
        .and_then(|index| lines.get(index))
        .map(|line| line.trim_end_matches(['\n', '\r']).to_string())
        .unwrap_or_default()
}

fn annotate_word_changes(hunks: &mut [TextHunk]) {
    for hunk in hunks {
        let mut index = 0;
        while index < hunk.lines.len() {
            if hunk.lines[index].tag != TextTag::Delete {
                index += 1;
                continue;
            }
            let delete_start = index;
            while index < hunk.lines.len() && hunk.lines[index].tag == TextTag::Delete {
                index += 1;
            }
            let delete_end = index;
            let insert_start = index;
            while index < hunk.lines.len() && hunk.lines[index].tag == TextTag::Insert {
                index += 1;
            }
            let insert_end = index;

            let pairs = (delete_end - delete_start).min(insert_end - insert_start);
            for offset in 0..pairs {
                let (old_spans, new_spans) = word_spans(
                    &hunk.lines[delete_start + offset].text,
                    &hunk.lines[insert_start + offset].text,
                );
                hunk.lines[delete_start + offset].spans = old_spans;
                hunk.lines[insert_start + offset].spans = new_spans;
            }
        }
    }
}

fn word_spans(old: &str, new: &str) -> (Vec<InlineSpan>, Vec<InlineSpan>) {
    if old.len() > 4000 || new.len() > 4000 {
        return (Vec::new(), Vec::new());
    }
    let diff = SimilarDiff::from_words(old, new);
    let mut old_spans = Vec::new();
    let mut new_spans = Vec::new();
    for change in diff.iter_all_changes() {
        let text = change.value();
        if text.is_empty() {
            continue;
        }
        match change.tag() {
            ChangeTag::Equal => {
                old_spans.push(InlineSpan {
                    text: text.to_string(),
                    changed: false,
                });
                new_spans.push(InlineSpan {
                    text: text.to_string(),
                    changed: false,
                });
            }
            ChangeTag::Delete => old_spans.push(InlineSpan {
                text: text.to_string(),
                changed: true,
            }),
            ChangeTag::Insert => new_spans.push(InlineSpan {
                text: text.to_string(),
                changed: true,
            }),
        }
    }
    (old_spans, new_spans)
}

#[derive(Debug, Clone)]
pub struct DecodedText {
    pub text: String,
    pub encoding: String,
    pub lossy: bool,
}

/// Decode bytes using BOM detection, UTF-8, then statistical detection.
/// This is what lets Windows/GBK/UTF-16 files compare as text rather than
/// noise.
pub fn decode_text(bytes: &[u8]) -> DecodedText {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return decode_utf8(rest, "UTF-8 (BOM)");
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let (text, _, lossy) = encoding_rs::UTF_16LE.decode(rest);
        return DecodedText {
            text: text.into_owned(),
            encoding: "UTF-16LE".to_string(),
            lossy,
        };
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let (text, _, lossy) = encoding_rs::UTF_16BE.decode(rest);
        return DecodedText {
            text: text.into_owned(),
            encoding: "UTF-16BE".to_string(),
            lossy,
        };
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        return DecodedText {
            text: text.to_string(),
            encoding: "UTF-8".to_string(),
            lossy: false,
        };
    }

    let mut detector = chardetng::EncodingDetector::new();
    detector.feed(bytes, true);
    let encoding = detector.guess(None, true);
    let (text, _, lossy) = encoding.decode(bytes);
    DecodedText {
        text: text.into_owned(),
        encoding: encoding.name().to_string(),
        lossy,
    }
}

fn decode_utf8(bytes: &[u8], label: &str) -> DecodedText {
    let text = String::from_utf8_lossy(bytes);
    DecodedText {
        text: text.into_owned(),
        encoding: label.to_string(),
        lossy: false,
    }
}

#[cfg(test)]
mod tests {
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
}
