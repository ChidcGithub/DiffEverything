use crate::model::{
    AudioDiff, BinaryDiff, DiffBody, DiffNode, DiffReport, ImageDiff, NotebookDiff, StructuredDiff,
    TableDiff, TextDiff, TextHunk, TextLine, TextTag, TokenDiff, TreeDiff, WorkbookDiff,
};
use globset::{GlobBuilder, GlobMatcher};
use std::collections::BTreeMap;

/// Content-level selection applied after comparison and before rendering.
///
/// `head`/`last` limit detail units per file/entry; containers limit their
/// entries. `patterns` select matching paths/labels or detail content.
#[derive(Debug, Clone)]
pub struct ContentFilter {
    pub head: Option<usize>,
    pub last: Option<usize>,
    pub patterns: Vec<Pattern>,
    pub context: usize,
}

#[derive(Debug, Clone)]
pub enum Pattern {
    Text(String),
    Glob(GlobMatcher),
}

impl Pattern {
    pub fn new(pattern: &str) -> Result<Self, String> {
        if pattern
            .chars()
            .any(|ch| matches!(ch, '*' | '?' | '[' | ']' | '{' | '}'))
        {
            let glob = GlobBuilder::new(pattern)
                .case_insensitive(true)
                .literal_separator(false)
                .build()
                .map_err(|error| format!("invalid filter glob {pattern:?}: {error}"))?;
            Ok(Self::Glob(glob.compile_matcher()))
        } else {
            Ok(Self::Text(pattern.to_lowercase()))
        }
    }

    fn matches(&self, text: &str) -> bool {
        match self {
            Self::Text(needle) => text.to_lowercase().contains(needle),
            Self::Glob(matcher) => matcher.is_match(text),
        }
    }
}

impl ContentFilter {
    pub fn new(
        head: Option<usize>,
        last: Option<usize>,
        patterns: &[String],
        context: usize,
    ) -> Result<Self, String> {
        if head == Some(0) || last == Some(0) {
            return Err("head/last must be at least 1".to_string());
        }
        let mut compiled = Vec::with_capacity(patterns.len());
        for pattern in patterns {
            if pattern.is_empty() {
                return Err("filter pattern must not be empty".to_string());
            }
            compiled.push(Pattern::new(pattern)?);
        }
        Ok(Self {
            head,
            last,
            patterns: compiled,
            context,
        })
    }

    pub fn is_active(&self) -> bool {
        self.head.is_some() || self.last.is_some() || !self.patterns.is_empty()
    }

    fn has_patterns(&self) -> bool {
        !self.patterns.is_empty()
    }

    fn matches(&self, text: &str) -> bool {
        self.patterns.iter().any(|pattern| pattern.matches(text))
    }

    fn window(&self, length: usize) -> (usize, usize, bool) {
        if let Some(limit) = self.head {
            let end = limit.min(length);
            (0, end, end < length)
        } else if let Some(limit) = self.last {
            let start = length.saturating_sub(limit);
            (start, length, start > 0)
        } else {
            (0, length, false)
        }
    }
}

pub fn apply(report: &mut DiffReport, filter: &ContentFilter) -> bool {
    if !filter.is_active() {
        return true;
    }
    let root_match = filter.matches(&report.root.label);
    filter_node(&mut report.root, filter, root_match)
}

fn filter_node(node: &mut DiffNode, filter: &ContentFilter, parent_keep_all: bool) -> bool {
    let label = node.label.clone();
    let label_match = filter.matches(&label);
    let keep_all = parent_keep_all || label_match;
    match &mut node.body {
        DiffBody::Text(diff) => filter_text(diff, filter, keep_all),
        DiffBody::Tokens(diff) => filter_tokens(diff, filter, keep_all),
        DiffBody::Tree(diff) => filter_tree(diff, filter, keep_all),
        DiffBody::Notebook(diff) => filter_notebook(diff, filter, keep_all),
        DiffBody::Structured(diff) => filter_structured(diff, filter, keep_all),
        DiffBody::Table(diff) => filter_table(diff, filter, keep_all),
        DiffBody::Workbook(diff) => filter_workbook(diff, filter, keep_all),
        DiffBody::Image(diff) => filter_image(diff, filter, keep_all),
        DiffBody::Audio(diff) => filter_audio(diff, filter, keep_all),
        DiffBody::Binary(diff) => filter_binary(diff, filter, keep_all),
        DiffBody::Directory(diff) => filter_entries(
            &mut diff.entries,
            &mut diff.total_entries,
            &mut diff.truncated,
            filter,
            keep_all,
        ),
        DiffBody::Archive(diff) => filter_entries(
            &mut diff.entries,
            &mut diff.total_entries,
            &mut diff.truncated,
            filter,
            keep_all,
        ),
        DiffBody::OneSided(_) | DiffBody::Equal(_) | DiffBody::Unsupported(_) => {
            keep_all || !filter.has_patterns()
        }
    }
}

