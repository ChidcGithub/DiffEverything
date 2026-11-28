use crate::model::*;
use crate::render::{
    RenderOptions, display_path, human_size, marker_for, sanitize_controls, summary_parts,
    truncate_chars,
};
use std::fmt::Write;

const RESET: &str = "\x1b[0m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const CYAN: &str = "\x1b[36m";
const DIM: &str = "\x1b[2m";
const BOLD_RED: &str = "\x1b[1;31m";
const BOLD_GREEN: &str = "\x1b[1;32m";
const BOLD_YELLOW: &str = "\x1b[1;33m";
const BOLD_CYAN: &str = "\x1b[1;36m";
const BOLD_WHITE: &str = "\x1b[1;37m";
const BADGE_RED: &str = "\x1b[1;97;41m";
const BADGE_GREEN: &str = "\x1b[1;97;42m";
const BADGE_YELLOW: &str = "\x1b[1;30;43m";
const RULE_WIDTH: usize = 72;

pub fn render(report: &DiffReport, options: &RenderOptions) -> String {
    let mut out = String::new();
    if options.rich {
        render_rich_header(report, options, &mut out);
    } else {
        render_plain_header(report, options, &mut out);
    }

    if options.summary_only {
        let entries = crate::render::changed_entries(&report.root);
        if !entries.is_empty() {
            let _ = writeln!(out);
            if options.rich {
                section_rule("Changed paths", options, &mut out);
                for (marker, path) in entries {
                    let marker = paint(
                        &marker.to_string(),
                        marker_color_char(marker),
                        options.color,
                    );
                    if path == "<root>" {
                        let _ = writeln!(
                            out,
                            "  {marker} {} -> {}",
                            sanitize_controls(&report.left.path),
                            sanitize_controls(&report.right.path)
                        );
                    } else {
                        let _ = writeln!(out, "  {marker} {path}");
                    }
                }
            } else {
                let _ = writeln!(out, "changed paths:");
                for (marker, path) in entries {
                    if path == "<root>" {
                        let _ = writeln!(
                            out,
                            "  {marker} {} -> {}",
                            sanitize_controls(&report.left.path),
                            sanitize_controls(&report.right.path)
                        );
                    } else {
                        let _ = writeln!(out, "  {marker} {path}");
                    }
                }
            }
        }
        render_warnings(report, options, &mut out);
        return out;
    }

    let _ = writeln!(out);
    if options.rich {
        let title = if report.status == DiffStatus::Equal {
            "Result"
        } else {
            "Changes"
        };
        section_rule(title, options, &mut out);
    }
    render_node(&report.root, 0, options, &mut out);
    render_decompiled(report, options, &mut out);
    render_warnings(report, options, &mut out);
    out
}

fn render_plain_header(report: &DiffReport, options: &RenderOptions, out: &mut String) {
    let _ = writeln!(out, "{} {}", report.tool, report.version);
    let _ = writeln!(
        out,
        "left   : {} [{}, {}]",
        sanitize_controls(&report.left.path),
        report.left.kind.label(),
        human_size(report.left.size)
    );
    let _ = writeln!(
        out,
        "right  : {} [{}, {}]",
        sanitize_controls(&report.right.path),
        report.right.kind.label(),
        human_size(report.right.size)
    );
    let _ = writeln!(
        out,
        "status : {}",
        paint_status(report.status, options.color)
    );
    if !report.is_complete() {
        let _ = writeln!(
            out,
            "completeness: {} (part of the input was skipped by limits; see warnings)",
            paint("TRUNCATED", BOLD_YELLOW, options.color)
        );
    }
    if report.completely_different {
        let _ = writeln!(
            out,
            "verdict: {}",
            paint(
                "COMPLETELY DIFFERENT (>99% changed)",
                BOLD_RED,
                options.color
            )
        );
    }
    let _ = writeln!(
        out,
        "summary: {}",
        summary_parts(&report.summary).join(" | ")
    );
    let _ = writeln!(out, "elapsed: {} ms", report.elapsed_ms);
}

fn render_rich_header(report: &DiffReport, options: &RenderOptions, out: &mut String) {
    let title = format!("{} {}", report.tool, report.version);
    let _ = writeln!(out, "{}", paint(&title, BOLD_CYAN, options.color));
    let _ = writeln!(
        out,
        "{}",
        paint(&"=".repeat(RULE_WIDTH), DIM, options.color)
    );
    let _ = writeln!(
        out,
        "{} {}",
        field_label("left", options),
        input_summary(&report.left, options)
    );
    let _ = writeln!(
        out,
        "{} {}",
        field_label("right", options),
        input_summary(&report.right, options)
    );
    let _ = writeln!(
        out,
        "{} {}",
        field_label("status", options),
        status_badge(report.status, options)
    );
    if !report.is_complete() {
        let _ = writeln!(
            out,
            "{} {}",
            field_label("completeness", options),
            paint(
                "TRUNCATED (part of the input was skipped by limits)",
                BOLD_YELLOW,
                options.color
            )
        );
    }
    if report.completely_different {
        let _ = writeln!(
            out,
            "{} {}",
            field_label("verdict", options),
            paint(
                "COMPLETELY DIFFERENT (>99% changed)",
                BOLD_RED,
                options.color
            )
        );
    }
    let summary = summary_parts(&report.summary).join(" | ");
    let _ = writeln!(
        out,
        "{} {}",
        field_label("summary", options),
        paint(&summary, DIM, options.color)
    );
    let _ = writeln!(
        out,
        "{} {}",
        field_label("elapsed", options),
        paint(&format!("{} ms", report.elapsed_ms), DIM, options.color)
    );
}

fn field_label(label: &str, options: &RenderOptions) -> String {
    paint(&format!("{label:<8}"), CYAN, options.color)
}

fn input_summary(info: &ArtifactInfo, options: &RenderOptions) -> String {
    format!(
        "{}  {}",
        paint(&sanitize_controls(&info.path), BOLD_WHITE, options.color),
        paint(
            &format!("[{} | {}]", info.kind.label(), human_size(info.size)),
            DIM,
            options.color
        )
    )
}

fn section_rule(title: &str, options: &RenderOptions, out: &mut String) {
    let prefix = "-- ";
    let suffix = " ";
    let used = prefix.chars().count() + title.chars().count() + suffix.chars().count();
    let remaining = RULE_WIDTH.saturating_sub(used);
    let _ = writeln!(
        out,
        "{}{}{}{}",
        paint(prefix, DIM, options.color),
        paint(title, BOLD_CYAN, options.color),
        paint(suffix, DIM, options.color),
        paint(&"-".repeat(remaining), DIM, options.color)
    );
}

fn render_decompiled(report: &DiffReport, options: &RenderOptions, out: &mut String) {
    if let Some(decompiled) = &report.decompiled {
        render_decompiled_content(decompiled, 0, options, out);
    }
}

fn render_decompiled_content(
    decompiled: &DecompilationDiff,
    indent: usize,
    options: &RenderOptions,
    out: &mut String,
) {
    let diff = &decompiled.text;
    let pad = " ".repeat(indent);
    let _ = writeln!(out);
    let mut header = format!(
        "{pad}decompiled code ({}): +{} -{}",
        sanitize_controls(&decompiled.tool),
        diff.added_lines,
        diff.removed_lines
    );
    if decompiled.truncated || diff.truncated {
        header.push_str(" (truncated)");
    }
    let _ = writeln!(
        out,
        "{}",
        paint(
            &header,
            if options.rich { BOLD_CYAN } else { CYAN },
            options.color
        )
    );
    for hunk in &diff.hunks {
        render_hunk(hunk, &format!("{pad}  "), options, out);
    }
}

fn render_warnings(report: &DiffReport, options: &RenderOptions, out: &mut String) {
    if report.warnings.is_empty() {
        return;
    }
    let _ = writeln!(out);
    if options.rich {
        section_rule("Warnings", options, out);
        for warning in &report.warnings {
            let _ = writeln!(
                out,
                "  {} {}",
                paint("!", BOLD_YELLOW, options.color),
                truncate_chars(warning, 500)
            );
        }
    } else {
        let _ = writeln!(out, "{}:", paint("warnings", YELLOW, options.color));
        for warning in &report.warnings {
            let _ = writeln!(out, "  - {}", truncate_chars(warning, 500));
        }
    }
}

fn render_node(node: &DiffNode, indent: usize, options: &RenderOptions, out: &mut String) {
    let pad = " ".repeat(indent);
    let marker = if options.rich {
        paint(
            &marker_for(node.status).to_string(),
            marker_color(node.status),
            options.color,
        )
    } else {
        marker_for(node.status).to_string()
    };
    let label_text = super::display_node_label(node);
    let label = if node.label.is_empty() {
        String::new()
    } else if options.rich {
        format!("{} ", paint(&label_text, BOLD_WHITE, options.color))
    } else {
        format!("{label_text} ")
    };

    match &node.body {
        DiffBody::Text(diff) => {
            let mut header = format!(
                "{pad}{marker} {label}{}: +{} -{}",
                diff.format, diff.added_lines, diff.removed_lines
            );
            if diff.truncated {
                header.push_str(" (truncated)");
            }
            let _ = writeln!(out, "{header}");
            if diff.encoding_left != diff.encoding_right {
                let _ = writeln!(
                    out,
                    "{pad}  encodings: {} / {}",
                    diff.encoding_left, diff.encoding_right
                );
            }
            for hunk in &diff.hunks {
                render_hunk(hunk, &format!("{pad}  "), options, out);
            }
        }
        DiffBody::Tokens(diff) => {
            let mut header = format!(
                "{pad}{marker} {label}{} tokens: +{} -{}",
                diff.language, diff.added_tokens, diff.removed_tokens
            );
            if diff.truncated {
                header.push_str(" (truncated)");
            }
            let _ = writeln!(out, "{header}");
            for chunk in &diff.chunks {
                let (prefix, color) = match chunk.tag {
                    TextTag::Equal => (" ", DIM),
                    TextTag::Delete => ("-", RED),
                    TextTag::Insert => ("+", GREEN),
                };
                let line = format!("{pad}  {prefix}{}", truncate_chars(&chunk.text, 600));
                let _ = writeln!(out, "{}", paint(&line, color, options.color));
            }
        }
        DiffBody::Tree(diff) => {
            let mut header = format!(
                "{pad}{marker} {label}{} tree: +{} -{} ~{} >{}",
                diff.language,
                diff.added_nodes,
                diff.removed_nodes,
                diff.modified_nodes,
                diff.moved_nodes
            );
            if diff.truncated {
                header.push_str(" (truncated)");
            }
            let _ = writeln!(out, "{header}");
            for chunk in &diff.chunks {
                let (prefix, color) = match chunk.tag {
                    TreeTag::Added => ("+", GREEN),
                    TreeTag::Removed => ("-", RED),
                    TreeTag::Modified => ("~", YELLOW),
                    TreeTag::Moved => (">", CYAN),
                };
                let indent = " ".repeat(chunk.depth.min(12));
                let line = format!(
                    "{pad}  {prefix}{indent} {}  {}",
                    chunk.path,
                    truncate_chars(&chunk.text, 400)
                );
                let _ = writeln!(out, "{}", paint(&line, color, options.color));
            }
        }
        DiffBody::Notebook(diff) => {
            let mut header = format!(
                "{pad}{marker} {label}notebook: {} -> {} cells, +{} -{} ~{} moved {}",
                diff.left_cells,
                diff.right_cells,
                diff.cells_added,
                diff.cells_removed,
                diff.changed_cells.len(),
                diff.cells_moved
            );
            if diff.truncated {
                header.push_str(" (truncated)");
            }
            let _ = writeln!(out, "{header}");
            for cell in &diff.changed_cells {
                let mut detail = format!(
                    "{pad}  ~ cell {} [{}]",
                    cell.index,
                    sanitize_controls(&cell.cell_type)
                );
                if cell.execution_changed {
                    detail.push_str(" execution changed");
                }
                if cell.outputs_changed {
                    detail.push_str(" outputs changed");
                }
                let _ = writeln!(out, "{}", paint(&detail, YELLOW, options.color));
                if let Some(source) = &cell.source {
                    for hunk in &source.hunks {
                        render_hunk(hunk, &format!("{pad}    "), options, out);
                    }
                }
            }
        }
        DiffBody::Structured(diff) => {
            let mut header = format!(
                "{pad}{marker} {label}{}: {} change(s)",
                diff.format.to_uppercase(),
                diff.changes.len()
            );
            let breaking = diff.changes.iter().filter(|change| change.breaking).count();
            if breaking > 0 {
                header.push_str(&format!(", {breaking} breaking"));
            }
            if diff.truncated {
                header.push_str(" (truncated)");
            }
            let _ = writeln!(out, "{header}");
            for change in &diff.changes {
                let (prefix, color) = match change.kind {
                    StructuredChangeKind::Added => ("+", GREEN),
                    StructuredChangeKind::Removed => ("-", RED),
                    StructuredChangeKind::Changed => ("~", YELLOW),
                };
                let value = match (&change.left, &change.right) {
                    (Some(left), Some(right)) => {
                        format!("{} -> {}", compact_json(left), compact_json(right))
                    }
                    (Some(left), None) => compact_json(left),
                    (None, Some(right)) => compact_json(right),
                    (None, None) => String::new(),
                };
                let line = if change.breaking {
                    format!("{prefix} {}: {value} [breaking]", change.path)
                } else {
                    format!("{prefix} {}: {value}", change.path)
                };
                let _ = writeln!(
                    out,
                    "{pad}  {}",
                    paint(&truncate_chars(&line, 600), color, options.color)
                );
            }
        }
        DiffBody::Document(diff) => {
            let mut header = format!(
                "{pad}{marker} {label}{}: {} block(s), {} table(s), {} media change(s)",
                diff.format,
                diff.blocks.len(),
                diff.tables.len(),
                diff.media.len()
            );
            if diff.truncated {
                header.push_str(" (truncated)");
            }
            let _ = writeln!(out, "{header}");
            for field in &diff.metadata {
                let _ = writeln!(
                    out,
                    "{pad}  ~ {}: {} -> {}",
                    field.path,
                    truncate_chars(field.left.as_deref().unwrap_or("-"), 300),
                    truncate_chars(field.right.as_deref().unwrap_or("-"), 300)
                );
            }
            if let Some(visual) = &diff.visual {
                let _ = writeln!(
                    out,
                    "{pad}  visual: PDF visual diff ({} pages compared, {} added, {} removed, {} changed)",
                    visual.pages_compared,
                    visual.pages_added,
                    visual.pages_removed,
                    visual.changed_pages
                );
                let _ = writeln!(out, "{pad}    report: {}", visual.report_html);
                let _ = writeln!(out, "{pad}    json  : {}", visual.report_json);
                for page in visual.pages.iter().filter(|page| page.differing_pixels > 0) {
                    let _ = writeln!(
                        out,
                        "{pad}    page {}: {} / {} pixels differ ({:.2}%)",
                        page.page,
                        page.differing_pixels,
                        page.total_pixels,
                        page.diff_ratio * 100.0
                    );
                }
            }
            for block in &diff.blocks {
                let (prefix, color) = match block.change {
                    ChangeKind::Added => ("+", GREEN),
                    ChangeKind::Removed => ("-", RED),
                    ChangeKind::Changed => ("~", YELLOW),
                    ChangeKind::Moved => ("~", CYAN),
                };
                let detail = match block.change {
                    ChangeKind::Added => block.right.clone().unwrap_or_default(),
                    ChangeKind::Removed => block.left.clone().unwrap_or_default(),
                    ChangeKind::Changed | ChangeKind::Moved => format!(
                        "{} -> {}",
                        block.left.as_deref().unwrap_or("-"),
                        block.right.as_deref().unwrap_or("-")
                    ),
                };
                let moved = block
                    .moved_from
                    .as_deref()
                    .map(|from| format!(" (moved from {from})"))
                    .unwrap_or_default();
                let style = block
                    .style
                    .as_ref()
                    .map(|style| {
                        format!(
                            " [style {} -> {}]",
                            style.left.as_deref().unwrap_or("-"),
                            style.right.as_deref().unwrap_or("-")
                        )
                    })
                    .unwrap_or_default();
                let line = format!(
                    "{prefix} {} [{}]{moved}: {}{style}",
                    block.path,
                    block.kind,
                    truncate_chars(&detail, 600)
                );
                let _ = writeln!(out, "{pad}  {}", paint(&line, color, options.color));
            }
            for table in &diff.tables {
                let changed_rows = table.table.rows.len();
                if changed_rows == 0
                    && table.table.columns_added.is_empty()
                    && table.table.columns_removed.is_empty()
                {
                    continue;
                }
                let _ = writeln!(
                    out,
                    "{pad}  ~ {} [table]: {} changed row(s)",
                    table.path, changed_rows
                );
                for row in table.table.rows.iter().take(10) {
                    let _ = writeln!(
                        out,
                        "{pad}    {:?} {}: {} column change(s)",
                        row.kind,
                        row.key,
                        row.cells.len()
                    );
                    for cell in row.cells.iter().take(6) {
                        let _ = writeln!(
                            out,
                            "{pad}      {}: {} -> {}",
                            cell.column,
                            cell.left.as_deref().unwrap_or("-"),
                            cell.right.as_deref().unwrap_or("-")
                        );
                    }
                }
            }
            for media in &diff.media {
                let _ = writeln!(
                    out,
                    "{pad}  ~ {} [{}]: {}{}",
                    media.path,
                    media.kind,
                    media
                        .left_size
                        .map(|size| format!("{size} B"))
                        .unwrap_or_else(|| "-".to_string()),
                    media
                        .right_size
                        .map(|size| format!(" -> {size} B"))
                        .unwrap_or_default()
                );
            }
        }
        DiffBody::Presentation(diff) => {
            let mut header = format!(
                "{pad}{marker} {label}{}: {} -> {} slide(s), +{} -{} ~{}",
                diff.format,
                diff.left_slides,
                diff.right_slides,
                diff.slides_added,
                diff.slides_removed,
                diff.slides_moved
            );
            if diff.truncated {
                header.push_str(" (truncated)");
            }
            let _ = writeln!(out, "{header}");
            for slide in &diff.slides {
                let (prefix, _color) = match slide.change {
                    ChangeKind::Added => ("+", GREEN),
                    ChangeKind::Removed => ("-", RED),
                    ChangeKind::Changed => ("~", YELLOW),
                    ChangeKind::Moved => ("~", CYAN),
                };
                let title = slide.title.as_deref().unwrap_or("(untitled)");
                let position = match (slide.left_index, slide.right_index) {
                    (Some(left), Some(right)) => format!("{left} -> {right}"),
                    (None, Some(right)) => format!("new {right}"),
                    (Some(left), None) => format!("old {left}"),
                    (None, None) => "?".to_string(),
                };
                let hidden = slide
                    .visibility
                    .as_ref()
                    .map(|visibility| {
                        format!(
                            " (hidden {} -> {})",
                            visibility.left.as_deref().unwrap_or("-"),
                            visibility.right.as_deref().unwrap_or("-")
                        )
                    })
                    .unwrap_or_default();
                let _ = writeln!(out, "{pad}  {prefix} slide {position} [{title}]{hidden}");
                for shape in &slide.shapes {
                    let shape_prefix = match shape.change {
                        ChangeKind::Added => "+",
                        ChangeKind::Removed => "-",
                        ChangeKind::Changed => "~",
                        ChangeKind::Moved => "~",
                    };
                    let geometry = shape
                        .geometry
                        .as_ref()
                        .map(|geometry| {
                            format!(
                                " (geometry {} -> {})",
                                geometry.left.as_deref().unwrap_or("-"),
                                geometry.right.as_deref().unwrap_or("-")
                            )
                        })
                        .unwrap_or_default();
                    let font = shape
                        .font
                        .as_ref()
                        .map(|font| {
                            format!(
                                " (font {} -> {})",
                                font.left.as_deref().unwrap_or("-"),
                                font.right.as_deref().unwrap_or("-")
                            )
                        })
                        .unwrap_or_default();
                    let _ = writeln!(
                        out,
                        "{pad}    {shape_prefix} {} [{}]{geometry}{font}",
                        shape.name, shape.kind
                    );
                    if let Some(text) = &shape.text {
                        for line in text
                            .hunks
                            .iter()
                            .flat_map(|hunk| &hunk.lines)
                            .filter(|line| line.tag != TextTag::Equal)
                            .take(3)
                        {
                            let _ = writeln!(
                                out,
                                "{pad}      {} {}",
                                match line.tag {
                                    TextTag::Delete => "-",
                                    TextTag::Insert => "+",
                                    TextTag::Equal => " ",
                                },
                                truncate_chars(&line.text, 400)
                            );
                        }
                    }
                }
                if let Some(notes) = &slide.notes {
                    let changed = notes
                        .hunks
                        .iter()
                        .flat_map(|hunk| &hunk.lines)
                        .filter(|line| line.tag != TextTag::Equal)
                        .count();
                    if changed > 0 {
                        let _ = writeln!(out, "{pad}    ~ notes: {changed} changed line(s)");
                    }
                }
            }
        }
        DiffBody::Table(diff) => {
            let mut header = format!(
                "{pad}{marker} {label}{}: {} -> {} rows, {} changed row(s)",
                diff.format,
                diff.left_rows,
                diff.right_rows,
                diff.rows
                    .iter()
                    .filter(|row| matches!(row.kind, RowChangeKind::Changed | RowChangeKind::Moved))
                    .count()
            );
            if diff.truncated {
                header.push_str(" (truncated)");
            }
            let _ = writeln!(out, "{header}");
            if !diff.columns_added.is_empty() {
                let _ = writeln!(
                    out,
                    "{pad}  columns added: {}",
                    sanitize_controls(&diff.columns_added.join(", "))
                );
            }
            if !diff.columns_removed.is_empty() {
                let _ = writeln!(
                    out,
                    "{pad}  columns removed: {}",
                    sanitize_controls(&diff.columns_removed.join(", "))
                );
            }
            for column in &diff.columns_moved {
                let _ = writeln!(
                    out,
                    "{pad}  ~ column {} moved: position {} -> {}",
                    sanitize_controls(&column.name),
                    column.left_index,
                    column.right_index
                );
            }
            for row in &diff.rows {
                let (prefix, color) = match row.kind {
                    RowChangeKind::Added => ("+", GREEN),
                    RowChangeKind::Removed => ("-", RED),
                    RowChangeKind::Changed => ("~", YELLOW),
                    RowChangeKind::Moved => ("~", CYAN),
                };
                let mut detail = String::new();
                for cell in &row.cells {
                    if !detail.is_empty() {
                        detail.push_str("; ");
                    }
                    detail.push_str(&format!(
                        "{}: {} -> {}",
                        cell.column,
                        cell.left.as_deref().unwrap_or("-"),
                        cell.right.as_deref().unwrap_or("-")
                    ));
                }
                let line = format!("{prefix} {}: {detail}", row.key);
                let _ = writeln!(
                    out,
                    "{pad}  {}",
                    paint(&truncate_chars(&line, 700), color, options.color)
                );
            }
        }
        DiffBody::Workbook(diff) => {
            let mut header = format!(
                "{pad}{marker} {label}{}: {} sheet(s), {} changed cell(s)",
                diff.format,
                diff.sheets.len() + diff.sheets_added.len() + diff.sheets_removed.len(),
                diff.sheets
                    .iter()
                    .map(|sheet| sheet.cells.len())
                    .sum::<usize>()
            );
            if diff.truncated {
                header.push_str(" (truncated)");
            }
            let _ = writeln!(out, "{header}");
            for name in &diff.sheets_added {
                let _ = writeln!(out, "{pad}  + sheet {name}");
            }
            for name in &diff.sheets_removed {
                let _ = writeln!(out, "{pad}  - sheet {name}");
            }
            for sheet in &diff.sheets_moved {
                let _ = writeln!(
                    out,
                    "{pad}  ~ sheet {} moved: position {} -> {}",
                    sanitize_controls(&sheet.name),
                    sheet.left_index,
                    sheet.right_index
                );
            }
            for rename in &diff.sheets_renamed {
                let _ = writeln!(
                    out,
                    "{pad}  ~ sheet {} renamed to {}",
                    sanitize_controls(&rename.from),
                    sanitize_controls(&rename.to)
                );
            }
            for name in &diff.defined_names {
                let _ = writeln!(
                    out,
                    "{pad}  ~ defined name {}: {} -> {}",
                    sanitize_controls(&name.path),
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
                        ", hidden on right"
                    } else {
                        ", hidden on left"
                    }
                } else {
                    ""
                };
                let _ = writeln!(
                    out,
                    "{pad}  ~ sheet {}: {}x{} -> {}x{}, {} cell(s){hidden}",
                    sanitize_controls(&sheet.name),
                    sheet.left_rows,
                    sheet.left_columns,
                    sheet.right_rows,
                    sheet.right_columns,
                    sheet.cells.len()
                );
                for row in &sheet.rows_moved {
                    let _ = writeln!(
                        out,
                        "{pad}    ~ row {} moved to row {}",
                        row.left_row, row.right_row
                    );
                }
                for column in &sheet.columns_moved {
                    let _ = writeln!(
                        out,
                        "{pad}    ~ column {} moved to {}",
                        column.left_column, column.right_column
                    );
                }
                for merge in &sheet.merges_added {
                    let _ = writeln!(out, "{pad}    + merge {merge}");
                }
                for merge in &sheet.merges_removed {
                    let _ = writeln!(out, "{pad}    - merge {merge}");
                }
                for dimension in &sheet.hidden_dimensions {
                    let _ = writeln!(
                        out,
                        "{pad}    ~ {}: {} -> {}",
                        dimension.path,
                        dimension.left.as_deref().unwrap_or("-"),
                        dimension.right.as_deref().unwrap_or("-")
                    );
                }
                for drawing in &sheet.drawings_added {
                    let _ = writeln!(out, "{pad}    + drawing {drawing}");
                }
                for drawing in &sheet.drawings_removed {
                    let _ = writeln!(out, "{pad}    - drawing {drawing}");
                }
                for change in &sheet.chart_changes {
                    let _ = writeln!(
                        out,
                        "{pad}    ~ {}: {} -> {}",
                        change.path,
                        change.left.as_deref().unwrap_or("-"),
                        change.right.as_deref().unwrap_or("-")
                    );
                }
                for rule in &sheet.conditional_formats_added {
                    let _ = writeln!(out, "{pad}    + conditional format {rule}");
                }
                for rule in &sheet.conditional_formats_removed {
                    let _ = writeln!(out, "{pad}    - conditional format {rule}");
                }
                for cell in &sheet.cells {
                    let mut line = format!(
                        "{}: {} -> {}",
                        cell.column,
                        cell.left.as_deref().unwrap_or("-"),
                        cell.right.as_deref().unwrap_or("-")
                    );
                    if let Some(formula) = &cell.left_formula {
                        line.push_str(&format!(" [formula {}]", formula));
                    }
                    if let Some(formula) = &cell.right_formula {
                        line.push_str(&format!(" -> [formula {}]", formula));
                    }
                    let _ = writeln!(
                        out,
                        "{pad}    {}",
                        paint(&truncate_chars(&line, 600), YELLOW, options.color)
                    );
                }
            }
        }
        DiffBody::Image(diff) => {
            let _ = writeln!(
                out,
                "{pad}{marker} {label}image: {}x{} -> {}x{}",
                diff.left_width, diff.left_height, diff.right_width, diff.right_height
            );
            let mut detail = format!(
                "{} pixels differ ({:.2}%), max channel delta {}",
                diff.differing_pixels,
                diff.diff_ratio * 100.0,
                diff.max_channel_delta
            );
            if let Some(bbox) = diff.bounding_box {
                detail.push_str(&format!(
                    ", bbox ({}, {}) {}x{}",
                    bbox.x, bbox.y, bbox.width, bbox.height
                ));
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
                detail.push_str(&format!(
                    ", {} changed region(s): {}{}",
                    diff.regions.len(),
                    sample.join("; "),
                    if diff.regions_truncated { " ..." } else { "" }
                ));
            }
            let _ = writeln!(out, "{pad}  {detail}");
        }
        DiffBody::Audio(diff) => {
            let _ = writeln!(
                out,
                "{pad}{marker} {label}audio: {} {}ms -> {} {}ms",
                diff.left_format, diff.left_duration_ms, diff.right_format, diff.right_duration_ms
            );
            let _ = writeln!(
                out,
                "{pad}  bitrate {} -> {}, sample rate {} -> {}, channels {} -> {}",
                diff.left_bitrate
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                diff.right_bitrate
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                diff.left_sample_rate
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                diff.right_sample_rate
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                diff.left_channels
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string()),
                diff.right_channels
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "-".to_string())
            );
            for tag in &diff.tags {
                let line = format!(
                    "{}: {} -> {}",
                    tag.key,
                    tag.left.as_deref().unwrap_or("-"),
                    tag.right.as_deref().unwrap_or("-")
                );
                let _ = writeln!(
                    out,
                    "{pad}  {}",
                    paint(&truncate_chars(&line, 500), YELLOW, options.color)
                );
            }
            if diff.content_differs {
                let _ = writeln!(out, "{pad}  audio content differs");
            }
            if let Some(waveform) = &diff.waveform {
                let _ = writeln!(
                    out,
                    "{pad}  waveform: {} Hz, {} ch, {} ms windows, max delta {:.4}, {} changed{}",
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
                for (index, (left, right)) in waveform
                    .left
                    .iter()
                    .zip(&waveform.right)
                    .take(16)
                    .enumerate()
                {
                    let _ = writeln!(out, "{pad}    window {index}: {left:.4} -> {right:.4}");
                }
            }
        }
        DiffBody::Binary(diff) => {
            let mut header = format!(
                "{pad}{marker} {label}binary: {} -> {} bytes, {} byte(s) differ",
                diff.left_len, diff.right_len, diff.differing_bytes
            );
            if let Some(first) = diff.first_difference {
                header.push_str(&format!(", first at 0x{first:08x}"));
            }
            if let Some(tail) = &diff.tail {
                header.push_str(", ");
                header.push_str(&tail.description());
            }
            if diff.truncated {
                header.push_str(" (detail truncated)");
            }
            let _ = writeln!(out, "{header}");
            for row in &diff.rows {
                let _ = writeln!(
                    out,
                    "{pad}  0x{:08x}  left  {}  |{}|",
                    row.offset, row.left_hex, row.left_ascii
                );
                let _ = writeln!(
                    out,
                    "{pad}            right {}  |{}|",
                    row.right_hex, row.right_ascii
                );
            }
        }
        DiffBody::Directory(directory) => {
            let mut header = format!(
                "{pad}{marker} {label}directory: {} entries",
                directory.total_entries
            );
            if directory.truncated {
                header.push_str(" (truncated)");
            }
            let _ = writeln!(out, "{header}");
            render_entries(&directory.entries, indent + 2, options, out);
        }
        DiffBody::Archive(archive) => {
            let mut header = format!(
                "{pad}{marker} {label}{} archive: {} entries",
                archive.format, archive.total_entries
            );
            if archive.truncated {
                header.push_str(" (truncated)");
            }
            let _ = writeln!(out, "{header}");
            render_entries(&archive.entries, indent + 2, options, out);
        }
        DiffBody::OneSided(info) => {
            let (prefix, color) = match info.side {
                PresenceSide::LeftOnly => ('-', RED),
                PresenceSide::RightOnly => ('+', GREEN),
            };
            let line = format!(
                "{pad}{prefix} {label}[{}, {}] {}",
                info.kind.label(),
                human_size(info.size),
                match info.side {
                    PresenceSide::LeftOnly => "only in left",
                    PresenceSide::RightOnly => "only in right",
                }
            );
            let _ = writeln!(out, "{}", paint(&line, color, options.color));
        }
        DiffBody::Equal(info) => {
            let _ = writeln!(
                out,
                "{pad}{marker} {label}[{}] {}",
                info.kind.label(),
                sanitize_controls(&info.note)
            );
        }
        DiffBody::Unsupported(info) => {
            let line = format!("{pad}{marker} {label}{}", info.note);
            let _ = writeln!(
                out,
                "{}",
                paint(&truncate_chars(&line, 600), YELLOW, options.color)
            );
        }
    }
    if let Some(decompiled) = &node.decompiled {
        render_decompiled_content(decompiled, indent + 2, options, out);
    }
}

