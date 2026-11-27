use crate::model::*;
use crate::render::display_path;
use std::fmt::Write;

/// GNU-style unified output for text nodes. Non-text leaves get a short
/// machine-friendly note so the format remains usable in pipelines.
pub fn render(report: &DiffReport) -> String {
    let mut out = String::new();
    render_node(&report.root, report, &mut out);
    if let Some(decompiled) = &report.decompiled {
        let _ = writeln!(out, "--- {} (decompiled)", report.left.path);
        let _ = writeln!(out, "+++ {} (decompiled)", report.right.path);
        for hunk in &decompiled.text.hunks {
            let _ = writeln!(
                out,
                "@@ -{},{} +{},{} @@",
                hunk.old_start, hunk.old_len, hunk.new_start, hunk.new_len
            );
            for line in &hunk.lines {
                let prefix = match line.tag {
                    TextTag::Equal => ' ',
                    TextTag::Delete => '-',
                    TextTag::Insert => '+',
                };
                let _ = writeln!(out, "{prefix}{}", line.text);
            }
        }
    }
    if out.is_empty() {
        out.push_str("No differences\n");
    }
    out
}

fn render_node(node: &DiffNode, report: &DiffReport, out: &mut String) {
    match &node.body {
        DiffBody::Text(diff) => {
            if diff.added_lines == 0 && diff.removed_lines == 0 {
                return;
            }
            let (left_path, right_path) = paths(node, report);
            let _ = writeln!(out, "--- {left_path}");
            let _ = writeln!(out, "+++ {right_path}");
            for hunk in &diff.hunks {
                let _ = writeln!(
                    out,
                    "@@ -{},{} +{},{} @@",
                    hunk.old_start, hunk.old_len, hunk.new_start, hunk.new_len
                );
                for line in &hunk.lines {
                    let prefix = match line.tag {
                        TextTag::Equal => ' ',
                        TextTag::Delete => '-',
                        TextTag::Insert => '+',
                    };
                    let _ = writeln!(out, "{prefix}{}", line.text);
                }
            }
        }
        DiffBody::Directory(directory) => {
            for entry in &directory.entries {
                render_node(entry, report, out);
            }
        }
        DiffBody::Archive(archive) => {
            for entry in &archive.entries {
                render_node(entry, report, out);
            }
        }
        DiffBody::OneSided(info) => {
            let side = match info.side {
                PresenceSide::LeftOnly => "left",
                PresenceSide::RightOnly => "right",
            };
            let _ = writeln!(
                out,
                "Only in {side}: {} ({}, {} bytes)",
                display_path(&node.label),
                info.kind.label(),
                info.size
            );
        }
        DiffBody::Binary(_) => {
            let (left_path, right_path) = paths(node, report);
            let _ = writeln!(out, "Binary files {left_path} and {right_path} differ");
        }
        DiffBody::Image(_)
        | DiffBody::Structured(_)
        | DiffBody::Table(_)
        | DiffBody::Workbook(_)
        | DiffBody::Tokens(_)
        | DiffBody::Tree(_)
        | DiffBody::Notebook(_)
        | DiffBody::Audio(_) => {
            let (left_path, right_path) = paths(node, report);
            let _ = writeln!(
                out,
                "# {} diff between {left_path} and {right_path}; use --format ansi/json for details",
                body_name(&node.body)
            );
        }
        DiffBody::Unsupported(info) => {
            let _ = writeln!(out, "# {}: {}", display_path(&node.label), info.note);
        }
        DiffBody::Equal(_) => {}
    }
    if let Some(decompiled) = &node.decompiled {
        let _ = writeln!(out, "--- {} (decompiled)", node.label);
        let _ = writeln!(out, "+++ {} (decompiled)", node.label);
        for hunk in &decompiled.text.hunks {
            let _ = writeln!(
                out,
                "@@ -{},{} +{},{} @@",
                hunk.old_start, hunk.old_len, hunk.new_start, hunk.new_len
            );
            for line in &hunk.lines {
                let prefix = match line.tag {
                    TextTag::Equal => ' ',
                    TextTag::Delete => '-',
                    TextTag::Insert => '+',
                };
                let _ = writeln!(out, "{prefix}{}", line.text);
            }
        }
    }
}

fn paths(node: &DiffNode, report: &DiffReport) -> (String, String) {
    if node.label.is_empty() {
        (report.left.path.clone(), report.right.path.clone())
    } else {
        (format!("a/{}", node.label), format!("b/{}", node.label))
    }
}

fn body_name(body: &DiffBody) -> &'static str {
    match body {
        DiffBody::Text(_) => "text",
        DiffBody::Tokens(_) => "syntax token",
        DiffBody::Tree(_) => "syntax tree",
        DiffBody::Notebook(_) => "notebook",
        DiffBody::Audio(_) => "audio",
        DiffBody::Structured(_) => "structured",
        DiffBody::Table(_) => "table",
        DiffBody::Workbook(_) => "workbook",
        DiffBody::Image(_) => "image",
        DiffBody::Binary(_) => "binary",
        DiffBody::Directory(_) => "directory",
        DiffBody::Archive(_) => "archive",
        DiffBody::OneSided(_) => "one-sided",
        DiffBody::Equal(_) => "equal",
        DiffBody::Unsupported(_) => "unsupported",
    }
}
