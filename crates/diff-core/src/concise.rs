use crate::model::{DiffBody, DiffNode, DiffReport, TextDiff, TextHunk, TextLine, TextTag};
use std::collections::BTreeMap;

const MAX_TEXT_LINES: usize = 10;
const MAX_CHANGES: usize = 4;
const MAX_CONTAINER_ENTRIES: usize = 16;
const MAX_ROWS: usize = 2;

/// Compress a report for terminal/brief consumption.
///
/// Concise mode removes unchanged context, drops equal container entries and
/// caps detail units. Full mode never calls this transform.
pub fn apply(report: &mut DiffReport) {
    apply_node(&mut report.root);
}

fn apply_node(node: &mut DiffNode) {
    match &mut node.body {
        DiffBody::Text(diff) => concise_text(diff),
        DiffBody::Tokens(diff) => {
            diff.chunks.retain(|chunk| chunk.tag != TextTag::Equal);
            limit(&mut diff.chunks, MAX_CHANGES, &mut diff.truncated);
        }
        DiffBody::Tree(diff) => limit(&mut diff.chunks, MAX_CHANGES, &mut diff.truncated),
        DiffBody::Notebook(diff) => {
            limit(&mut diff.changed_cells, MAX_CHANGES, &mut diff.truncated);
        }
        DiffBody::Structured(diff) => limit(&mut diff.changes, MAX_CHANGES, &mut diff.truncated),
        DiffBody::Table(diff) => limit(&mut diff.rows, MAX_CHANGES, &mut diff.truncated),
        DiffBody::Workbook(diff) => {
            limit(&mut diff.sheets_added, MAX_CHANGES, &mut diff.truncated);
            limit(&mut diff.sheets_removed, MAX_CHANGES, &mut diff.truncated);
            limit(&mut diff.sheets, MAX_CHANGES, &mut diff.truncated);
            for sheet in &mut diff.sheets {
                limit(&mut sheet.cells, MAX_CHANGES, &mut sheet.truncated);
            }
        }
        DiffBody::Image(diff) => {
            limit(&mut diff.regions, MAX_CHANGES, &mut diff.regions_truncated);
        }
        DiffBody::Audio(diff) => {
            if diff.tags.len() > MAX_CHANGES {
                diff.tags.truncate(MAX_CHANGES);
            }
            if let Some(waveform) = &mut diff.waveform {
                let windows = waveform.left.len().min(waveform.right.len());
                let mut left = Vec::new();
                let mut right = Vec::new();
                for index in 0..windows {
                    if (waveform.left[index] - waveform.right[index]).abs() > 0.01 {
                        left.push(waveform.left[index]);
                        right.push(waveform.right[index]);
                        if left.len() >= MAX_CHANGES {
                            break;
                        }
                    }
                }
                if left.len() < windows {
                    waveform.truncated = true;
                }
                waveform.left = left;
                waveform.right = right;
            }
        }
        DiffBody::Binary(diff) => limit(&mut diff.rows, MAX_ROWS, &mut diff.truncated),
        DiffBody::Directory(diff) => concise_entries(
            &mut diff.entries,
            &mut diff.total_entries,
            &mut diff.truncated,
        ),
        DiffBody::Archive(diff) => concise_entries(
            &mut diff.entries,
            &mut diff.total_entries,
            &mut diff.truncated,
        ),
        DiffBody::OneSided(_) | DiffBody::Equal(_) | DiffBody::Unsupported(_) => {}
    }
}

fn concise_text(diff: &mut TextDiff) {
    let hunks = std::mem::take(&mut diff.hunks);
    let mut flat: Vec<(usize, TextLine)> = Vec::new();
    for (hunk_index, hunk) in hunks.iter().enumerate() {
        for line in &hunk.lines {
            if line.tag != TextTag::Equal {
                flat.push((hunk_index, line.clone()));
            }
        }
    }
    let cut = flat.len() > MAX_TEXT_LINES;
    if cut {
        flat.truncate(MAX_TEXT_LINES);
        diff.truncated = true;
    }
    let mut grouped: BTreeMap<usize, Vec<TextLine>> = BTreeMap::new();
    for (hunk_index, line) in flat {
        grouped.entry(hunk_index).or_default().push(line);
    }
    diff.hunks = grouped
        .into_iter()
        .map(|(index, lines)| {
            let mut hunk = hunks[index].clone();
            hunk.lines = lines;
            rebuild_hunk(&mut hunk);
            hunk
        })
        .collect();
    diff.added_lines = diff
        .hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .filter(|line| line.tag == TextTag::Insert)
        .count();
    diff.removed_lines = diff
        .hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .filter(|line| line.tag == TextTag::Delete)
        .count();
}