fn filter_entries(
    entries: &mut Vec<DiffNode>,
    total_entries: &mut usize,
    truncated: &mut bool,
    filter: &ContentFilter,
    keep_all: bool,
) -> bool {
    let mut kept = Vec::new();
    for mut entry in std::mem::take(entries) {
        if filter_node(&mut entry, filter, keep_all) {
            kept.push(entry);
        }
    }
    let (start, end, cut) = filter.window(kept.len());
    if cut {
        *truncated = true;
    }
    let selected: Vec<DiffNode> = kept.drain(start..end).collect();
    *total_entries = selected.len();
    *entries = selected;
    !entries.is_empty() || keep_all
}

fn filter_text(diff: &mut TextDiff, filter: &ContentFilter, keep_all: bool) -> bool {
    let content_filter = filter.has_patterns() && !keep_all;
    let hunks = std::mem::take(&mut diff.hunks);
    let mut flat: Vec<(usize, TextLine)> = Vec::new();
    for (hunk_index, hunk) in hunks.iter().enumerate() {
        if !content_filter {
            for line in &hunk.lines {
                flat.push((hunk_index, line.clone()));
            }
            continue;
        }
        let mut keep = vec![false; hunk.lines.len()];
        let mut matched = false;
        for (line_index, line) in hunk.lines.iter().enumerate() {
            if filter.matches(&line.text) {
                matched = true;
                let start = line_index.saturating_sub(filter.context);
                let end = (line_index + filter.context + 1).min(hunk.lines.len());
                for flag in &mut keep[start..end] {
                    *flag = true;
                }
            }
        }
        if matched {
            for (line_index, line) in hunk.lines.iter().enumerate() {
                if keep[line_index] {
                    flat.push((hunk_index, line.clone()));
                }
            }
        }
    }

    let (start, end, cut) = filter.window(flat.len());
    if cut {
        diff.truncated = true;
    }
    let selected = flat.drain(start..end);
    let mut grouped: BTreeMap<usize, Vec<TextLine>> = BTreeMap::new();
    for (hunk_index, line) in selected {
        grouped.entry(hunk_index).or_default().push(line);
    }

    let mut output = Vec::new();
    for (hunk_index, lines) in grouped {
        let mut hunk = hunks[hunk_index].clone();
        hunk.lines = lines;
        rebuild_hunk_ranges(&mut hunk);
        output.push(hunk);
    }
    diff.hunks = output;
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
    !diff.hunks.is_empty() || !content_filter
}

