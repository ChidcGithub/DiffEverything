//! Unit tests for this module, compiled as its child through `#[path]`.
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
        completeness: crate::model::ComparisonCompleteness::Complete,
        expanded_bytes: 0,
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
