use crate::model::*;
use crate::render::{display_path, human_size, marker_for, summary_parts, truncate_chars};
use std::fmt::Write;

pub fn render(report: &DiffReport) -> String {
    let mut out = String::new();
    out.push_str(
        r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>DiffEverything report</title>
<style>
:root {
  color-scheme: light;
  --bg: #f6f6f3;
  --panel: #ffffff;
  --text: #1d1d1b;
  --muted: #6d6d66;
  --border: #deded8;
  --add-bg: #e8f8ec;
  --add-text: #176b36;
  --del-bg: #fdecef;
  --del-text: #a12a35;
  --hunk-bg: #eef3ff;
  --accent: #3157c8;
}
* { box-sizing: border-box; }
body {
  margin: 0;
  background: var(--bg);
  color: var(--text);
  font: 15px/1.55 ui-sans-serif, system-ui, -apple-system, "Segoe UI", Roboto, sans-serif;
}
main { max-width: 1100px; margin: 0 auto; padding: 34px 20px 70px; }
h1 { margin: 2px 0 6px; font-size: 27px; letter-spacing: -0.02em; }
h2 { margin: 0 0 10px; font-size: 18px; letter-spacing: -0.01em; }
code, pre { font-family: ui-monospace, "Cascadia Code", Consolas, monospace; }
.report-head { display: flex; align-items: center; justify-content: space-between; gap: 20px; margin-bottom: 22px; }
.eyebrow { margin: 0; color: var(--accent); font-size: 11px; font-weight: 700; letter-spacing: 0.14em; }
.badge { display: inline-block; padding: 5px 11px; border-radius: 999px; font-size: 12px; font-weight: 700; letter-spacing: 0.06em; }
.badge.equal { background: var(--add-bg); color: var(--add-text); }
.badge.different, .badge.error { background: var(--del-bg); color: var(--del-text); }
.badge.unsupported { background: #fff4d6; color: #8a5a00; }
.verdict { margin: 0 0 18px; padding: 10px 14px; border: 1px solid #e7b6bc; border-radius: 10px; background: var(--del-bg); color: var(--del-text); font-weight: 700; letter-spacing: 0.02em; }
.files { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; margin-bottom: 14px; }
.file { padding: 12px 14px; background: var(--panel); border: 1px solid var(--border); border-radius: 10px; }
.file .label { display: block; color: var(--muted); font-size: 11px; font-weight: 700; letter-spacing: 0.08em; text-transform: uppercase; }
.file code { display: block; margin: 3px 0; overflow-wrap: anywhere; }
.file small { color: var(--muted); }
.summary { display: flex; flex-wrap: wrap; gap: 7px; margin-bottom: 22px; }
.pill { padding: 4px 9px; background: var(--panel); border: 1px solid var(--border); border-radius: 999px; color: var(--muted); font-size: 12px; }
.card { margin: 16px 0; padding: 18px; background: var(--panel); border: 1px solid var(--border); border-radius: 12px; }
.meta { margin: -4px 0 12px; color: var(--muted); font-size: 13px; }
pre.diff, pre.hex { margin: 0; padding: 14px; overflow-x: auto; background: #fbfbf9; border: 1px solid var(--border); border-radius: 8px; font-size: 13px; line-height: 1.5; }
pre.diff .line { display: block; white-space: pre-wrap; overflow-wrap: anywhere; padding: 0 4px; }
pre.diff .ctx { color: #4a4a44; }
pre.diff .add, pre.diff .del { display: inline-block; width: 100%; }
pre.diff .add { background: var(--add-bg); color: var(--add-text); }
pre.diff .del { background: var(--del-bg); color: var(--del-text); }
pre.diff .word { text-decoration: underline; text-underline-offset: 2px; }
pre.diff .hunk { display: block; margin: 6px 0 2px; padding: 2px 6px; background: var(--hunk-bg); color: #2e4a9e; border-radius: 4px; }
ul.changes { margin: 0; padding-left: 20px; }
ul.changes li { margin: 4px 0; overflow-wrap: anywhere; }
ul.changes li.add { color: var(--add-text); }
ul.changes li.del { color: var(--del-text); }
ul.changes li.mod { color: #8a5a00; }
table.sxs { width: 100%; border-collapse: collapse; table-layout: fixed; margin: 6px 0 18px; }
table.sxs th, table.sxs td { border: 1px solid var(--border); padding: 7px 9px; vertical-align: top; text-align: left; font-size: 13px; }
table.sxs th { background: #f0f0eb; color: var(--muted); font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; }
table.sxs td { white-space: pre-wrap; overflow-wrap: anywhere; }
table.sxs tr.add td { background: var(--add-bg); }
table.sxs tr.del td { background: var(--del-bg); }
table.sxs tr.mod td { background: #fff7df; }
table.sxs code.path { display: block; margin-bottom: 4px; color: var(--muted); }
dl.stats { display: grid; grid-template-columns: max-content 1fr; gap: 4px 16px; margin: 0; }
dl.stats dt { color: var(--muted); }
dl.stats dd { margin: 0; }
.entries { display: flex; flex-direction: column; gap: 6px; }
.row, summary { display: flex; align-items: center; gap: 9px; padding: 6px 8px; border-radius: 7px; }
.row:hover, summary:hover { background: #f0f0eb; }
.row.equal { color: var(--muted); }
.row.add { background: var(--add-bg); color: var(--add-text); }
.row.del { background: var(--del-bg); color: var(--del-text); }
summary { cursor: pointer; list-style: none; }
summary::-webkit-details-marker { display: none; }
summary .marker, .row .marker { width: 14px; text-align: center; font-weight: 700; }
.marker.different { color: var(--del-text); }
.marker.equal { color: var(--add-text); }
.marker.unsupported { color: #8a5a00; }
.kind { margin-left: auto; color: var(--muted); font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; }
.entry { border: 1px solid transparent; border-radius: 8px; }
.entry[open] { border-color: var(--border); padding: 4px 8px 8px; background: #fcfcfa; }
.entry-body { margin-top: 8px; }
.note { margin: 0; color: var(--muted); }
.note.warn { color: #8a5a00; }
.warnings { margin-top: 26px; padding: 14px 18px; background: #fff9e8; border: 1px solid #ecd9a5; border-radius: 10px; }
.warnings h2 { font-size: 15px; }
.warnings ul { margin: 0; padding-left: 20px; }
footer { margin-top: 34px; color: var(--muted); font-size: 12px; }
@media (max-width: 720px) {
  .files { grid-template-columns: 1fr; }
  .report-head { align-items: flex-start; flex-direction: column; }
}
</style>
</head>
<body>
<main>
"#,
    );

    let _ = writeln!(
        out,
        r#"<header class="report-head">
  <div>
    <p class="eyebrow">DIFF EVERYTHING</p>
    <h1>Comparison report</h1>
  </div>
  <span class="badge {status_class}">{status}</span>
</header>"#,
        status_class = status_class(report.status),
        status = report.status.label()
    );
    if report.completely_different {
        let _ = writeln!(
            out,
            r#"<div class="verdict">COMPLETELY DIFFERENT (&gt;99% changed)</div>"#
        );
    }
    if !report.is_complete() {
        let _ = writeln!(
            out,
            r#"<div class="verdict">TRUNCATED (part of the input was skipped by limits)</div>"#
        );
    }

    let _ = writeln!(
        out,
        r#"<div class="files">
  <div class="file">
    <span class="label">Left</span>
    <code>{left_path}</code>
    <small>{left_kind} · {left_size}</small>
  </div>
  <div class="file">
    <span class="label">Right</span>
    <code>{right_path}</code>
    <small>{right_kind} · {right_size}</small>
  </div>
</div>"#,
        left_path = esc(&report.left.path),
        left_kind = report.left.kind.label(),
        left_size = human_size(report.left.size),
        right_path = esc(&report.right.path),
        right_kind = report.right.kind.label(),
        right_size = human_size(report.right.size),
    );

    out.push_str("<div class=\"summary\">");
    for part in summary_parts(&report.summary) {
        let _ = write!(out, "<span class=\"pill\">{}</span>", esc(&part));
    }
    let _ = write!(out, "<span class=\"pill\">{} ms</span>", report.elapsed_ms);
    out.push_str("</div>\n");

    match &report.root.body {
        DiffBody::Directory(_) | DiffBody::Archive(_) => {
            out.push_str("<section class=\"card\">\n<h2>Comparison</h2>\n");
            render_body(&report.root, &mut out);
            out.push_str("</section>\n");
        }
        _ => {
            out.push_str("<section class=\"card\">\n");
            let _ = writeln!(
                out,
                "<h2>{}</h2>",
                if report.root.label.is_empty() {
                    "Diff".to_string()
                } else {
                    esc(&report.root.label)
                }
            );
            render_body(&report.root, &mut out);
            out.push_str("</section>\n");
        }
    }

    if let Some(decompiled) = &report.decompiled {
        out.push_str("<section class=\"card\">\n<h2>Decompiled code</h2>\n");
        let _ = writeln!(
            out,
            "<p class=\"meta\">{} diff: +{} / -{} lines{}</p>",
            esc(&decompiled.tool),
            decompiled.text.added_lines,
            decompiled.text.removed_lines,
            if decompiled.truncated || decompiled.text.truncated {
                " (truncated)"
            } else {
                ""
            }
        );
        out.push_str("<pre class=\"decompiled\">");
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
                let _ = writeln!(out, "{prefix}{}", esc(&line.text));
            }
        }
        out.push_str("</pre>\n</section>\n");
    }

    if !report.warnings.is_empty() {
        out.push_str("<section class=\"warnings\">\n<h2>Warnings</h2>\n<ul>\n");
        for warning in &report.warnings {
            let _ = writeln!(out, "<li>{}</li>", esc(&truncate_chars(warning, 500)));
        }
        out.push_str("</ul>\n</section>\n");
    }

    let _ = writeln!(
        out,
        "<footer>Generated by {} {}</footer>",
        report.tool, report.version
    );
    out.push_str("</main>\n</body>\n</html>\n");
    out
}

fn render_body(node: &DiffNode, out: &mut String) {
    match &node.body {
        DiffBody::Text(diff) => {
            let _ = writeln!(
                out,
                "<p class=\"meta\">{} diff · +{} / -{} lines{}</p>",
                esc(&diff.format),
                diff.added_lines,
                diff.removed_lines,
                if diff.truncated { " · truncated" } else { "" }
            );
            out.push_str("<pre class=\"diff\">");
            for hunk in &diff.hunks {
                let _ = writeln!(
                    out,
                    "<span class=\"hunk\">@@ -{},{} +{},{} @@</span>",
                    hunk.old_start, hunk.old_len, hunk.new_start, hunk.new_len
                );
                for line in &hunk.lines {
                    let (prefix, class) = match line.tag {
                        TextTag::Equal => (' ', "ctx"),
                        TextTag::Delete => ('-', "del"),
                        TextTag::Insert => ('+', "add"),
                    };
                    let content = render_text_line(line);
                    let _ = writeln!(out, "<span class=\"line {class}\">{prefix}{content}</span>");
                }
            }
            out.push_str("</pre>\n");
        }
        DiffBody::Tokens(diff) => {
            let _ = writeln!(
                out,
                "<p class=\"meta\">{} syntax tokens · +{} / -{}{}</p>",
                esc(&diff.language),
                diff.added_tokens,
                diff.removed_tokens,
                if diff.truncated { " · truncated" } else { "" }
            );
            out.push_str("<pre class=\"diff\">");
            for chunk in &diff.chunks {
                let (prefix, class) = match chunk.tag {
                    TextTag::Equal => (' ', "ctx"),
                    TextTag::Delete => ('-', "del"),
                    TextTag::Insert => ('+', "add"),
                };
                let _ = writeln!(
                    out,
                    "<span class=\"line {class}\">{prefix}{}</span>",
                    esc(&chunk.text)
                );
            }
            out.push_str("</pre>\n");
        }
        DiffBody::Tree(diff) => {
            let _ = writeln!(
                out,
                "<p class=\"meta\">{} syntax tree · +{} / -{} / ~{} / >{}{}</p>",
                esc(&diff.language),
                diff.added_nodes,
                diff.removed_nodes,
                diff.modified_nodes,
                diff.moved_nodes,
                if diff.truncated { " · truncated" } else { "" }
            );
            out.push_str("<ul class=\"changes\">\n");
            for chunk in &diff.chunks {
                let (class, marker) = match chunk.tag {
                    TreeTag::Added => ("add", "+"),
                    TreeTag::Removed => ("del", "-"),
                    TreeTag::Modified => ("mod", "~"),
                    TreeTag::Moved => ("move", ">"),
                };
                let _ = writeln!(
                    out,
                    "<li class=\"{class}\"><code>{marker} {}</code>: {}</li>",
                    esc(&chunk.path),
                    esc(&truncate_chars(&chunk.text, 400))
                );
            }
            out.push_str("</ul>\n");
        }
        DiffBody::Notebook(diff) => {
            let _ = writeln!(
                out,
                "<p class=\"meta\">Notebook · {} → {} cells · +{} / -{} / ~{} / moved {}{}</p>",
                diff.left_cells,
                diff.right_cells,
                diff.cells_added,
                diff.cells_removed,
                diff.changed_cells.len(),
                diff.cells_moved,
                if diff.truncated { " · truncated" } else { "" }
            );
            for cell in &diff.changed_cells {
                let _ = writeln!(
                    out,
                    "<h3>Cell {} [{}]</h3>",
                    cell.index,
                    esc(&cell.cell_type)
                );
                if cell.execution_changed || cell.outputs_changed {
                    let notes = [
                        cell.execution_changed.then_some("execution changed"),
                        cell.outputs_changed.then_some("outputs changed"),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · ");
                    let _ = writeln!(out, "<p class=\"meta\">{}</p>", esc(&notes));
                }
                if let Some(source) = &cell.source {
                    out.push_str("<pre class=\"diff\">");
                    for hunk in &source.hunks {
                        let _ = writeln!(
                            out,
                            "<span class=\"hunk\">@@ -{},{} +{},{} @@</span>",
                            hunk.old_start, hunk.old_len, hunk.new_start, hunk.new_len
                        );
                        for line in &hunk.lines {
                            let (prefix, class) = match line.tag {
                                TextTag::Equal => (' ', "ctx"),
                                TextTag::Delete => ('-', "del"),
                                TextTag::Insert => ('+', "add"),
                            };
                            let _ = writeln!(
                                out,
                                "<span class=\"line {class}\">{prefix}{}</span>",
                                render_text_line(line)
                            );
                        }
                    }
                    out.push_str("</pre>\n");
                }
            }
        }
        DiffBody::Structured(diff) => {
            let _ = writeln!(
                out,
                "<p class=\"meta\">{} changes · {}{}{}</p>",
                esc(&diff.format.to_uppercase()),
                diff.changes.len(),
                if diff.changes.iter().any(|change| change.breaking) {
                    " · breaking changes"
                } else {
                    ""
                },
                if diff.truncated { " · truncated" } else { "" }
            );
            out.push_str("<ul class=\"changes\">\n");
            for change in &diff.changes {
                let (class, marker) = match change.kind {
                    StructuredChangeKind::Added => ("add", "+"),
                    StructuredChangeKind::Removed => ("del", "-"),
                    StructuredChangeKind::Changed => ("mod", "~"),
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
                    "<li class=\"{class}\"><code>{marker} {}</code>{}: <code>{}</code></li>",
                    esc(&change.path),
                    if change.breaking { " [breaking]" } else { "" },
                    esc(&truncate_chars(&value, 300))
                );
            }
            out.push_str("</ul>\n");
        }
        DiffBody::Document(diff) => {
            let _ = writeln!(
                out,
                "<p class=\"meta\">{} document · {} block(s) · {} table(s) · {} media change(s){}</p>",
                esc(&diff.format),
                diff.blocks.len(),
                diff.tables.len(),
                diff.media.len(),
                if diff.truncated { " · truncated" } else { "" }
            );
            out.push_str("<ul class=\"changes\">\n");
            for field in &diff.metadata {
                let _ = writeln!(
                    out,
                    "<li class=\"mod\"><code>~ {}</code>: {} → {}</li>",
                    esc(&field.path),
                    esc(truncate_chars(field.left.as_deref().unwrap_or("-"), 300).as_str()),
                    esc(truncate_chars(field.right.as_deref().unwrap_or("-"), 300).as_str())
                );
            }
            if let Some(visual) = &diff.visual {
                let _ = writeln!(
                    out,
                    "<h3>PDF visual diff</h3><p class=\"meta\">{} pages compared · +{} / -{} · {} changed</p>",
                    visual.pages_compared,
                    visual.pages_added,
                    visual.pages_removed,
                    visual.changed_pages
                );
                let _ = writeln!(
                    out,
                    "<p class=\"meta\">HTML: <code>{}</code> · JSON: <code>{}</code></p>",
                    esc(&visual.report_html),
                    esc(&visual.report_json)
                );
                out.push_str("<ul class=\"changes\">\n");
                for page in visual.pages.iter().filter(|page| page.differing_pixels > 0) {
                    let _ = writeln!(
                        out,
                        "<li class=\"mod\"><code>page {}</code>: {} / {} pixels differ ({:.2}%)</li>",
                        page.page,
                        page.differing_pixels,
                        page.total_pixels,
                        page.diff_ratio * 100.0
                    );
                }
                out.push_str("</ul>\n");
            }
            for block in &diff.blocks {
                let class = match block.change {
                    ChangeKind::Added => "add",
                    ChangeKind::Removed => "del",
                    ChangeKind::Changed | ChangeKind::Moved => "mod",
                };
                let marker = match block.change {
                    ChangeKind::Added => "+",
                    ChangeKind::Removed => "-",
                    ChangeKind::Changed | ChangeKind::Moved => "~",
                };
                let moved = block
                    .moved_from
                    .as_deref()
                    .map(|from| format!(" (moved from {})", esc(from)))
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "<li class=\"{class}\"><code>{marker} {}</code> [{}]{moved}: {}</li>",
                    esc(&block.path),
                    esc(&block.kind),
                    esc(&truncate_chars(
                        &match block.change {
                            ChangeKind::Added => block.right.clone().unwrap_or_default(),
                            ChangeKind::Removed => block.left.clone().unwrap_or_default(),
                            ChangeKind::Changed | ChangeKind::Moved => format!(
                                "{} → {}",
                                block.left.as_deref().unwrap_or("-"),
                                block.right.as_deref().unwrap_or("-")
                            ),
                        },
                        500
                    ))
                );
            }
            out.push_str("</ul>\n");
            if !diff.blocks.is_empty() {
                out.push_str(
                    "<h3>Side-by-side</h3>\n<table class=\"sxs\"><thead><tr><th>LEFT</th><th>RIGHT</th></tr></thead><tbody>\n",
                );
                for block in &diff.blocks {
                    let class = match block.change {
                        ChangeKind::Added => "add",
                        ChangeKind::Removed => "del",
                        ChangeKind::Changed | ChangeKind::Moved => "mod",
                    };
                    let _ = writeln!(
                        out,
                        "<tr class=\"{class}\"><td><code class=\"path\">{} [{}]</code>{}</td><td><code class=\"path\">{} [{}]</code>{}</td></tr>",
                        esc(&block.path),
                        esc(&block.kind),
                        esc(block.left.as_deref().unwrap_or("-")),
                        esc(&block.path),
                        esc(&block.kind),
                        esc(block.right.as_deref().unwrap_or("-"))
                    );
                }
                out.push_str("</tbody></table>\n");
            }
            for table in &diff.tables {
                let _ = writeln!(
                    out,
                    "<h3>Table {}</h3><p class=\"meta\">{} changed row(s)</p>",
                    esc(&table.path),
                    table.table.rows.len()
                );
            }
            for media in &diff.media {
                let _ = writeln!(
                    out,
                    "<p class=\"meta\">media {} [{}]: {} → {}</p>",
                    esc(&media.path),
                    esc(&media.kind),
                    media
                        .left_size
                        .map(|size| format!("{size} B"))
                        .unwrap_or_else(|| "-".to_string()),
                    media
                        .right_size
                        .map(|size| format!("{size} B"))
                        .unwrap_or_else(|| "-".to_string())
                );
            }
        }
        DiffBody::Presentation(diff) => {
            let _ = writeln!(
                out,
                "<p class=\"meta\">{} presentation · {} → {} slide(s) · +{} / -{} / moved {}{}</p>",
                esc(&diff.format),
                diff.left_slides,
                diff.right_slides,
                diff.slides_added,
                diff.slides_removed,
                diff.slides_moved,
                if diff.truncated { " · truncated" } else { "" }
            );
            for slide in &diff.slides {
                let class = match slide.change {
                    ChangeKind::Added => "add",
                    ChangeKind::Removed => "del",
                    ChangeKind::Changed | ChangeKind::Moved => "mod",
                };
                let title = slide.title.as_deref().unwrap_or("(untitled)");
                let _ = writeln!(
                    out,
                    "<h3 class=\"{class}\">Slide {}{}</h3>",
                    slide
                        .right_index
                        .or(slide.left_index)
                        .map(|index| index.to_string())
                        .unwrap_or_else(|| "?".to_string()),
                    if slide.change == ChangeKind::Moved {
                        " (moved)"
                    } else {
                        ""
                    }
                );
                let _ = writeln!(out, "<p class=\"meta\">{}</p>", esc(title));
                if !slide.shapes.is_empty() {
                    out.push_str("<ul class=\"changes\">\n");
                    for shape in &slide.shapes {
                        let shape_class = match shape.change {
                            ChangeKind::Added => "add",
                            ChangeKind::Removed => "del",
                            ChangeKind::Changed | ChangeKind::Moved => "mod",
                        };
                        let _ = writeln!(
                            out,
                            "<li class=\"{shape_class}\"><code>{}</code> [{}]</li>",
                            esc(&shape.path),
                            esc(&shape.kind)
                        );
                    }
                    out.push_str("</ul>\n");
                }
                if let Some(notes) = &slide.notes {
                    let changed = notes
                        .hunks
                        .iter()
                        .flat_map(|hunk| &hunk.lines)
                        .filter(|line| line.tag != TextTag::Equal)
                        .count();
                    if changed > 0 {
                        let _ = writeln!(
                            out,
                            "<p class=\"meta\">notes: {} changed line(s)</p>",
                            changed
                        );
                    }
                }
            }
            if !diff.slides.is_empty() {
                out.push_str(
                    "<h3>Slide side-by-side</h3>\n<table class=\"sxs\"><thead><tr><th>LEFT</th><th>RIGHT</th></tr></thead><tbody>\n",
                );
                for slide in &diff.slides {
                    let class = match slide.change {
                        ChangeKind::Added => "add",
                        ChangeKind::Removed => "del",
                        ChangeKind::Changed | ChangeKind::Moved => "mod",
                    };
                    let left_index = slide
                        .left_index
                        .map(|index| index.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    let right_index = slide
                        .right_index
                        .map(|index| index.to_string())
                        .unwrap_or_else(|| "-".to_string());
                    let title = slide.title.as_deref().unwrap_or("(untitled)");
                    let left_notes = slide
                        .notes
                        .as_ref()
                        .map(|notes| text_side(notes, TextTag::Delete))
                        .unwrap_or_default();
                    let right_notes = slide
                        .notes
                        .as_ref()
                        .map(|notes| text_side(notes, TextTag::Insert))
                        .unwrap_or_default();
                    let _ = writeln!(
                        out,
                        "<tr class=\"{class}\"><td><code class=\"path\">slide {left_index}</code>{}</td><td><code class=\"path\">slide {right_index}</code>{}</td></tr>",
                        esc(&format!("{title}\n{left_notes}")),
                        esc(&format!("{title}\n{right_notes}"))
                    );
                }
                out.push_str("</tbody></table>\n");
            }
        }
        DiffBody::Table(diff) => {
            let _ = writeln!(
                out,
                "<p class=\"meta\">{} table · {} → {} rows · {} changed row(s){}</p>",
                esc(&diff.format),
                diff.left_rows,
                diff.right_rows,
                diff.rows
                    .iter()
                    .filter(|row| matches!(row.kind, RowChangeKind::Changed | RowChangeKind::Moved))
                    .count(),
                if diff.truncated { " · truncated" } else { "" }
            );
            if !diff.columns_added.is_empty() {
                let _ = writeln!(
                    out,
                    "<p class=\"meta\">Columns added: {}</p>",
                    esc(&diff.columns_added.join(", "))
                );
            }
            if !diff.columns_removed.is_empty() {
                let _ = writeln!(
                    out,
                    "<p class=\"meta\">Columns removed: {}</p>",
                    esc(&diff.columns_removed.join(", "))
                );
            }
            for column in &diff.columns_moved {
                let _ = writeln!(
                    out,
                    "<p class=\"meta\">Column {} moved: {} → {}</p>",
                    esc(&column.name),
                    column.left_index,
                    column.right_index
                );
            }
            out.push_str("<ul class=\"changes\">\n");
            for row in &diff.rows {
                let (class, marker, verb) = match row.kind {
                    RowChangeKind::Added => ("add", "+", "added"),
                    RowChangeKind::Removed => ("del", "-", "removed"),
                    RowChangeKind::Changed => ("mod", "~", "changed"),
                    RowChangeKind::Moved => ("mod", "~", "moved"),
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
                    "<li class=\"{class}\"><code>{marker} {}</code> ({verb}): {}</li>",
                    esc(&row.key),
                    esc(&truncate_chars(&cells.join("; "), 500))
                );
            }
            out.push_str("</ul>\n");
        }
        DiffBody::Workbook(diff) => {
            let _ = writeln!(
                out,
                "<p class=\"meta\">{} workbook · {} sheet(s) · {} changed cell(s){}</p>",
                esc(&diff.format),
                diff.sheets.len() + diff.sheets_added.len() + diff.sheets_removed.len(),
                diff.sheets
                    .iter()
                    .map(|sheet| sheet.cells.len())
                    .sum::<usize>(),
                if diff.truncated { " · truncated" } else { "" }
            );
            out.push_str("<ul class=\"changes\">\n");
            for name in &diff.sheets_added {
                let _ = writeln!(
                    out,
                    "<li class=\"add\"><code>+ sheet {}</code> added</li>",
                    esc(name)
                );
            }
            for name in &diff.sheets_removed {
                let _ = writeln!(
                    out,
                    "<li class=\"del\"><code>- sheet {}</code> removed</li>",
                    esc(name)
                );
            }
            for sheet in &diff.sheets_moved {
                let _ = writeln!(
                    out,
                    "<li class=\"move\"><code>~ sheet {}</code> moved: {} → {}</li>",
                    esc(&sheet.name),
                    sheet.left_index,
                    sheet.right_index
                );
            }
            for rename in &diff.sheets_renamed {
                let _ = writeln!(
                    out,
                    "<li class=\"move\"><code>~ sheet {}</code> renamed to {}</li>",
                    esc(&rename.from),
                    esc(&rename.to)
                );
            }
            for name in &diff.defined_names {
                let _ = writeln!(
                    out,
                    "<li class=\"mod\"><code>~ defined name {}</code>: {} → {}</li>",
                    esc(&name.path),
                    esc(name.left.as_deref().unwrap_or("-")),
                    esc(name.right.as_deref().unwrap_or("-"))
                );
            }
            out.push_str("</ul>\n");
            for sheet in &diff.sheets {
                if sheet.cells.is_empty()
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
                        " · hidden on right"
                    } else {
                        " · hidden on left"
                    }
                } else {
                    ""
                };
                let _ = writeln!(
                    out,
                    "<h3>Sheet {} ({}x{} → {}x{}){hidden}</h3>",
                    esc(&sheet.name),
                    sheet.left_rows,
                    sheet.left_columns,
                    sheet.right_rows,
                    sheet.right_columns
                );
                out.push_str("<ul class=\"changes\">\n");
                for merge in &sheet.merges_added {
                    let _ = writeln!(out, "<li class=\"add\"><code>+ merge {merge}</code></li>");
                }
                for merge in &sheet.merges_removed {
                    let _ = writeln!(out, "<li class=\"del\"><code>- merge {merge}</code></li>");
                }
                for dimension in &sheet.hidden_dimensions {
                    let _ = writeln!(
                        out,
                        "<li class=\"mod\"><code>{}</code>: {} → {}</li>",
                        esc(&dimension.path),
                        esc(dimension.left.as_deref().unwrap_or("-")),
                        esc(dimension.right.as_deref().unwrap_or("-"))
                    );
                }
                for drawing in &sheet.drawings_added {
                    let _ = writeln!(
                        out,
                        "<li class=\"add\"><code>+ drawing {drawing}</code></li>"
                    );
                }
                for drawing in &sheet.drawings_removed {
                    let _ = writeln!(
                        out,
                        "<li class=\"del\"><code>- drawing {drawing}</code></li>"
                    );
                }
                for change in &sheet.chart_changes {
                    let _ = writeln!(
                        out,
                        "<li class=\"mod\"><code>~ {}</code>: {} → {}</li>",
                        esc(&change.path),
                        esc(change.left.as_deref().unwrap_or("-")),
                        esc(change.right.as_deref().unwrap_or("-"))
                    );
                }
                for rule in &sheet.conditional_formats_added {
                    let _ = writeln!(
                        out,
                        "<li class=\"add\"><code>+ conditional format</code> {}</li>",
                        esc(rule)
                    );
                }
                for rule in &sheet.conditional_formats_removed {
                    let _ = writeln!(
                        out,
                        "<li class=\"del\"><code>- conditional format</code> {}</li>",
                        esc(rule)
                    );
                }
                for cell in &sheet.cells {
                    let formulas = match (&cell.left_formula, &cell.right_formula) {
                        (Some(left), Some(right)) => {
                            format!(" [formula {left} → {right}]")
                        }
                        (Some(left), None) => format!(" [formula {left}]"),
                        (None, Some(right)) => format!(" [formula {right}]"),
                        (None, None) => String::new(),
                    };
                    let _ = writeln!(
                        out,
                        "<li class=\"mod\"><code>{}</code>: <code>{}</code> → <code>{}</code>{}</li>",
                        esc(&cell.column),
                        esc(cell.left.as_deref().unwrap_or("-")),
                        esc(cell.right.as_deref().unwrap_or("-")),
                        esc(&formulas)
                    );
                }
                out.push_str("</ul>\n");
                if !sheet.cells.is_empty() {
                    out.push_str(
                        "<h3>Cell side-by-side</h3>\n<table class=\"sxs\"><thead><tr><th>LEFT</th><th>RIGHT</th></tr></thead><tbody>\n",
                    );
                    for cell in &sheet.cells {
                        let _ = writeln!(
                            out,
                            "<tr class=\"mod\"><td><code class=\"path\">{}</code>{}</td><td><code class=\"path\">{}</code>{}</td></tr>",
                            esc(&cell.column),
                            esc(cell.left.as_deref().unwrap_or("-")),
                            esc(&cell.column),
                            esc(cell.right.as_deref().unwrap_or("-"))
                        );
                    }
                    out.push_str("</tbody></table>\n");
                }
            }
        }
        DiffBody::Image(diff) => {
            out.push_str("<dl class=\"stats\">\n");
            let _ = writeln!(
                out,
                "<dt>Dimensions</dt><dd>{}x{} → {}x{}</dd>",
                diff.left_width, diff.left_height, diff.right_width, diff.right_height
            );
            let _ = writeln!(
                out,
                "<dt>Differing pixels</dt><dd>{} / {} ({:.2}%)</dd>",
                diff.differing_pixels,
                diff.total_pixels,
                diff.diff_ratio * 100.0
            );
            let _ = writeln!(
                out,
                "<dt>Max channel delta</dt><dd>{}</dd>",
                diff.max_channel_delta
            );
            if let Some(bbox) = diff.bounding_box {
                let _ = writeln!(
                    out,
                    "<dt>Bounding box</dt><dd>({}, {}) {}x{}</dd>",
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
                    "<dt>Changed regions</dt><dd>{}{}{}</dd>",
                    diff.regions.len(),
                    if sample.is_empty() { "" } else { ": " },
                    sample.join("; ")
                );
            }
            out.push_str("</dl>\n");
        }
        DiffBody::Audio(diff) => {
            out.push_str("<dl class=\"stats\">\n");
            let _ = writeln!(
                out,
                "<dt>Format</dt><dd>{} → {}</dd>",
                esc(&diff.left_format),
                esc(&diff.right_format)
            );
            let _ = writeln!(
                out,
                "<dt>Duration</dt><dd>{} ms → {} ms</dd>",
                diff.left_duration_ms, diff.right_duration_ms
            );
            let _ = writeln!(
                out,
                "<dt>Bitrate</dt><dd>{:?} → {:?}</dd>",
                diff.left_bitrate, diff.right_bitrate
            );
            let _ = writeln!(
                out,
                "<dt>Sample rate</dt><dd>{:?} → {:?}</dd>",
                diff.left_sample_rate, diff.right_sample_rate
            );
            let _ = writeln!(
                out,
                "<dt>Channels</dt><dd>{:?} → {:?}</dd>",
                diff.left_channels, diff.right_channels
            );
            if diff.content_differs {
                out.push_str("<dt>Content</dt><dd>audio data differs</dd>\n");
            }
            out.push_str("</dl>\n");
            if let Some(waveform) = &diff.waveform {
                let _ = writeln!(
                    out,
                    "<p class=\"meta\">Waveform {} Hz, {} ch, {} ms windows, max delta {:.4}, {} changed{}</p>",
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
                    out.push_str("<ul class=\"changes\">\n");
                    for (index, (left, right)) in waveform
                        .left
                        .iter()
                        .zip(&waveform.right)
                        .take(16)
                        .enumerate()
                    {
                        let _ = writeln!(
                            out,
                            "<li class=\"mod\">window {index}: {left:.4} &#8594; {right:.4}</li>"
                        );
                    }
                    out.push_str("</ul>\n");
                }
            }
            if !diff.tags.is_empty() {
                out.push_str("<ul class=\"changes\">\n");
                for tag in &diff.tags {
                    let _ = writeln!(
                        out,
                        "<li class=\"mod\"><code>{}</code>: <code>{}</code> → <code>{}</code></li>",
                        esc(&tag.key),
                        esc(tag.left.as_deref().unwrap_or("-")),
                        esc(tag.right.as_deref().unwrap_or("-"))
                    );
                }
                out.push_str("</ul>\n");
            }
        }
        DiffBody::Binary(diff) => {
            let _ = writeln!(
                out,
                "<p class=\"meta\">Binary diff · {} → {} bytes · {} differing byte(s){}{}</p>",
                diff.left_len,
                diff.right_len,
                diff.differing_bytes,
                diff.first_difference
                    .map(|first| format!(" · first at 0x{first:08x}"))
                    .unwrap_or_default(),
                if diff.truncated { " · truncated" } else { "" }
            );
            if !diff.rows.is_empty() {
                out.push_str("<pre class=\"hex\">");
                for row in &diff.rows {
                    let _ = writeln!(
                        out,
                        "0x{:08x}  left  {}  |{}|",
                        row.offset,
                        esc(&row.left_hex),
                        esc(&row.left_ascii)
                    );
                    let _ = writeln!(
                        out,
                        "            right {}  |{}|",
                        esc(&row.right_hex),
                        esc(&row.right_ascii)
                    );
                }
                out.push_str("</pre>\n");
            }
        }
        DiffBody::Directory(directory) => {
            let _ = writeln!(
                out,
                "<p class=\"meta\">Directory · {} entries{}</p>",
                directory.total_entries,
                if directory.truncated {
                    " · truncated"
                } else {
                    ""
                }
            );
            out.push_str("<div class=\"entries\">\n");
            for entry in &directory.entries {
                render_entry(entry, out);
            }
            out.push_str("</div>\n");
        }
        DiffBody::Archive(archive) => {
            let _ = writeln!(
                out,
                "<p class=\"meta\">{} archive · {} entries{}</p>",
                esc(&archive.format),
                archive.total_entries,
                if archive.truncated {
                    " · truncated"
                } else {
                    ""
                }
            );
            out.push_str("<div class=\"entries\">\n");
            for entry in &archive.entries {
                render_entry(entry, out);
            }
            out.push_str("</div>\n");
        }
        DiffBody::OneSided(info) => {
            let (class, marker, side) = match info.side {
                PresenceSide::LeftOnly => ("del", "-", "only in left"),
                PresenceSide::RightOnly => ("add", "+", "only in right"),
            };
            let _ = writeln!(
                out,
                "<div class=\"row {class}\"><span class=\"marker\">{marker}</span><code>{}</code><span class=\"kind\">{} · {} · {}</span></div>",
                esc(&display_path(&node.label)),
                info.kind.label(),
                human_size(info.size),
                side
            );
        }
        DiffBody::Equal(info) => {
            let _ = writeln!(
                out,
                "<p class=\"note\">= {} ({})</p>",
                esc(&display_path(&node.label)),
                info.kind.label()
            );
        }
        DiffBody::Unsupported(info) => {
            let _ = writeln!(
                out,
                "<p class=\"note warn\">! {}</p>",
                esc(&truncate_chars(&info.note, 500))
            );
        }
    }
    if let Some(decompiled) = &node.decompiled {
        let _ = writeln!(
            out,
            "<details class=\"decompiled\"><summary>Decompiled code: +{} / -{} lines</summary><pre>",
            decompiled.text.added_lines, decompiled.text.removed_lines
        );
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
                let _ = writeln!(out, "{prefix}{}", esc(&line.text));
            }
        }
        out.push_str("</pre></details>\n");
    }
}

fn render_entry(entry: &DiffNode, out: &mut String) {
    match &entry.body {
        DiffBody::Equal(info) => {
            let _ = writeln!(
                out,
                "<div class=\"row equal\"><span class=\"marker\">=</span><code>{}</code><span class=\"kind\">{}</span></div>",
                esc(&display_path(&entry.label)),
                info.kind.label()
            );
        }
        DiffBody::OneSided(info) => {
            let (class, marker, side) = match info.side {
                PresenceSide::LeftOnly => ("del", "-", "only in left"),
                PresenceSide::RightOnly => ("add", "+", "only in right"),
            };
            let _ = writeln!(
                out,
                "<div class=\"row {class}\"><span class=\"marker\">{marker}</span><code>{}</code><span class=\"kind\">{} · {} · {}</span></div>",
                esc(&display_path(&entry.label)),
                info.kind.label(),
                human_size(info.size),
                side
            );
        }
        _ => {
            let marker = marker_for(entry.status);
            let open = if entry.status.is_different() {
                " open"
            } else {
                ""
            };
            let _ = writeln!(
                out,
                "<details class=\"entry\"{open}><summary><span class=\"marker {status_class}\">{marker}</span><code>{}</code><span class=\"kind\">{}</span></summary><div class=\"entry-body\">",
                esc(&display_path(&entry.label)),
                body_kind_label(&entry.body),
                status_class = status_class(entry.status),
            );
            render_body(entry, out);
            out.push_str("</div></details>\n");
        }
    }
}

fn text_side(diff: &TextDiff, tag: TextTag) -> String {
    diff.hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .filter(|line| line.tag == tag)
        .map(|line| line.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

fn body_kind_label(body: &DiffBody) -> &'static str {
    match body {
        DiffBody::Text(_) => "text",
        DiffBody::Tokens(_) => "tokens",
        DiffBody::Tree(_) => "tree",
        DiffBody::Structured(diff) => match diff.format.as_str() {
            "json" => "json",
            "yaml" => "yaml",
            "toml" => "toml",
            "xml" => "xml",
            "plist" => "plist",
            "pickle" => "pickle",
            "openapi" => "openapi",
            "ini" => "ini",
            "html" => "html",
            _ => "structured",
        },
        DiffBody::Document(_) => "document",
        DiffBody::Presentation(_) => "presentation",
        DiffBody::Table(_) => "table",
        DiffBody::Workbook(_) => "workbook",
        DiffBody::Notebook(_) => "notebook",
        DiffBody::Image(_) => "image",
        DiffBody::Audio(_) => "audio",
        DiffBody::Binary(_) => "binary",
        DiffBody::Directory(_) => "directory",
        DiffBody::Archive(_) => "archive",
        DiffBody::OneSided(info) => info.kind.label(),
        DiffBody::Equal(info) => info.kind.label(),
        DiffBody::Unsupported(_) => "unsupported",
    }
}

fn status_class(status: DiffStatus) -> &'static str {
    match status {
        DiffStatus::Equal => "equal",
        DiffStatus::Different => "different",
        DiffStatus::Unsupported => "unsupported",
        DiffStatus::Error => "error",
    }
}

fn compact_json(value: &serde_json::Value) -> String {
    let text = serde_json::to_string(value).unwrap_or_else(|_| "<invalid>".to_string());
    truncate_chars(&text, 220)
}

fn render_text_line(line: &TextLine) -> String {
    if line.spans.is_empty() {
        return esc(&line.text);
    }
    let mut out = String::new();
    for span in &line.spans {
        if span.changed {
            let _ = write!(out, "<strong class=\"word\">{}</strong>", esc(&span.text));
        } else {
            out.push_str(&esc(&span.text));
        }
    }
    out
}

fn esc(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}
