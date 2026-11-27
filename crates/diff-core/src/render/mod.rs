pub mod ansi;
pub mod html;
pub mod json;
pub mod markdown;
pub mod unified;

use crate::model::{DiffBody, DiffNode, Summary};
use std::borrow::Cow;

#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub color: bool,
    pub summary_only: bool,
    /// Use the decorated terminal layout (headers, rules and line-number
    /// gutters). Piped/redirected output uses the compact plain layout.
    pub rich: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            color: true,
            summary_only: false,
            rich: true,
        }
    }
}

pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Replace C0/C1 control characters (except tab) with `?` before writing
/// untrusted paths or file content to a terminal. ANSI escape sequences are
/// control data, not content, and must never reach the terminal raw.
pub fn sanitize_controls(text: &str) -> Cow<'_, str> {
    if !text
        .chars()
        .any(|character| character.is_control() && character != '\t')
    {
        return Cow::Borrowed(text);
    }
    Cow::Owned(
        text.chars()
            .map(|character| {
                if character.is_control() && character != '\t' {
                    '?'
                } else {
                    character
                }
            })
            .collect(),
    )
}

pub fn truncate_chars(text: &str, max_chars: usize) -> String {
    let text = sanitize_controls(text);
    if text.chars().count() <= max_chars {
        return text.into_owned();
    }
    let mut out: String = text.chars().take(max_chars).collect();
    out.push_str("...");
    out
}

pub fn summary_parts(summary: &Summary) -> Vec<String> {
    let mut parts = Vec::new();
    parts.push(format!("{} files/entries compared", summary.files_compared));
    if summary.files_changed > 0 {
        parts.push(format!("{} changed", summary.files_changed));
    }
    if summary.files_added > 0 {
        parts.push(format!("{} added", summary.files_added));
    }
    if summary.files_removed > 0 {
        parts.push(format!("{} removed", summary.files_removed));
    }
    if summary.files_equal > 0 {
        parts.push(format!("{} equal", summary.files_equal));
    }
    if summary.lines_added > 0 || summary.lines_removed > 0 {
        parts.push(format!(
            "+{} -{} lines",
            summary.lines_added, summary.lines_removed
        ));
    }
    if summary.bytes_differing > 0 {
        parts.push(format!("{} bytes differ", summary.bytes_differing));
    }
    if summary.structured_changes > 0 {
        parts.push(format!("{} structured changes", summary.structured_changes));
    }
    if summary.table_rows_added > 0
        || summary.table_rows_removed > 0
        || summary.table_rows_changed > 0
    {
        parts.push(format!(
            "table rows +{} -{} ~{}",
            summary.table_rows_added, summary.table_rows_removed, summary.table_rows_changed
        ));
    }
    if summary.table_cells_changed > 0 {
        parts.push(format!(
            "{} table cells changed",
            summary.table_cells_changed
        ));
    }
    if summary.workbook_cells_changed > 0 {
        parts.push(format!(
            "{} workbook cells changed",
            summary.workbook_cells_changed
        ));
    }
    if summary.tokens_added > 0 || summary.tokens_removed > 0 {
        parts.push(format!(
            "+{} -{} syntax tokens",
            summary.tokens_added, summary.tokens_removed
        ));
    }
    if summary.tree_added > 0
        || summary.tree_removed > 0
        || summary.tree_modified > 0
        || summary.tree_moved > 0
    {
        parts.push(format!(
            "syntax tree +{} -{} ~{} >{}",
            summary.tree_added, summary.tree_removed, summary.tree_modified, summary.tree_moved
        ));
    }
    if summary.notebook_cells_added > 0
        || summary.notebook_cells_removed > 0
        || summary.notebook_cells_changed > 0
    {
        parts.push(format!(
            "notebook cells +{} -{} ~{} moved {}",
            summary.notebook_cells_added,
            summary.notebook_cells_removed,
            summary.notebook_cells_changed,
            summary.notebook_cells_moved
        ));
    }
    if summary.audio_tag_changes > 0 || summary.audio_property_changes > 0 {
        parts.push(format!(
            "audio tags {} properties {}",
            summary.audio_tag_changes, summary.audio_property_changes
        ));
    }
    parts
}

/// Changed leaf paths, with a diff-style marker: `+` added, `-` removed,
/// `~` modified, `!` unsupported.
pub fn changed_entries(root: &DiffNode) -> Vec<(char, String)> {
    let mut out = Vec::new();
    collect_changed(root, &mut out);
    out
}

fn collect_changed(node: &DiffNode, out: &mut Vec<(char, String)>) {
    match &node.body {
        DiffBody::Directory(directory) => {
            for entry in &directory.entries {
                collect_changed(entry, out);
            }
        }
        DiffBody::Archive(archive) => {
            for entry in &archive.entries {
                collect_changed(entry, out);
            }
        }
        DiffBody::OneSided(info) => {
            let marker = if info.side == crate::model::PresenceSide::LeftOnly {
                '-'
            } else {
                '+'
            };
            out.push((marker, display_path(&node.label)));
        }
        DiffBody::Unsupported(_) => {
            out.push(('!', display_path(&node.label)));
        }
        _ if node.status.is_different() => {
            out.push(('~', display_path(&node.label)));
        }
        _ => {}
    }
}

pub fn display_path(label: &str) -> String {
    if label.is_empty() {
        "<root>".to_string()
    } else {
        sanitize_controls(label).into_owned()
    }
}

pub fn display_title(label: &str) -> String {
    if label.is_empty() {
        "Diff".to_string()
    } else {
        label.to_string()
    }
}

pub fn marker_for(status: crate::model::DiffStatus) -> char {
    match status {
        crate::model::DiffStatus::Equal => '=',
        crate::model::DiffStatus::Different => '~',
        crate::model::DiffStatus::Unsupported | crate::model::DiffStatus::Error => '!',
    }
}
