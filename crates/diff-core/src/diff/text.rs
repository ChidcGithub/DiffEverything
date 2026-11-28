use crate::model::{InlineSpan, TextDiff, TextHunk, TextLine, TextTag};
use similar::{Algorithm, ChangeTag, TextDiff as SimilarDiff};

/// Compare two decoded strings line-by-line, keeping a few lines of context
/// around every changed region.
fn byte_difference_ratio(left: &str, right: &str) -> f64 {
    let left = left.as_bytes();
    let right = right.as_bytes();
    let left_len = left.len() as f64;
    let right_len = right.len() as f64;
    if left.is_empty() && right.is_empty() {
        return 0.0;
    }
    if left.is_empty() || right.is_empty() {
        return 1.0;
    }
    let common = left.len().min(right.len());
    let mut changed = left.len().abs_diff(right.len()) as f64;
    for index in 0..common {
        if left[index] != right[index] {
            changed += 1.0;
        }
    }
    ((changed / left_len + changed / right_len) / 2.0).clamp(0.0, 1.0)
}

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
    if !ignore_whitespace && !ignore_case {
        return compare_text(left, right, context, max_hunks);
    }
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
    let original_left_lines: Vec<&str> = original_left.split_inclusive('\n').collect();
    let original_right_lines: Vec<&str> = original_right.split_inclusive('\n').collect();

    // Most large files differ only near the beginning or end. Strip the
    // common line prefix/suffix before running Myers/patience, so the diff
    // works on the small changed middle instead of the whole file. Without
    // normalization the two inputs are the same allocation, so reuse the
    // already split original lines instead of allocating a second copy.
    let left_diff_owned;
    let left_diff_lines: &[&str] = if std::ptr::eq(diff_left, original_left) {
        &original_left_lines
    } else {
        left_diff_owned = diff_left.split_inclusive('\n').collect::<Vec<_>>();
        &left_diff_owned
    };
    let right_diff_owned;
    let right_diff_lines: &[&str] = if std::ptr::eq(diff_right, original_right) {
        &original_right_lines
    } else {
        right_diff_owned = diff_right.split_inclusive('\n').collect::<Vec<_>>();
        &right_diff_owned
    };
    let mut common_prefix = 0usize;
    while common_prefix < left_diff_lines.len()
        && common_prefix < right_diff_lines.len()
        && left_diff_lines[common_prefix] == right_diff_lines[common_prefix]
    {
        common_prefix += 1;
    }
    let mut common_suffix = 0usize;
    while common_suffix < left_diff_lines.len().saturating_sub(common_prefix)
        && common_suffix < right_diff_lines.len().saturating_sub(common_prefix)
        && left_diff_lines[left_diff_lines.len() - 1 - common_suffix]
            == right_diff_lines[right_diff_lines.len() - 1 - common_suffix]
    {
        common_suffix += 1;
    }
    let middle_start = common_prefix.saturating_sub(context);
    let left_end =
        (left_diff_lines.len().saturating_sub(common_suffix) + context).min(left_diff_lines.len());
    let right_end = (right_diff_lines.len().saturating_sub(common_suffix) + context)
        .min(right_diff_lines.len());
    let left_middle = left_diff_lines[middle_start..left_end].concat();
    let right_middle = right_diff_lines[middle_start..right_end].concat();
    let diff = if left_middle.len().saturating_add(right_middle.len()) > 256 * 1024 {
        SimilarDiff::configure()
            .algorithm(Algorithm::Patience)
            .diff_lines(&left_middle, &right_middle)
    } else {
        SimilarDiff::from_lines(&left_middle, &right_middle)
    };
    let changes: Vec<_> = diff.iter_all_changes().collect();

    let added_lines = changes
        .iter()
        .filter(|change| change.tag() == ChangeTag::Insert)
        .count();
    let removed_lines = changes
        .iter()
        .filter(|change| change.tag() == ChangeTag::Delete)
        .count();

    let mut hunks: Vec<TextHunk> = Vec::new();
    let mut truncated = false;
    let mut index = 0usize;
    while index < changes.len() {
        while index < changes.len() && changes[index].tag() == ChangeTag::Equal {
            index += 1;
        }
        if index >= changes.len() {
            break;
        }
        if hunks.len() >= max_hunks {
            truncated = true;
            break;
        }
        let first_change = index;
        let mut last_change = index;
        let mut probe = index + 1;
        while probe < changes.len() {
            if changes[probe].tag() != ChangeTag::Equal {
                last_change = probe;
                probe += 1;
                continue;
            }
            let run_start = probe;
            while probe < changes.len() && changes[probe].tag() == ChangeTag::Equal {
                probe += 1;
            }
            if probe >= changes.len() {
                break;
            }
            // Keep nearby edits in one hunk when their context windows would
            // overlap, matching the previous range-merging behaviour.
            if probe - run_start <= context.saturating_mul(2) {
                last_change = probe;
                probe += 1;
            } else {
                break;
            }
        }

        let slice_start = first_change.saturating_sub(context);
        let slice_end = (last_change + context).min(changes.len().saturating_sub(1));
        let slice = &changes[slice_start..=slice_end];
        let old_start = slice
            .iter()
            .find_map(|change| change.old_index())
            .map(|index| index + middle_start + 1)
            .unwrap_or(middle_start + 1);
        let new_start = slice
            .iter()
            .find_map(|change| change.new_index())
            .map(|index| index + middle_start + 1)
            .unwrap_or(middle_start + 1);
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
                    old_lineno: change.old_index().map(|index| index + middle_start + 1),
                    new_lineno: change.new_index().map(|index| index + middle_start + 1),
                    text: original_line(
                        change.tag(),
                        change.old_index().map(|index| index + middle_start),
                        change.new_index().map(|index| index + middle_start),
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
        index = last_change + 1;
    }

    annotate_word_changes(&mut hunks);

    TextDiff {
        format: String::new(),
        encoding_left: String::new(),
        encoding_right: String::new(),
        left_lines: original_left_lines.len(),
        right_lines: original_right_lines.len(),
        byte_difference_ratio: byte_difference_ratio(original_left, original_right),
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
    if hunks.iter().map(|hunk| hunk.lines.len()).sum::<usize>() > 4_000 {
        return;
    }
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
#[path = "../../tests/unit/diff/text.rs"]
mod tests;
