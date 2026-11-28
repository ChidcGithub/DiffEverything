use crate::model::{DiffBody, DiffNode, DiffReport, TextDiff, TextHunk, TextLine, TextTag};
use std::collections::BTreeMap;

/// Compress a report for terminal/brief consumption.
///
/// Concise mode removes unchanged context, drops equal container entries and
/// caps detail units. Full mode never calls this transform.
pub fn apply(report: &mut DiffReport, limit: usize) {
    apply_node(&mut report.root, limit);
}

fn apply_node(node: &mut DiffNode, limit: usize) {
    match &mut node.body {
        DiffBody::Text(diff) => concise_text(diff, limit),
        DiffBody::Tokens(diff) => {
            diff.chunks.retain(|chunk| chunk.tag != TextTag::Equal);
            cap(&mut diff.chunks, limit, &mut diff.truncated);
        }
        DiffBody::Tree(diff) => cap(&mut diff.chunks, limit, &mut diff.truncated),
        DiffBody::Notebook(diff) => {
            cap(&mut diff.changed_cells, limit, &mut diff.truncated);
        }
        DiffBody::Structured(diff) => cap(&mut diff.changes, limit, &mut diff.truncated),
        DiffBody::Document(diff) => {
            cap(&mut diff.metadata, limit, &mut diff.truncated);
            cap(&mut diff.blocks, limit, &mut diff.truncated);
            cap(&mut diff.tables, limit, &mut diff.truncated);
            cap(&mut diff.media, limit, &mut diff.truncated);
        }
        DiffBody::Presentation(diff) => {
            cap(&mut diff.slides, limit, &mut diff.truncated);
            for slide in &mut diff.slides {
                cap(&mut slide.shapes, limit, &mut diff.truncated);
            }
        }
        DiffBody::Table(diff) => cap(&mut diff.rows, limit, &mut diff.truncated),
        DiffBody::Workbook(diff) => {
            cap(&mut diff.sheets_added, limit, &mut diff.truncated);
            cap(&mut diff.sheets_removed, limit, &mut diff.truncated);
            cap(&mut diff.sheets_moved, limit, &mut diff.truncated);
            cap(&mut diff.sheets_renamed, limit, &mut diff.truncated);
            cap(&mut diff.defined_names, limit, &mut diff.truncated);
            cap(&mut diff.sheets, limit, &mut diff.truncated);
            for sheet in &mut diff.sheets {
                cap(&mut sheet.cells, limit, &mut sheet.truncated);
                cap(&mut sheet.merges_added, limit, &mut sheet.truncated);
                cap(&mut sheet.merges_removed, limit, &mut sheet.truncated);
                cap(&mut sheet.hidden_dimensions, limit, &mut sheet.truncated);
                cap(&mut sheet.drawings_added, limit, &mut sheet.truncated);
                cap(&mut sheet.drawings_removed, limit, &mut sheet.truncated);
                cap(&mut sheet.chart_changes, limit, &mut sheet.truncated);
                cap(
                    &mut sheet.conditional_formats_added,
                    limit,
                    &mut sheet.truncated,
                );
                cap(
                    &mut sheet.conditional_formats_removed,
                    limit,
                    &mut sheet.truncated,
                );
            }
        }
        DiffBody::Image(diff) => {
            cap(&mut diff.regions, limit, &mut diff.regions_truncated);
        }
        DiffBody::Audio(diff) => {
            if diff.tags.len() > limit {
                diff.tags.truncate(limit);
            }
            if let Some(waveform) = &mut diff.waveform {
                let windows = waveform.left.len().min(waveform.right.len());
                let mut left = Vec::new();
                let mut right = Vec::new();
                for index in 0..windows {
                    if (waveform.left[index] - waveform.right[index]).abs() > 0.01 {
                        left.push(waveform.left[index]);
                        right.push(waveform.right[index]);
                        if left.len() >= limit {
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
        DiffBody::Binary(diff) => cap(&mut diff.rows, limit, &mut diff.truncated),
        DiffBody::Directory(diff) => concise_entries(
            &mut diff.entries,
            &mut diff.total_entries,
            &mut diff.truncated,
            limit,
        ),
        DiffBody::Archive(diff) => concise_entries(
            &mut diff.entries,
            &mut diff.total_entries,
            &mut diff.truncated,
            limit,
        ),
        DiffBody::OneSided(_) | DiffBody::Equal(_) | DiffBody::Unsupported(_) => {}
    }
}

fn concise_text(diff: &mut TextDiff, limit: usize) {
    let hunks = std::mem::take(&mut diff.hunks);
    let mut flat: Vec<(usize, TextLine)> = Vec::new();
    for (hunk_index, hunk) in hunks.iter().enumerate() {
        for line in &hunk.lines {
            if line.tag != TextTag::Equal {
                flat.push((hunk_index, line.clone()));
            }
        }
    }
    let cut = flat.len() > limit;
    if cut {
        flat.truncate(limit);
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

fn concise_entries(
    entries: &mut Vec<DiffNode>,
    total_entries: &mut usize,
    truncated: &mut bool,
    limit: usize,
) {
    let mut kept = Vec::new();
    for mut entry in std::mem::take(entries) {
        apply_node(&mut entry, limit);
        if entry.status != crate::model::DiffStatus::Equal {
            kept.push(entry);
        }
    }
    let cut = kept.len() > limit;
    if cut {
        kept.truncate(limit);
        *truncated = true;
    }
    *total_entries = kept.len();
    *entries = kept;
}

fn cap<T>(items: &mut Vec<T>, max: usize, truncated: &mut bool) {
    if items.len() > max {
        items.truncate(max);
        *truncated = true;
    }
}

#[cfg(test)]
#[path = "../tests/unit/concise.rs"]
mod tests;
