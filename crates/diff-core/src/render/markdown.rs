use crate::model::*;
use crate::render::{display_path, display_title, human_size, summary_parts, truncate_chars};
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
    if report.completely_different {
        let _ = writeln!(out, "| Result | **COMPLETELY DIFFERENT (>90% changed)** |");
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
        DiffBody::Table(diff) => {
            let _ = writeln!(out, "{heading} {}\n", display_title(&node.label));
            let _ = writeln!(
                out,
                "Table diff: **{} -> {} rows**, {} changed row(s){}",
                diff.left_rows,
                diff.right_rows,
                diff.rows
                    .iter()
                    .filter(|row| row.kind == RowChangeKind::Changed)
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
            for row in &diff.rows {
                let (marker, verb) = match row.kind {
                    RowChangeKind::Added => ("+", "added"),
                    RowChangeKind::Removed => ("-", "removed"),
                    RowChangeKind::Changed => ("~", "changed"),
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
            for sheet in &diff.sheets {
                if sheet.cells.is_empty() {
                    continue;
                }
                let _ = writeln!(
                    out,
                    "\n**Sheet {}** ({}x{} -> {}x{})",
                    sheet.name,
                    sheet.left_rows,
                    sheet.left_columns,
                    sheet.right_rows,
                    sheet.right_columns
                );
                for cell in &sheet.cells {
                    let _ = writeln!(
                        out,
                        "- `{}`: `{}` -> `{}`",
                        cell.column,
                        cell.left.as_deref().unwrap_or("-"),
                        cell.right.as_deref().unwrap_or("-")
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
            let _ = writeln!(
                out,
                "- `=` **{}** — equal ({})",
                display_path(&node.label),
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
