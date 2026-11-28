//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;
use crate::model::{DiffStatus, EqualInfo, FileKind, TextHunk};

fn line(tag: TextTag, old: Option<usize>, new: Option<usize>, text: &str) -> TextLine {
    TextLine {
        tag,
        old_lineno: old,
        new_lineno: new,
        text: text.to_string(),
        spans: Vec::new(),
    }
}

fn report(node: DiffNode) -> DiffReport {
    DiffReport {
        tool: "test",
        version: "0",
        left: crate::model::ArtifactInfo {
            path: "left".into(),
            kind: crate::model::FileKind::Text,
            size: 0,
        },
        right: crate::model::ArtifactInfo {
            path: "right".into(),
            kind: crate::model::FileKind::Text,
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
fn head_and_last_limit_text_lines() {
    let lines = vec![
        line(TextTag::Delete, Some(1), None, "alpha"),
        line(TextTag::Insert, None, Some(1), "beta"),
        line(TextTag::Delete, Some(2), None, "gamma"),
        line(TextTag::Insert, None, Some(2), "delta"),
    ];
    let node = DiffNode::new(
        "text",
        DiffStatus::Different,
        DiffBody::Text(TextDiff {
            format: "text".into(),
            encoding_left: "utf-8".into(),
            encoding_right: "utf-8".into(),
            left_lines: 2,
            right_lines: 2,
            byte_difference_ratio: 0.0,
            added_lines: 2,
            removed_lines: 2,
            hunks: vec![TextHunk {
                old_start: 1,
                old_len: 2,
                new_start: 1,
                new_len: 2,
                lines,
            }],
            truncated: false,
        }),
    );
    let mut head = report(node.clone());
    let filter = ContentFilter::new(Some(2), None, &[], 0).unwrap();
    assert!(apply(&mut head, &filter));
    match &head.root.body {
        DiffBody::Text(diff) => {
            assert_eq!(diff.hunks[0].lines.len(), 2);
            assert!(diff.truncated);
        }
        other => panic!("{other:?}"),
    }

    let mut tail = report(node);
    let filter = ContentFilter::new(None, Some(2), &[], 0).unwrap();
    assert!(apply(&mut tail, &filter));
    match &tail.root.body {
        DiffBody::Text(diff) => {
            let texts: Vec<_> = diff.hunks[0]
                .lines
                .iter()
                .map(|line| line.text.as_str())
                .collect();
            assert_eq!(texts, vec!["gamma", "delta"]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn filter_text_keeps_matching_line() {
    let node = DiffNode::new(
        "text",
        DiffStatus::Different,
        DiffBody::Text(TextDiff {
            format: "text".into(),
            encoding_left: "utf-8".into(),
            encoding_right: "utf-8".into(),
            left_lines: 2,
            right_lines: 2,
            byte_difference_ratio: 0.0,
            added_lines: 2,
            removed_lines: 2,
            hunks: vec![TextHunk {
                old_start: 1,
                old_len: 2,
                new_start: 1,
                new_len: 2,
                lines: vec![
                    line(TextTag::Delete, Some(1), None, "alpha"),
                    line(TextTag::Insert, None, Some(1), "beta"),
                    line(TextTag::Delete, Some(2), None, "gamma"),
                    line(TextTag::Insert, None, Some(2), "delta"),
                ],
            }],
            truncated: false,
        }),
    );
    let mut report = report(node);
    let filter = ContentFilter::new(None, None, &["beta".to_string()], 0).unwrap();
    assert!(apply(&mut report, &filter));
    match &report.root.body {
        DiffBody::Text(diff) => {
            assert_eq!(diff.hunks.len(), 1);
            assert_eq!(diff.hunks[0].lines.len(), 1);
            assert_eq!(diff.hunks[0].lines[0].text, "beta");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn filter_can_drop_unrelated_nodes() {
    let node = DiffNode::new(
        "unrelated",
        DiffStatus::Equal,
        DiffBody::Equal(EqualInfo {
            kind: FileKind::Text,
            note: String::new(),
        }),
    );
    let mut report = report(node);
    let filter = ContentFilter::new(None, None, &["needle".to_string()], 0).unwrap();
    assert!(!apply(&mut report, &filter));
}
