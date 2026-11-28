use crate::model::*;
use crate::render::{
    display_node_label, display_path, display_title, human_size, summary_parts, truncate_chars,
};
use std::fmt::Write;

pub fn render(report: &DiffReport) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "# DiffEverything report\n");
    let _ = writeln!(out, "| Field | Value |");
    let _ = writeln!(out, "|---|---|");
    let _ = writeln!(
        out,
        "| Left | `{}` ({} , {}) |",
        report.left.path,
        report.left.kind.label(),
        human_size(report.left.size)
    );
    let _ = writeln!(
        out,
        "| Right | `{}` ({} , {}) |",
        report.right.path,
        report.right.kind.label(),
        human_size(report.right.size)
    );
    let _ = writeln!(out, "| Status | **{}** |", report.status.label());
    if !report.is_complete() {
        let _ = writeln!(
            out,
            "| Completeness | **TRUNCATED** (part of the input was skipped by limits) |"
        );
    }
    if report.completely_different {
        let _ = writeln!(out, "| Result | **COMPLETELY DIFFERENT (>99% changed)** |");
    }
    let _ = writeln!(out, "| Elapsed | {} ms |", report.elapsed_ms);
    let _ = writeln!(
        out,
        "\n**Summary:** {}\n",
        summary_parts(&report.summary).join(" | ")
    );

    render_node(&report.root, 2, &mut out);

    if let Some(decompiled) = &report.decompiled {
        let _ = writeln!(out, "\n## Decompiled code\n");
        let _ = writeln!(
            out,
            "`{}`: +{} / -{} lines\n",
            decompiled.tool, decompiled.text.added_lines, decompiled.text.removed_lines
        );
        let _ = writeln!(out, "```diff");
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
        let _ = writeln!(out, "```");
    }

    if !report.warnings.is_empty() {
        let _ = writeln!(out, "\n## Warnings\n");
        for warning in &report.warnings {
            let _ = writeln!(out, "- {}", truncate_chars(warning, 500));
        }
    }

    out
}