fn render_hunk(hunk: &TextHunk, line_pad: &str, options: &RenderOptions, out: &mut String) {
    let header = format!(
        "@@ -{},{} +{},{} @@",
        hunk.old_start, hunk.old_len, hunk.new_start, hunk.new_len
    );
    let _ = writeln!(
        out,
        "{line_pad}{}",
        paint(
            &header,
            if options.rich { BOLD_CYAN } else { CYAN },
            options.color
        )
    );
    let old_width = hunk
        .lines
        .iter()
        .filter_map(|line| line.old_lineno)
        .max()
        .unwrap_or(1)
        .to_string()
        .len()
        .max(3);
    let new_width = hunk
        .lines
        .iter()
        .filter_map(|line| line.new_lineno)
        .max()
        .unwrap_or(1)
        .to_string()
        .len()
        .max(3);
    for line in &hunk.lines {
        let (prefix, color) = match line.tag {
            TextTag::Equal => (" ", DIM),
            TextTag::Delete => ("-", RED),
            TextTag::Insert => ("+", GREEN),
        };
        let text = render_text_line(line, options.color);
        let rendered = paint(&format!("{prefix}{text}"), color, options.color);
        if options.rich {
            let old = line
                .old_lineno
                .map(|number| format!("{number:>old_width$}"))
                .unwrap_or_else(|| " ".repeat(old_width));
            let new = line
                .new_lineno
                .map(|number| format!("{number:>new_width$}"))
                .unwrap_or_else(|| " ".repeat(new_width));
            let gutter = paint(&format!("{old} {new} | "), DIM, options.color);
            let _ = writeln!(out, "{line_pad}{gutter}{rendered}");
        } else {
            let _ = writeln!(out, "{line_pad}{rendered}");
        }
    }
}