fn rebuild_hunk_ranges(hunk: &mut TextHunk) {
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

fn filter_tokens(diff: &mut TokenDiff, filter: &ContentFilter, keep_all: bool) -> bool {
    let matched = select_items(
        &mut diff.chunks,
        filter,
        keep_all,
        &mut diff.truncated,
        |chunk| format!("{:?} {} {}", chunk.tag, chunk.text, chunk.text),
    );
    matched || keep_all || !filter.has_patterns()
}

fn filter_tree(diff: &mut TreeDiff, filter: &ContentFilter, keep_all: bool) -> bool {
    let matched = select_items(
        &mut diff.chunks,
        filter,
        keep_all,
        &mut diff.truncated,
        |chunk| format!("{:?} {} {}", chunk.tag, chunk.path, chunk.text),
    );
    matched || keep_all || !filter.has_patterns()
}

fn filter_notebook(diff: &mut NotebookDiff, filter: &ContentFilter, keep_all: bool) -> bool {
    let matched = select_items(
        &mut diff.changed_cells,
        filter,
        keep_all,
        &mut diff.truncated,
        |cell| {
            let source = cell
                .source
                .as_ref()
                .map(text_lines_text)
                .unwrap_or_default();
            format!("{} {} {}", cell.index, cell.cell_type, source)
        },
    );
    matched || keep_all || !filter.has_patterns()
}

fn filter_structured(diff: &mut StructuredDiff, filter: &ContentFilter, keep_all: bool) -> bool {
    select_items(
        &mut diff.changes,
        filter,
        keep_all,
        &mut diff.truncated,
        |change| {
            format!(
                "{} {:?} {:?} {:?}",
                change.path, change.kind, change.left, change.right
            )
        },
    ) || keep_all
        || !filter.has_patterns()
}

fn filter_table(diff: &mut TableDiff, filter: &ContentFilter, keep_all: bool) -> bool {
    let matched = select_items(
        &mut diff.rows,
        filter,
        keep_all,
        &mut diff.truncated,
        |row| {
            let cells = row
                .cells
                .iter()
                .map(|cell| format!("{}={:?}/{:?}", cell.column, cell.left, cell.right))
                .collect::<Vec<_>>()
                .join(" ");
            format!("{} {:?} {}", row.key, row.kind, cells)
        },
    );
    matched || keep_all || !filter.has_patterns()
}

fn filter_workbook(diff: &mut WorkbookDiff, filter: &ContentFilter, keep_all: bool) -> bool {
    diff.sheets_added
        .retain(|name| keep_all || filter.matches(name));
    diff.sheets_removed
        .retain(|name| keep_all || filter.matches(name));
    let mut any_cells = false;
    for sheet in &mut diff.sheets {
        let sheet_name = sheet.name.clone();
        let sheet_match = keep_all || filter.matches(&sheet_name);
        if !sheet_match {
            sheet.cells.retain(|cell| {
                filter.matches(&format!(
                    "{} {} {:?} {:?}",
                    sheet_name, cell.column, cell.left, cell.right
                ))
            });
        }
        let matched = select_items(
            &mut sheet.cells,
            filter,
            sheet_match,
            &mut sheet.truncated,
            |cell| {
                format!(
                    "{} {} {:?} {:?}",
                    sheet_name, cell.column, cell.left, cell.right
                )
            },
        );
        any_cells |= matched;
    }
    diff.sheets.retain(|sheet| !sheet.cells.is_empty());
    any_cells
        || !diff.sheets_added.is_empty()
        || !diff.sheets_removed.is_empty()
        || keep_all
        || !filter.has_patterns()
}

fn filter_image(diff: &mut ImageDiff, filter: &ContentFilter, keep_all: bool) -> bool {
    let matched = select_items(
        &mut diff.regions,
        filter,
        keep_all,
        &mut diff.regions_truncated,
        |region| {
            format!(
                "{}x{}+{}+{}",
                region.width, region.height, region.x, region.y
            )
        },
    );
    matched || keep_all || !filter.has_patterns()
}

fn filter_audio(diff: &mut AudioDiff, filter: &ContentFilter, keep_all: bool) -> bool {
    let mut truncated = false;
    select_items(&mut diff.tags, filter, keep_all, &mut truncated, |tag| {
        format!("{} {:?} {:?}", tag.key, tag.left, tag.right)
    }) || keep_all
        || !filter.has_patterns()
}

fn filter_binary(diff: &mut BinaryDiff, filter: &ContentFilter, keep_all: bool) -> bool {
    let matched = select_items(
        &mut diff.rows,
        filter,
        keep_all,
        &mut diff.truncated,
        |row| {
            format!(
                "{:x} {} {} {} {}",
                row.offset, row.left_hex, row.right_hex, row.left_ascii, row.right_ascii
            )
        },
    );
    matched || keep_all || !filter.has_patterns()
}

fn text_lines_text(diff: &TextDiff) -> String {
    diff.hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn select_items<T, F>(
    items: &mut Vec<T>,
    filter: &ContentFilter,
    keep_all: bool,
    truncated: &mut bool,
    text: F,
) -> bool
where
    F: Fn(&T) -> String,
{
    if items.is_empty() {
        return !filter.has_patterns() || keep_all;
    }
    let content_filter = filter.has_patterns() && !keep_all;
    let matching: Vec<usize> = if content_filter {
        items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                if filter.matches(&text(item)) {
                    Some(index)
                } else {
                    None
                }
            })
            .collect()
    } else {
        (0..items.len()).collect()
    };
    if content_filter && matching.is_empty() {
        items.clear();
        return false;
    }
    let (start, end, cut) = filter.window(matching.len());
    if cut {
        *truncated = true;
    }
    let selected: Vec<usize> = matching[start..end].to_vec();
    let mut selected = selected.into_iter().peekable();
    let original = std::mem::take(items);
    for (index, item) in original.into_iter().enumerate() {
        if selected.peek().is_some_and(|next| *next == index) {
            selected.next();
            items.push(item);
        }
    }
    true
}

#[cfg(test)]
#[path = "../tests/unit/filter.rs"]
mod tests;