fn render_node(node: &DiffNode, level: usize, out: &mut String) {
    let heading = "#".repeat(level.min(6));
    match &node.body {
        DiffBody::Text(diff) => {
            let _ = writeln!(out, "{heading} {}\n", display_title(&node.label));
            let _ = writeln!(
                out,
                "{} diff: **+{} / -{}** lines{}",
                diff.format,
                diff.added_lines,
                diff.removed_lines,
                if diff.truncated { " (truncated)" } else { "" }
            );
            let _ = writeln!(out, "\n```diff");
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
                    let _ = writeln!(out, "{prefix}{}", render_text_line(line));
                }
            }
            let _ = writeln!(out, "```\n");
        }
        DiffBody::Tokens(diff) => {
            let _ = writeln!(out, "{heading} {}\n", display_title(&node.label));
            let _ = writeln!(
                out,
                "Syntax token diff ({}): **+{} / -{}** tokens{}",
                diff.language,
                diff.added_tokens,
                diff.removed_tokens,
                if diff.truncated { " (truncated)" } else { "" }
            );
            let _ = writeln!(out, "\n```diff");
            for chunk in &diff.chunks {
                let prefix = match chunk.tag {
                    TextTag::Equal => ' ',
                    TextTag::Delete => '-',
                    TextTag::Insert => '+',
                };
                let _ = writeln!(out, "{prefix}{}", chunk.text);
            }
            let _ = writeln!(out, "```\n");
        }
        DiffBody::Tree(diff) => {
            let _ = writeln!(out, "{heading} {}\n", display_title(&node.label));
            let _ = writeln!(
                out,
                "Syntax tree diff ({}): **+{} / -{} / ~{} / >{}** nodes{}",
                diff.language,
                diff.added_nodes,
                diff.removed_nodes,
                diff.modified_nodes,
                diff.moved_nodes,
                if diff.truncated { " (truncated)" } else { "" }
            );
            for chunk in &diff.chunks {
                let marker = match chunk.tag {
                    TreeTag::Added => "+",
                    TreeTag::Removed => "-",
                    TreeTag::Modified => "~",
                    TreeTag::Moved => ">",
                };
                let _ = writeln!(
                    out,
                    "- `{marker}` `{}`: {}",
                    chunk.path,
                    truncate_chars(&chunk.text, 400)
                );
            }
            let _ = writeln!(out);
        }
        DiffBody::Notebook(diff) => {
            let _ = writeln!(out, "{heading} {}\n", display_title(&node.label));
            let _ = writeln!(
                out,
                "Notebook: **{} -> {} cells**, +{} -{} ~{} moved {}{}",
                diff.left_cells,
                diff.right_cells,
                diff.cells_added,
                diff.cells_removed,
                diff.changed_cells.len(),
                diff.cells_moved,
                if diff.truncated { " (truncated)" } else { "" }
            );
            for cell in &diff.changed_cells {
                let _ = writeln!(
                    out,
                    "\n**Cell {} [{}]** — {}{}",
                    cell.index,
                    cell.cell_type,
                    if cell.execution_changed {
                        "execution changed; "
                    } else {
                        ""
                    },
                    if cell.outputs_changed {
                        "outputs changed"
                    } else {
                        ""
                    }
                );
                if let Some(source) = &cell.source {
                    let _ = writeln!(out, "\n```diff");
                    for hunk in &source.hunks {
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
                    let _ = writeln!(out, "```");
                }
            }
            let _ = writeln!(out);
        }
        DiffBody::Structured(diff) => {
            let _ = writeln!(out, "{heading} {}\n", display_title(&node.label));
            let _ = writeln!(
                out,
                "{} changes: **{}**{}",
                diff.format.to_uppercase(),
                diff.changes.len(),
                if diff.truncated { " (truncated)" } else { "" }
            );
            for change in &diff.changes {
                let (marker, verb) = match change.kind {
                    StructuredChangeKind::Added => ("+", "added"),
                    StructuredChangeKind::Removed => ("-", "removed"),
                    StructuredChangeKind::Changed => ("~", "changed"),
                };
                let value = match (&change.left, &change.right) {
                    (Some(left), Some(right)) => {
                        format!("{} -> {}", compact_json(left), compact_json(right))
                    }
                    (Some(left), None) => compact_json(left),
                    (None, Some(right)) => compact_json(right),
                    (None, None) => String::new(),
                };
                let _ = writeln!(
                    out,
                    "- `{marker}` **{}** (`{verb}`){}: `{}`",
                    change.path,
                    if change.breaking {
                        " **[breaking]**"
                    } else {
                        ""
                    },
                    truncate_chars(&value, 300)
                );
            }
            let _ = writeln!(out);
        }
        DiffBody::Document(diff) => {
            let _ = writeln!(out, "{heading} {}\n", display_title(&node.label));
            let _ = writeln!(
                out,
                "Document diff: **{}**, {} block(s), {} table(s), {} media change(s){}",
                diff.format,
                diff.blocks.len(),
                diff.tables.len(),
                diff.media.len(),
                if diff.truncated { " (truncated)" } else { "" }
            );
            for field in &diff.metadata {
                let _ = writeln!(
                    out,
                    "- metadata `{}`: `{}` -> `{}`",
                    field.path,
                    field.left.as_deref().unwrap_or("-"),
                    field.right.as_deref().unwrap_or("-")
                );
            }
            if let Some(visual) = &diff.visual {
                let _ = writeln!(
                    out,
                    "\n**PDF visual diff**: {} pages compared, +{} / -{}, {} changed\n",
                    visual.pages_compared,
                    visual.pages_added,
                    visual.pages_removed,
                    visual.changed_pages
                );
                let _ = writeln!(out, "- HTML: `{}`", visual.report_html);
                let _ = writeln!(out, "- JSON: `{}`", visual.report_json);
                for page in visual.pages.iter().filter(|page| page.differing_pixels > 0) {
                    let _ = writeln!(
                        out,
                        "- page {}: {} / {} pixels differ ({:.2}%)",
                        page.page,
                        page.differing_pixels,
                        page.total_pixels,
                        page.diff_ratio * 100.0
                    );
                }
            }
            for block in &diff.blocks {
                let marker = match block.change {
                    ChangeKind::Added => "+",
                    ChangeKind::Removed => "-",
                    ChangeKind::Changed | ChangeKind::Moved => "~",
                };
                let moved = block
                    .moved_from
                    .as_deref()
                    .map(|from| format!(" (moved from `{from}`)"))
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "- `{marker}` `{}` [{}]{moved}: {}",
                    block.path,
                    block.kind,
                    truncate_chars(
                        &match block.change {
                            ChangeKind::Added => block.right.clone().unwrap_or_default(),
                            ChangeKind::Removed => block.left.clone().unwrap_or_default(),
                            ChangeKind::Changed | ChangeKind::Moved => format!(
                                "{} -> {}",
                                block.left.as_deref().unwrap_or("-"),
                                block.right.as_deref().unwrap_or("-")
                            ),
                        },
                        300
                    )
                );
            }
            for table in &diff.tables {
                let _ = writeln!(
                    out,
                    "- table `{}`: {} changed row(s)",
                    table.path,
                    table.table.rows.len()
                );
            }
        }
        DiffBody::Presentation(diff) => {
            let _ = writeln!(out, "{heading} {}\n", display_title(&node.label));
            let _ = writeln!(
                out,
                "Presentation diff: **{}**, {} -> {} slide(s), +{} / -{} / moved {}{}",
                diff.format,
                diff.left_slides,
                diff.right_slides,
                diff.slides_added,
                diff.slides_removed,
                diff.slides_moved,
                if diff.truncated { " (truncated)" } else { "" }
            );
            for slide in &diff.slides {
                let marker = match slide.change {
                    ChangeKind::Added => "+",
                    ChangeKind::Removed => "-",
                    ChangeKind::Changed | ChangeKind::Moved => "~",
                };
                let position = slide
                    .right_index
                    .or(slide.left_index)
                    .map(|index| index.to_string())
                    .unwrap_or_else(|| "?".to_string());
                let _ = writeln!(
                    out,
                    "- `{marker}` slide {position} **{}**{}",
                    slide.title.as_deref().unwrap_or("(untitled)"),
                    if slide.change == ChangeKind::Moved {
                        " (moved)"
                    } else {
                        ""
                    }
                );
                for shape in &slide.shapes {
                    let marker = match shape.change {
                        ChangeKind::Added => "+",
                        ChangeKind::Removed => "-",
                        ChangeKind::Changed | ChangeKind::Moved => "~",
                    };
                    let _ = writeln!(out, "  - `{marker}` {}", shape.name);
                }
            }
        }
        DiffBody::Table(diff) => {
            let _ = writeln!(out, "{heading} {}\n", display_title(&node.label));
            let _ = writeln!(
                out,
                "Table diff: **{} -> {} rows**, {} changed row(s){}",
                diff.left_rows,
                diff.right_rows,
                diff.rows
                    .iter()
                    .filter(|row| matches!(row.kind, RowChangeKind::Changed | RowChangeKind::Moved))
                    .count(),
                if diff.truncated { " (truncated)" } else { "" }
            );
            if !diff.columns_added.is_empty() {
                let _ = writeln!(out, "- Columns added: {}", diff.columns_added.join(", "));
            }
            if !diff.columns_removed.is_empty() {
                let _ = writeln!(
                    out,
                    "- Columns removed: {}",
                    diff.columns_removed.join(", ")
                );
            }
            for column in &diff.columns_moved {
                let _ = writeln!(
                    out,
                    "- Column **{}** moved: {} -> {}",
                    column.name, column.left_index, column.right_index
                );
            }
            for row in &diff.rows {
                let (marker, verb) = match row.kind {
                    RowChangeKind::Added => ("+", "added"),
                    RowChangeKind::Removed => ("-", "removed"),
                    RowChangeKind::Changed => ("~", "changed"),
                    RowChangeKind::Moved => ("~", "moved"),
                };
                let cells: Vec<String> = row
                    .cells
                    .iter()
                    .map(|cell| {
                        format!(
                            "{}: {} -> {}",
                            cell.column,
                            cell.left.as_deref().unwrap_or("-"),
                            cell.right.as_deref().unwrap_or("-")
                        )
                    })
                    .collect();
                let _ = writeln!(
                    out,
                    "- `{marker}` **{}** (`{verb}`): {}",
                    row.key,
                    truncate_chars(&cells.join("; "), 500)
                );
            }
            let _ = writeln!(out);
        }
        DiffBody::Workbook(diff) => {
            let _ = writeln!(out, "{heading} {}\n", display_title(&node.label));
            let _ = writeln!(
                out,
                "Workbook diff: **{} sheet(s)**, {} changed cell(s){}",
                diff.sheets.len() + diff.sheets_added.len() + diff.sheets_removed.len(),
                diff.sheets
                    .iter()
                    .map(|sheet| sheet.cells.len())
                    .sum::<usize>(),
                if diff.truncated { " (truncated)" } else { "" }
            );
            for name in &diff.sheets_added {
                let _ = writeln!(out, "- `+` sheet **{name}** added");
            }
            for name in &diff.sheets_removed {
                let _ = writeln!(out, "- `-` sheet **{name}** removed");
            }
            for sheet in &diff.sheets_moved {
                let _ = writeln!(
                    out,
                    "- `~` sheet **{}** moved: {} -> {}",
                    sheet.name, sheet.left_index, sheet.right_index
                );
            }
            for rename in &diff.sheets_renamed {
                let _ = writeln!(
                    out,
                    "- `~` sheet **{}** renamed to **{}**",
                    rename.from, rename.to
                );
            }
            for name in &diff.defined_names {
                let _ = writeln!(
                    out,
                    "- `~` defined name **{}**: `{}` -> `{}`",
                    name.path,
                    name.left.as_deref().unwrap_or("-"),
                    name.right.as_deref().unwrap_or("-")
                );
            }
            for sheet in &diff.sheets {
                if sheet.cells.is_empty()
                    && sheet.rows_moved.is_empty()
                    && sheet.columns_moved.is_empty()
                    && sheet.left_hidden == sheet.right_hidden
                    && sheet.merges_added.is_empty()
                    && sheet.merges_removed.is_empty()
                    && sheet.hidden_dimensions.is_empty()
                    && sheet.drawings_added.is_empty()
                    && sheet.drawings_removed.is_empty()
                    && sheet.chart_changes.is_empty()
                    && sheet.conditional_formats_added.is_empty()
                    && sheet.conditional_formats_removed.is_empty()
                {
                    continue;
                }
                let hidden = if sheet.left_hidden != sheet.right_hidden {
                    if sheet.right_hidden {
                        " (hidden on right)"
                    } else {
                        " (hidden on left)"
                    }
                } else {
                    ""
                };
                let _ = writeln!(
                    out,
                    "\n**Sheet {}** ({}x{} -> {}x{}){hidden}",
                    sheet.name,
                    sheet.left_rows,
                    sheet.left_columns,
                    sheet.right_rows,
                    sheet.right_columns
                );
                for row in &sheet.rows_moved {
                    let _ = writeln!(
                        out,
                        "- `~` row {} moved to row {}",
                        row.left_row, row.right_row
                    );
                }
                for column in &sheet.columns_moved {
                    let _ = writeln!(
                        out,
                        "- `~` column {} moved to {}",
                        column.left_column, column.right_column
                    );
                }
                for merge in &sheet.merges_added {
                    let _ = writeln!(out, "- `+` merge `{merge}`");
                }
                for merge in &sheet.merges_removed {
                    let _ = writeln!(out, "- `-` merge `{merge}`");
                }
                for dimension in &sheet.hidden_dimensions {
                    let _ = writeln!(
                        out,
                        "- `~` {}: `{}` -> `{}`",
                        dimension.path,
                        dimension.left.as_deref().unwrap_or("-"),
                        dimension.right.as_deref().unwrap_or("-")
                    );
                }
                for drawing in &sheet.drawings_added {
                    let _ = writeln!(out, "- `+` drawing `{drawing}`");
                }
                for drawing in &sheet.drawings_removed {
                    let _ = writeln!(out, "- `-` drawing `{drawing}`");
                }
                for change in &sheet.chart_changes {
                    let _ = writeln!(
                        out,
                        "- `~` {}: `{}` -> `{}`",
                        change.path,
                        change.left.as_deref().unwrap_or("-"),
                        change.right.as_deref().unwrap_or("-")
                    );
                }
                for rule in &sheet.conditional_formats_added {
                    let _ = writeln!(out, "- `+` conditional format `{rule}`");
                }
                for rule in &sheet.conditional_formats_removed {
                    let _ = writeln!(out, "- `-` conditional format `{rule}`");
                }
                for cell in &sheet.cells {
                    let formulas = match (&cell.left_formula, &cell.right_formula) {
                        (Some(left), Some(right)) => {
                            format!(" [formula {left} -> {right}]")
                        }
                        (Some(left), None) => format!(" [formula {left}]"),
                        (None, Some(right)) => format!(" [formula {right}]"),
                        (None, None) => String::new(),
                    };
                    let _ = writeln!(
                        out,
                        "- `{}`: `{}` -> `{}`{}",
                        cell.column,
                        cell.left.as_deref().unwrap_or("-"),
                        cell.right.as_deref().unwrap_or("-"),
                        formulas
                    );
                }
            }
            let _ = writeln!(out);
        }
        DiffBody::Image(diff) => {
            let _ = writeln!(out, "{heading} {}\n", display_title(&node.label));
            let _ = writeln!(
                out,
                "- Dimensions: {}x{} -> {}x{}",
                diff.left_width, diff.left_height, diff.right_width, diff.right_height
            );
            let _ = writeln!(
                out,
                "- Differing pixels: **{} / {} ({:.2}%)**",
                diff.differing_pixels,
                diff.total_pixels,
                diff.diff_ratio * 100.0
            );
            let _ = writeln!(out, "- Max channel delta: {}", diff.max_channel_delta);
            if let Some(bbox) = diff.bounding_box {
                let _ = writeln!(
                    out,
                    "- Bounding box: ({}, {}) {}x{}",
                    bbox.x, bbox.y, bbox.width, bbox.height
                );
            }
            if !diff.regions.is_empty() {
                let sample: Vec<String> = diff
                    .regions
                    .iter()
                    .take(3)
                    .map(|region| {
                        format!(
                            "({}, {}) {}x{}",
                            region.x, region.y, region.width, region.height
                        )
                    })
                    .collect();
                let _ = writeln!(
                    out,
                    "- Changed regions: **{}**{}{}",
                    diff.regions.len(),
                    if sample.is_empty() { "" } else { ": " },
                    sample.join("; ")
                );
            }
            let _ = writeln!(out);
        }
        DiffBody::Audio(diff) => {
            let _ = writeln!(out, "{heading} {}\n", display_title(&node.label));
            let _ = writeln!(
                out,
                "- Format: {} → {}",
                diff.left_format, diff.right_format
            );
            let _ = writeln!(
                out,
                "- Duration: {} ms → {} ms",
                diff.left_duration_ms, diff.right_duration_ms
            );
            let _ = writeln!(
                out,
                "- Bitrate: {:?} → {:?}",
                diff.left_bitrate, diff.right_bitrate
            );
            let _ = writeln!(
                out,
                "- Sample rate: {:?} → {:?}",
                diff.left_sample_rate, diff.right_sample_rate
            );
            let _ = writeln!(
                out,
                "- Channels: {:?} → {:?}",
                diff.left_channels, diff.right_channels
            );
            if diff.content_differs {
                let _ = writeln!(out, "- Audio content differs");
            }
            if let Some(waveform) = &diff.waveform {
                let _ = writeln!(
                    out,
                    "- Waveform: {} Hz, {} ch, {} ms windows, max delta {:.4}, {} changed{}",
                    waveform.sample_rate,
                    waveform.channels,
                    waveform.window_ms,
                    waveform.max_delta,
                    waveform.changed_windows,
                    if waveform.truncated {
                        " (truncated)"
                    } else {
                        ""
                    }
                );
                if !waveform.left.is_empty() {
                    let _ = writeln!(out, "\n| Window | Left RMS | Right RMS |");
                    let _ = writeln!(out, "|---:|---:|---:|");
                    for (index, (left, right)) in waveform
                        .left
                        .iter()
                        .zip(&waveform.right)
                        .take(16)
                        .enumerate()
                    {
                        let _ = writeln!(out, "| {index} | {left:.4} | {right:.4} |");
                    }
                }
            }
            for tag in &diff.tags {
                let _ = writeln!(
                    out,
                    "- `~{}`: `{}` → `{}`",
                    tag.key,
                    tag.left.as_deref().unwrap_or("-"),
                    tag.right.as_deref().unwrap_or("-")
                );
            }
            let _ = writeln!(out);
        }
        DiffBody::Binary(diff) => {
            let _ = writeln!(out, "{heading} {}\n", display_title(&node.label));
            let _ = writeln!(out, "- Size: {} -> {} bytes", diff.left_len, diff.right_len);
            let _ = writeln!(out, "- Differing bytes: **{}**", diff.differing_bytes);
            if let Some(first) = diff.first_difference {
                let _ = writeln!(out, "- First difference: `0x{first:08x}`");
            }
            if diff.truncated {
                let _ = writeln!(out, "- Detail rows were truncated");
            }
            if !diff.rows.is_empty() {
                let _ = writeln!(out, "\n```text");
                for row in &diff.rows {
                    let _ = writeln!(
                        out,
                        "0x{:08x}  left  {}  |{}|",
                        row.offset, row.left_hex, row.left_ascii
                    );
                    let _ = writeln!(
                        out,
                        "            right {}  |{}|",
                        row.right_hex, row.right_ascii
                    );
                }
                let _ = writeln!(out, "```");
            }
            let _ = writeln!(out);
        }
        DiffBody::Directory(directory) => {
            let _ = writeln!(out, "{heading} {}\n", display_title(&node.label));
            let _ = writeln!(
                out,
                "Directory: **{}** entries{}",
                directory.total_entries,
                if directory.truncated {
                    " (truncated)"
                } else {
                    ""
                }
            );
            render_entries(&directory.entries, level + 1, out);
            let _ = writeln!(out);
        }
        DiffBody::Archive(archive) => {
            let _ = writeln!(out, "{heading} {}\n", display_title(&node.label));
            let _ = writeln!(
                out,
                "{} archive: **{}** entries{}",
                archive.format,
                archive.total_entries,
                if archive.truncated {
                    " (truncated)"
                } else {
                    ""
                }
            );
            render_entries(&archive.entries, level + 1, out);
            let _ = writeln!(out);
        }
        DiffBody::OneSided(info) => {
            let (marker, side) = match info.side {
                PresenceSide::LeftOnly => ('-', "only in left"),
                PresenceSide::RightOnly => ('+', "only in right"),
            };
            let _ = writeln!(
                out,
                "- `{marker}` **{}** — {} ({}, {})",
                display_path(&node.label),
                side,
                info.kind.label(),
                human_size(info.size)
            );
        }
        DiffBody::Equal(info) => {
            let marker = if node.renamed_from.is_some() {
                "~"
            } else {
                "="
            };
            let note = if node.renamed_from.is_some() {
                "renamed, content identical"
            } else {
                "equal"
            };
            let _ = writeln!(
                out,
                "- `{marker}` **{}** — {note} ({})",
                display_node_label(node),
                info.kind.label()
            );
        }
        DiffBody::Unsupported(info) => {
            let _ = writeln!(
                out,
                "{heading} {}\n\n> Unsupported: {}\n",
                display_path(&node.label),
                info.note
            );
        }
    }
    if let Some(decompiled) = &node.decompiled {
        let _ = writeln!(out, "{} Decompiled code\n", "#".repeat((level + 1).min(6)));
        let _ = writeln!(
            out,
            "`{}`: +{} / -{} lines\n",
            decompiled.tool, decompiled.text.added_lines, decompiled.text.removed_lines
        );
        let _ = writeln!(out, "```diff");
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
        let _ = writeln!(out, "```");
    }
}

fn render_entries(entries: &[DiffNode], level: usize, out: &mut String) {
    for entry in entries {
        match &entry.body {
            DiffBody::Equal(_) | DiffBody::OneSided(_) => {
                render_node(entry, level, out);
            }
            _ => {
                render_node(entry, level, out);
            }
        }
    }
}

fn compact_json(value: &serde_json::Value) -> String {
    let text = serde_json::to_string(value).unwrap_or_else(|_| "<invalid>".to_string());
    truncate_chars(&text, 200)
}

fn render_text_line(line: &TextLine) -> String {
    if line.spans.is_empty() {
        return line.text.clone();
    }
    let mut out = String::new();
    for span in &line.spans {
        if span.changed {
            let _ = write!(out, "**{}**", span.text);
        } else {
            out.push_str(&span.text);
        }
    }
    out
}