fn render_entries(entries: &[DiffNode], indent: usize, options: &RenderOptions, out: &mut String) {
    let pad = " ".repeat(indent);
    for entry in entries {
        if entry.renamed_from.is_some() {
            render_node(entry, indent, options, out);
            continue;
        }
        match &entry.body {
            DiffBody::Equal(info) => {
                let line = format!(
                    "{pad}= {} [{}]",
                    display_path(&entry.label),
                    info.kind.label()
                );
                if options.rich {
                    let _ = writeln!(out, "{}", paint(&line, DIM, options.color));
                } else {
                    let _ = writeln!(out, "{line}");
                }
            }
            DiffBody::OneSided(info) => {
                let (prefix, color) = match info.side {
                    PresenceSide::LeftOnly => ('-', RED),
                    PresenceSide::RightOnly => ('+', GREEN),
                };
                let line = format!(
                    "{pad}{prefix} {} [{}, {}]",
                    display_path(&entry.label),
                    info.kind.label(),
                    human_size(info.size)
                );
                let _ = writeln!(out, "{}", paint(&line, color, options.color));
            }
            _ => render_node(entry, indent, options, out),
        }
    }
}

fn status_style(status: DiffStatus) -> (&'static str, &'static str) {
    match status {
        DiffStatus::Equal => ("EQUAL", BOLD_GREEN),
        DiffStatus::Different => ("DIFFERENT", BOLD_RED),
        DiffStatus::Unsupported => ("UNSUPPORTED", BOLD_YELLOW),
        DiffStatus::Error => ("ERROR", BOLD_RED),
    }
}