fn rebuild_hunk(hunk: &mut TextHunk) {
    let old_lines: Vec<&TextLine> = hunk
        .lines
        .iter()
        .filter(|line| line.tag != TextTag::Insert && line.old_lineno.is_some())
        .collect();
    let new_lines: Vec<&TextLine> = hunk
        .lines
        .iter()
        .filter(|line| line.tag != TextTag::Delete && line.new_lineno.is_some())
        .collect();
    if let Some(first) = old_lines.first().and_then(|line| line.old_lineno) {
        hunk.old_start = first;
    }
    if let Some(first) = new_lines.first().and_then(|line| line.new_lineno) {
        hunk.new_start = first;
    }
    hunk.old_len = old_lines.len();
    hunk.new_len = new_lines.len();
}

fn concise_entries(entries: &mut Vec<DiffNode>, total_entries: &mut usize, truncated: &mut bool) {
    let mut kept = Vec::new();
    for mut entry in std::mem::take(entries) {
        apply_node(&mut entry);
        if entry.status != crate::model::DiffStatus::Equal {
            kept.push(entry);
        }
    }
    let cut = kept.len() > MAX_CONTAINER_ENTRIES;
    if cut {
        kept.truncate(MAX_CONTAINER_ENTRIES);
        *truncated = true;
    }
    *total_entries = kept.len();
    *entries = kept;
}

fn limit<T>(items: &mut Vec<T>, max: usize, truncated: &mut bool) {
    if items.len() > max {
        items.truncate(max);
        *truncated = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ArchiveDiff, DiffStatus, EqualInfo, FileKind, TextHunk};

    fn text_node() -> DiffNode {
        DiffNode::new(
            "text",
            DiffStatus::Different,
            DiffBody::Text(TextDiff {
                format: "text".into(),
                encoding_left: "utf-8".into(),
                encoding_right: "utf-8".into(),
                left_lines: 2,
                right_lines: 2,
                byte_difference_ratio: 0.0,
                added_lines: 1,
                removed_lines: 1,
                hunks: vec![TextHunk {
                    old_start: 1,
                    old_len: 2,
                    new_start: 1,
                    new_len: 2,
                    lines: vec![
                        TextLine {
                            tag: TextTag::Equal,
                            old_lineno: Some(1),
                            new_lineno: Some(1),
                            text: "same".into(),
                            spans: Vec::new(),
                        },
                        TextLine {
                            tag: TextTag::Delete,
                            old_lineno: Some(2),
                            new_lineno: None,
                            text: "old".into(),
                            spans: Vec::new(),
                        },
                        TextLine {
                            tag: TextTag::Insert,
                            old_lineno: None,
                            new_lineno: Some(2),
                            text: "new".into(),
                            spans: Vec::new(),
                        },
                    ],
                }],
                truncated: false,
            }),
        )
    }

    fn report(node: DiffNode) -> DiffReport {
        DiffReport {
            tool: "test",
            version: "0",
            left: crate::model::ArtifactInfo {
                path: "left".into(),
                kind: FileKind::Text,
                size: 0,
            },
            right: crate::model::ArtifactInfo {
                path: "right".into(),
                kind: FileKind::Text,
                size: 0,
            },
            status: DiffStatus::Different,
            summary: Default::default(),
            root: node,
            warnings: Vec::new(),
            elapsed_ms: 0,
            completely_different: false,
            decompiled: None,
        }
    }

    #[test]
    fn concise_text_drops_equal_context() {
        let mut report = report(text_node());
        apply(&mut report);
        match &report.root.body {
            DiffBody::Text(diff) => {
                assert_eq!(diff.hunks[0].lines.len(), 2);
                assert!(
                    diff.hunks[0]
                        .lines
                        .iter()
                        .all(|line| line.tag != TextTag::Equal)
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn concise_archive_drops_equal_entries() {
        let equal = DiffNode::new(
            "same.txt",
            DiffStatus::Equal,
            DiffBody::Equal(EqualInfo {
                kind: FileKind::Text,
                note: String::new(),
            }),
        );
        let changed = text_node();
        let mut report = report(DiffNode::new(
            "archive",
            DiffStatus::Different,
            DiffBody::Archive(ArchiveDiff {
                format: "zip".into(),
                entries: vec![equal, changed],
                total_entries: 2,
                truncated: false,
            }),
        ));
        apply(&mut report);
        match &report.root.body {
            DiffBody::Archive(diff) => {
                assert_eq!(diff.entries.len(), 1);
                assert_eq!(diff.entries[0].label, "text");
                assert_eq!(diff.total_entries, 1);
            }
            other => panic!("{other:?}"),
        }
    }
}