fn status_badge(status: DiffStatus, options: &RenderOptions) -> String {
    let (text, badge) = match status {
        DiffStatus::Equal => ("EQUAL", BADGE_GREEN),
        DiffStatus::Different => ("DIFFERENT", BADGE_RED),
        DiffStatus::Unsupported => ("UNSUPPORTED", BADGE_YELLOW),
        DiffStatus::Error => ("ERROR", BADGE_RED),
    };
    if options.color {
        format!("{badge} {text} {RESET}")
    } else {
        format!("[{text}]")
    }
}

fn paint_status(status: DiffStatus, color: bool) -> String {
    let (text, code) = status_style(status);
    paint(text, code, color)
}

fn marker_color(status: DiffStatus) -> &'static str {
    match status {
        DiffStatus::Equal => BOLD_GREEN,
        DiffStatus::Different => BOLD_YELLOW,
        DiffStatus::Unsupported | DiffStatus::Error => BOLD_RED,
    }
}

fn marker_color_char(marker: char) -> &'static str {
    match marker {
        '+' => BOLD_GREEN,
        '-' => BOLD_RED,
        '~' => BOLD_YELLOW,
        '!' => BOLD_RED,
        _ => DIM,
    }
}

fn paint(text: &str, code: &str, enabled: bool) -> String {
    if enabled {
        format!("{code}{text}{RESET}")
    } else {
        text.to_string()
    }
}

fn compact_json(value: &serde_json::Value) -> String {
    let text = serde_json::to_string(value).unwrap_or_else(|_| "<invalid>".to_string());
    truncate_chars(&text, 180)
}

fn render_text_line(line: &TextLine, color: bool) -> String {
    if line.spans.is_empty() || !color {
        return truncate_chars(&line.text, 500);
    }
    let mut out = String::new();
    let mut budget = 500usize;
    for span in &line.spans {
        if budget == 0 {
            out.push_str("...");
            break;
        }
        let text = truncate_chars(&span.text, budget);
        budget = budget.saturating_sub(text.chars().count());
        if span.changed {
            out.push_str("\x1b[1;4m");
            out.push_str(&text);
            out.push_str("\x1b[22;24m");
        } else {
            out.push_str(&text);
        }
    }
    out
}
