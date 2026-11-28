use crate::diff::package::Package;
use crate::model::{CellChange, FieldChange, SheetDiff, SheetMove, SheetRename, WorkbookDiff};
use calamine::{Data, Reader, SheetVisible, open_workbook_auto_from_rs};
use quick_xml::events::Event;
use quick_xml::{Reader as XmlReader, XmlVersion};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub fn compare_workbooks(
    left: &[u8],
    right: &[u8],
    format: &str,
    max_cells: usize,
    compare_formulas: bool,
    ignore_columns: &[String],
) -> std::result::Result<WorkbookDiff, String> {
    let parse_side = |bytes: &[u8]| -> std::result::Result<_, String> {
        let book = read_workbook(bytes)?;
        let meta = if format == "xlsx" {
            Package::new(bytes)
                .ok()
                .and_then(|package| read_xlsx_metadata(&package).ok())
        } else {
            None
        };
        Ok((book, meta))
    };
    let (left_side, right_side) = rayon::join(|| parse_side(left), || parse_side(right));
    let (left_book, left_meta) = left_side?;
    let (right_book, right_meta) = right_side?;
    let ignored_columns: BTreeSet<String> = ignore_columns
        .iter()
        .map(|column| column.trim().to_ascii_uppercase())
        .filter(|column| !column.is_empty())
        .collect();
    let defined_names = compare_defined_names(left_meta.as_ref(), right_meta.as_ref());

    let left_names = left_book.names();
    let right_names = right_book.names();
    let left_set: BTreeSet<&str> = left_names.iter().map(String::as_str).collect();
    let right_set: BTreeSet<&str> = right_names.iter().map(String::as_str).collect();
    let mut sheets_added: Vec<String> = right_names
        .iter()
        .filter(|name| !left_set.contains(name.as_str()))
        .cloned()
        .collect();
    let mut sheets_removed: Vec<String> = left_names
        .iter()
        .filter(|name| !right_set.contains(name.as_str()))
        .cloned()
        .collect();
    let mut sheets_renamed = Vec::new();
    let mut rename_candidates = Vec::new();
    for removed in &sheets_removed {
        let Some(left_sheet) = left_book.sheet(removed) else {
            continue;
        };
        let fingerprint = left_sheet.fingerprint();
        if let Some(added) = sheets_added.iter().find(|added| {
            right_book
                .sheet(added)
                .is_some_and(|right_sheet| right_sheet.fingerprint() == fingerprint)
        }) {
            rename_candidates.push((removed.clone(), added.clone()));
        }
    }
    for (from, to) in rename_candidates {
        sheets_removed.retain(|name| name != &from);
        sheets_added.retain(|name| name != &to);
        sheets_renamed.push(SheetRename { from, to });
    }
    let common_left: Vec<String> = left_names
        .iter()
        .filter(|name| right_set.contains(name.as_str()))
        .cloned()
        .collect();
    let common_right: Vec<String> = right_names
        .iter()
        .filter(|name| left_set.contains(name.as_str()))
        .cloned()
        .collect();
    let sheets_moved = detect_sheet_moves(&left_names, &right_names, &common_left, &common_right);

    let mut sheets = Vec::new();
    let mut truncated = false;
    for name in &common_left {
        let left_sheet = left_book.sheet(name).expect("common sheet exists");
        let right_sheet = right_book.sheet(name).expect("common sheet exists");
        let (cells, rows_moved, columns_moved, sheet_truncated) = compare_sheet_cells(
            left_sheet,
            right_sheet,
            compare_formulas,
            max_cells,
            &ignored_columns,
        );
        truncated |= sheet_truncated;
        let left_sheet_meta = left_meta
            .as_ref()
            .and_then(|meta| meta.sheets.get(name))
            .cloned()
            .unwrap_or_default();
        let right_sheet_meta = right_meta
            .as_ref()
            .and_then(|meta| meta.sheets.get(name))
            .cloned()
            .unwrap_or_default();
        let (merges_added, merges_removed) =
            set_difference(&left_sheet_meta.merges, &right_sheet_meta.merges);
        let (drawings_added, drawings_removed) =
            set_difference(&left_sheet_meta.drawings, &right_sheet_meta.drawings);
        let hidden_dimensions =
            compare_hidden_dimensions(name, &left_sheet_meta, &right_sheet_meta);
        let chart_changes =
            compare_chart_hashes(name, &left_sheet_meta.charts, &right_sheet_meta.charts);
        let (conditional_formats_added, conditional_formats_removed) = set_difference(
            &left_sheet_meta.conditional_formats,
            &right_sheet_meta.conditional_formats,
        );

        sheets.push(SheetDiff {
            name: name.clone(),
            left_rows: left_sheet.rows,
            left_columns: left_sheet.columns,
            right_rows: right_sheet.rows,
            right_columns: right_sheet.columns,
            left_hidden: left_sheet.hidden,
            right_hidden: right_sheet.hidden,
            rows_moved,
            columns_moved,
            merges_added,
            merges_removed,
            hidden_dimensions,
            drawings_added,
            drawings_removed,
            chart_changes,
            conditional_formats_added,
            conditional_formats_removed,
            cells,
            truncated: sheet_truncated,
        });
    }

    Ok(WorkbookDiff {
        format: format.to_string(),
        sheets_added,
        sheets_removed,
        sheets_moved,
        sheets_renamed,
        defined_names,
        sheets,
        truncated,
    })
}

/// A name is reported as moved when it is not part of the longest common
/// subsequence of the two workbook orders. Insertions and removals are
/// already reported separately, so only common sheets reach this check.
fn detect_sheet_moves(
    left_names: &[String],
    right_names: &[String],
    common_left: &[String],
    common_right: &[String],
) -> Vec<SheetMove> {
    if common_left == common_right {
        return Vec::new();
    }
    let left_positions: BTreeMap<&str, usize> = left_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.as_str(), index + 1))
        .collect();
    let right_positions: BTreeMap<&str, usize> = right_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.as_str(), index + 1))
        .collect();
    let kept_names = lcs_names(common_left, common_right);
    let kept: BTreeSet<&str> = kept_names.iter().map(String::as_str).collect();
    common_left
        .iter()
        .filter(|name| !kept.contains(name.as_str()))
        .filter_map(|name| {
            Some(SheetMove {
                name: name.clone(),
                left_index: *left_positions.get(name.as_str())?,
                right_index: *right_positions.get(name.as_str())?,
            })
        })
        .collect()
}

fn lcs_names(left: &[String], right: &[String]) -> Vec<String> {
    let mut table = vec![vec![0usize; right.len() + 1]; left.len() + 1];
    for left_index in (0..left.len()).rev() {
        for right_index in (0..right.len()).rev() {
            table[left_index][right_index] = if left[left_index] == right[right_index] {
                table[left_index + 1][right_index + 1] + 1
            } else {
                table[left_index + 1][right_index].max(table[left_index][right_index + 1])
            };
        }
    }
    let mut kept = Vec::new();
    let (mut left_index, mut right_index) = (0, 0);
    while left_index < left.len() && right_index < right.len() {
        if left[left_index] == right[right_index] {
            kept.push(left[left_index].clone());
            left_index += 1;
            right_index += 1;
        } else if table[left_index + 1][right_index] >= table[left_index][right_index + 1] {
            left_index += 1;
        } else {
            right_index += 1;
        }
    }
    kept
}

fn align_signatures(left: &[String], right: &[String]) -> (Vec<Option<usize>>, Vec<Option<usize>>) {
    let mut table = vec![vec![0u32; right.len() + 1]; left.len() + 1];
    for left_index in (0..left.len()).rev() {
        for right_index in (0..right.len()).rev() {
            table[left_index][right_index] = if left[left_index] == right[right_index] {
                table[left_index + 1][right_index + 1] + 1
            } else {
                table[left_index + 1][right_index].max(table[left_index][right_index + 1])
            };
        }
    }
    let mut left_match = vec![None; left.len()];
    let mut right_match = vec![None; right.len()];
    let (mut left_index, mut right_index) = (0, 0);
    while left_index < left.len() && right_index < right.len() {
        if left[left_index] == right[right_index] {
            left_match[left_index] = Some(right_index);
            right_match[right_index] = Some(left_index);
            left_index += 1;
            right_index += 1;
        } else if table[left_index + 1][right_index] >= table[left_index][right_index + 1] {
            left_index += 1;
        } else {
            right_index += 1;
        }
    }
    (left_match, right_match)
}

fn signature_has_content(signature: &str) -> bool {
    signature
        .chars()
        .any(|character| character != '\u{1f}' && character != '\u{1e}')
}

fn compare_sheet_cells(
    left: &Sheet,
    right: &Sheet,
    compare_formulas: bool,
    max_cells: usize,
    ignored: &BTreeSet<String>,
) -> (
    Vec<CellChange>,
    Vec<crate::model::SheetRowMove>,
    Vec<crate::model::SheetColumnMove>,
    bool,
) {
    let left_signatures: Vec<String> = (0..left.rows)
        .map(|row| sheet_row_signature(left, row, compare_formulas, ignored))
        .collect();
    let right_signatures: Vec<String> = (0..right.rows)
        .map(|row| sheet_row_signature(right, row, compare_formulas, ignored))
        .collect();
    if left_signatures.len().saturating_mul(right_signatures.len()) > 4_000_000 {
        let (cells, truncated) =
            compare_sheet_cells_positionally(left, right, compare_formulas, max_cells, ignored);
        return (cells, Vec::new(), Vec::new(), truncated);
    }
    let (mut left_match, mut right_match) = align_signatures(&left_signatures, &right_signatures);

    // Rows whose content is unchanged but whose position changed are moves,
    // not a long run of changed cells. Match them before pairing the remaining
    // edited rows positionally.
    let mut rows_moved = Vec::new();
    for candidate in 0..left_signatures.len() {
        if left_match[candidate].is_some() || !signature_has_content(&left_signatures[candidate]) {
            continue;
        }
        if let Some(target) = (0..right_signatures.len()).find(|target| {
            right_match[*target].is_none()
                && right_signatures[*target] == left_signatures[candidate]
        }) {
            left_match[candidate] = Some(target);
            right_match[target] = Some(candidate);
            if candidate != target {
                rows_moved.push(crate::model::SheetRowMove {
                    left_row: candidate as u32 + 1,
                    right_row: target as u32 + 1,
                });
            }
        }
    }
    let unmatched_left: Vec<usize> = (0..left_signatures.len())
        .filter(|index| left_match[*index].is_none())
        .collect();
    let unmatched_right: Vec<usize> = (0..right_signatures.len())
        .filter(|index| right_match[*index].is_none())
        .collect();
    for (left_row, right_row) in unmatched_left.iter().zip(unmatched_right.iter()) {
        left_match[*left_row] = Some(*right_row);
        right_match[*right_row] = Some(*left_row);
    }

    // Column alignment mirrors rows: keep columns in order where possible,
    // then pair moved columns by content, and finally pair edited columns
    // positionally. This avoids reporting every cell in a moved column.
    let (left_column_match, right_column_match, columns_moved) =
        align_sheet_columns(left, right, compare_formulas, ignored);

    let mut cells = Vec::new();
    let mut truncated = false;
    for (right_row, matched) in right_match.iter().enumerate() {
        match matched {
            Some(left_row) => {
                for right_column in 0..right.columns {
                    if column_is_ignored(right_column, ignored) {
                        continue;
                    }
                    match right_column_match[right_column as usize] {
                        Some(left_column) => push_sheet_cell(
                            &mut cells,
                            &mut truncated,
                            max_cells,
                            changed_sheet_cell_at(
                                left,
                                right,
                                *left_row as u32,
                                right_row as u32,
                                left_column as u32,
                                right_column,
                                compare_formulas,
                            ),
                        ),
                        None => push_sheet_cell(
                            &mut cells,
                            &mut truncated,
                            max_cells,
                            CellChange {
                                column: cell_address(right_row as u32, right_column),
                                left: None,
                                right: right.value(right_row as u32, right_column),
                                left_formula: None,
                                right_formula: compare_formulas
                                    .then(|| right.formula(right_row as u32, right_column))
                                    .flatten(),
                            },
                        ),
                    }
                }
            }
            None => {
                for column in 0..right.columns {
                    if column_is_ignored(column, ignored) {
                        continue;
                    }
                    push_sheet_cell(
                        &mut cells,
                        &mut truncated,
                        max_cells,
                        CellChange {
                            column: cell_address(right_row as u32, column),
                            left: None,
                            right: right.value(right_row as u32, column),
                            left_formula: None,
                            right_formula: compare_formulas
                                .then(|| right.formula(right_row as u32, column))
                                .flatten(),
                        },
                    );
                }
            }
        }
    }
    for (left_row, matched) in left_match.iter().enumerate() {
        for left_column in 0..left.columns {
            if column_is_ignored(left_column, ignored) {
                continue;
            }
            let removed = match matched {
                None => true,
                Some(_) => left_column_match[left_column as usize].is_none(),
            };
            if !removed {
                continue;
            }
            push_sheet_cell(
                &mut cells,
                &mut truncated,
                max_cells,
                CellChange {
                    column: cell_address(left_row as u32, left_column),
                    left: left.value(left_row as u32, left_column),
                    right: None,
                    left_formula: compare_formulas
                        .then(|| left.formula(left_row as u32, left_column))
                        .flatten(),
                    right_formula: None,
                },
            );
        }
    }
    (cells, rows_moved, columns_moved, truncated)
}

fn align_sheet_columns(
    left: &Sheet,
    right: &Sheet,
    compare_formulas: bool,
    ignored: &BTreeSet<String>,
) -> (
    Vec<Option<usize>>,
    Vec<Option<usize>>,
    Vec<crate::model::SheetColumnMove>,
) {
    // Very wide sheets fall back to positional columns; the LCS table is
    // quadratic and would otherwise dominate memory.
    if left.columns.saturating_mul(right.columns) > 4_000_000 {
        return (
            (0..left.columns)
                .map(|column| (column < right.columns).then_some(column as usize))
                .collect(),
            (0..right.columns)
                .map(|column| (column < left.columns).then_some(column as usize))
                .collect(),
            Vec::new(),
        );
    }
    let left_signatures: Vec<String> = (0..left.columns)
        .map(|column| sheet_column_signature(left, column, compare_formulas, ignored))
        .collect();
    let right_signatures: Vec<String> = (0..right.columns)
        .map(|column| sheet_column_signature(right, column, compare_formulas, ignored))
        .collect();
    let (mut left_match, mut right_match) = align_signatures(&left_signatures, &right_signatures);
    let mut moved = Vec::new();
    for candidate in 0..left_signatures.len() {
        if left_match[candidate].is_some() || !signature_has_content(&left_signatures[candidate]) {
            continue;
        }
        if let Some(target) = (0..right_signatures.len()).find(|target| {
            right_match[*target].is_none()
                && right_signatures[*target] == left_signatures[candidate]
        }) {
            left_match[candidate] = Some(target);
            right_match[target] = Some(candidate);
            if candidate != target {
                moved.push(crate::model::SheetColumnMove {
                    left_column: column_name(candidate as u32),
                    right_column: column_name(target as u32),
                });
            }
        }
    }
    let unmatched_left: Vec<usize> = (0..left_signatures.len())
        .filter(|index| left_match[*index].is_none())
        .collect();
    let unmatched_right: Vec<usize> = (0..right_signatures.len())
        .filter(|index| right_match[*index].is_none())
        .collect();
    for (left_column, right_column) in unmatched_left.iter().zip(unmatched_right.iter()) {
        left_match[*left_column] = Some(*right_column);
        right_match[*right_column] = Some(*left_column);
    }
    (left_match, right_match, moved)
}

fn sheet_column_signature(
    sheet: &Sheet,
    column: u32,
    compare_formulas: bool,
    ignored: &BTreeSet<String>,
) -> String {
    if column_is_ignored(column, ignored) {
        return String::new();
    }
    let mut value = String::new();
    for row in 0..sheet.rows {
        value.push_str(&sheet.value(row, column).unwrap_or_default());
        value.push('\u{1f}');
        if compare_formulas {
            value.push_str(&normalize_formula(
                &sheet.formula(row, column).unwrap_or_default(),
            ));
        }
        value.push('\u{1e}');
    }
    value
}

fn compare_sheet_cells_positionally(
    left: &Sheet,
    right: &Sheet,
    compare_formulas: bool,
    max_cells: usize,
    ignored: &BTreeSet<String>,
) -> (Vec<CellChange>, bool) {
    let mut cells = Vec::new();
    let mut truncated = false;
    let rows = left.rows.max(right.rows);
    let columns = left.columns.max(right.columns);
    for row in 0..rows {
        for column in 0..columns {
            if column_is_ignored(column, ignored) {
                continue;
            }
            let cell = match (row < left.rows, row < right.rows) {
                (true, true) => changed_sheet_cell(left, right, row, row, column, compare_formulas),
                (true, false) => CellChange {
                    column: cell_address(row, column),
                    left: left.value(row, column),
                    right: None,
                    left_formula: compare_formulas
                        .then(|| left.formula(row, column))
                        .flatten(),
                    right_formula: None,
                },
                (false, true) => CellChange {
                    column: cell_address(row, column),
                    left: None,
                    right: right.value(row, column),
                    left_formula: None,
                    right_formula: compare_formulas
                        .then(|| right.formula(row, column))
                        .flatten(),
                },
                (false, false) => continue,
            };
            push_sheet_cell(&mut cells, &mut truncated, max_cells, cell);
        }
    }
    (cells, truncated)
}

fn changed_sheet_cell(
    left: &Sheet,
    right: &Sheet,
    left_row: u32,
    right_row: u32,
    column: u32,
    compare_formulas: bool,
) -> CellChange {
    changed_sheet_cell_at(
        left,
        right,
        left_row,
        right_row,
        column,
        column,
        compare_formulas,
    )
}

#[allow(clippy::too_many_arguments)]
fn changed_sheet_cell_at(
    left: &Sheet,
    right: &Sheet,
    left_row: u32,
    right_row: u32,
    left_column: u32,
    right_column: u32,
    compare_formulas: bool,
) -> CellChange {
    let left_value = left.value(left_row, left_column);
    let right_value = right.value(right_row, right_column);
    let left_formula = compare_formulas
        .then(|| left.formula(left_row, left_column))
        .flatten();
    let right_formula = compare_formulas
        .then(|| right.formula(right_row, right_column))
        .flatten();
    CellChange {
        column: cell_address(right_row, right_column),
        left: left_value,
        right: right_value,
        left_formula,
        right_formula,
    }
}

fn push_sheet_cell(
    cells: &mut Vec<CellChange>,
    truncated: &mut bool,
    max_cells: usize,
    cell: CellChange,
) {
    if cell.left == cell.right && cell.left_formula == cell.right_formula {
        return;
    }
    if cells.len() < max_cells {
        cells.push(cell);
    } else {
        *truncated = true;
    }
}

fn sheet_row_signature(
    sheet: &Sheet,
    row: u32,
    compare_formulas: bool,
    ignored: &BTreeSet<String>,
) -> String {
    let mut value = String::new();
    for column in 0..sheet.columns {
        if column_is_ignored(column, ignored) {
            continue;
        }
        value.push_str(&sheet.value(row, column).unwrap_or_default());
        value.push('\u{1f}');
        if compare_formulas {
            value.push_str(&normalize_formula(
                &sheet.formula(row, column).unwrap_or_default(),
            ));
        }
        value.push('\u{1e}');
    }
    value
}

fn normalize_formula(formula: &str) -> String {
    formula
        .chars()
        .filter(|character| !character.is_whitespace())
        .flat_map(char::to_uppercase)
        .collect()
}

fn column_is_ignored(column: u32, ignored: &BTreeSet<String>) -> bool {
    ignored.contains(&column_name(column))
}

#[derive(Debug, Clone, Default)]
struct SheetMeta {
    merges: BTreeSet<String>,
    hidden_rows: BTreeSet<u32>,
    hidden_columns: BTreeSet<u32>,
    drawings: BTreeSet<String>,
    charts: BTreeMap<String, String>,
    conditional_formats: BTreeSet<String>,
}

#[derive(Debug, Default)]
struct XlsxMetadata {
    defined_names: BTreeMap<String, String>,
    sheets: BTreeMap<String, SheetMeta>,
}

fn read_xlsx_metadata(package: &Package<'_>) -> std::result::Result<XlsxMetadata, String> {
    if !package.contains("xl/workbook.xml") {
        return Err("xlsx: missing xl/workbook.xml".to_string());
    }
    let workbook_xml = package.read_text("xl/workbook.xml")?;
    let mut metadata = XlsxMetadata::default();
    let mut sheet_refs: Vec<(String, String)> = Vec::new();
    let mut reader = XmlReader::from_str(&workbook_xml);
    reader.config_mut().trim_text(false);
    let mut current_defined_name: Option<String> = None;
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                match event.local_name().as_ref() {
                    "sheet" => {
                        let name = attribute_value(&event, "name").unwrap_or_default();
                        let relationship = attribute_value(&event, "id").unwrap_or_default();
                        if !name.is_empty() && !relationship.is_empty() {
                            sheet_refs.push((name, relationship));
                        }
                    }
                    "definedName" => {
                        if let Some(name) = attribute_value(&event, "name") {
                            current_defined_name = Some(name);
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(event)) => {
                if let Some(name) = &current_defined_name {
                    let value = event.xml_content(XmlVersion::Implicit1_0);
                    metadata
                        .defined_names
                        .entry(name.clone())
                        .or_default()
                        .push_str(&value);
                }
            }
            Ok(Event::End(event)) => {
                if event.local_name().as_ref() == "definedName" {
                    current_defined_name = None;
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(error.to_string()),
        }
    }

    let mut rels = BTreeMap::new();
    if package.contains("xl/_rels/workbook.xml.rels") {
        let rels_xml = package.read_text("xl/_rels/workbook.xml.rels")?;
        let mut reader = XmlReader::from_str(&rels_xml);
        reader.config_mut().trim_text(true);
        loop {
            match reader.read_event() {
                Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                    if event.local_name().as_ref() != "Relationship" {
                        continue;
                    }
                    let id = attribute_value(&event, "Id").unwrap_or_default();
                    let target = attribute_value(&event, "Target").unwrap_or_default();
                    let external = attribute_value(&event, "TargetMode")
                        .is_some_and(|mode| mode.eq_ignore_ascii_case("External"));
                    if !id.is_empty() && !target.is_empty() && !external {
                        rels.insert(id, resolve_part_path("xl", &target));
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(error) => return Err(error.to_string()),
            }
        }
    }

    let dxfs = read_dxfs(package);
    for (sheet_name, relationship) in sheet_refs {
        let Some(part) = rels.get(&relationship) else {
            continue;
        };
        let Ok(sheet_xml) = package.read_text(part) else {
            continue;
        };
        let mut sheet = SheetMeta::default();
        let mut reader = XmlReader::from_str(&sheet_xml);
        reader.config_mut().trim_text(true);
        loop {
            match reader.read_event() {
                Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                    match event.local_name().as_ref() {
                        "mergeCell" => {
                            if let Some(reference) = attribute_value(&event, "ref") {
                                sheet.merges.insert(reference);
                            }
                        }
                        "row" => {
                            if let Some(r) = attribute_value(&event, "r")
                                && attribute_value(&event, "hidden")
                                    .is_some_and(|value| matches!(value.as_str(), "1" | "true"))
                                && let Ok(row) = r.parse::<u32>()
                            {
                                sheet.hidden_rows.insert(row);
                            }
                        }
                        "col" => {
                            let hidden = attribute_value(&event, "hidden")
                                .is_some_and(|value| matches!(value.as_str(), "1" | "true"));
                            if hidden {
                                let min = attribute_value(&event, "min")
                                    .and_then(|value| value.parse::<u32>().ok())
                                    .unwrap_or(1);
                                let max = attribute_value(&event, "max")
                                    .and_then(|value| value.parse::<u32>().ok())
                                    .unwrap_or(min);
                                for column in min..=max.min(min.saturating_add(16_384)) {
                                    sheet.hidden_columns.insert(column);
                                }
                            }
                        }
                        "drawing" => {
                            if let Some(id) = attribute_value(&event, "id") {
                                sheet.drawings.insert(id);
                            }
                        }
                        _ => {}
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(error) => return Err(error.to_string()),
            }
        }
        sheet.charts = read_sheet_charts(package, part, &sheet.drawings);
        sheet.conditional_formats = read_conditional_formats(&sheet_xml, &dxfs);
        metadata.sheets.insert(sheet_name, sheet);
    }
    Ok(metadata)
}

fn read_dxfs(package: &Package<'_>) -> Vec<String> {
    let Ok(xml) = package.read_text("xl/styles.xml") else {
        return Vec::new();
    };
    let mut dxfs = Vec::new();
    let mut in_dxfs = false;
    let mut current: Option<Vec<String>> = None;
    let mut reader = XmlReader::from_str(&xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => match event.local_name().as_ref() {
                "dxfs" => in_dxfs = true,
                "dxf" if in_dxfs => current = Some(Vec::new()),
                local if current.is_some() => {
                    if let Some(tokens) = &mut current {
                        tokens.push(dxf_token(local, &event));
                    }
                }
                _ => {}
            },
            Ok(Event::Empty(event)) => match event.local_name().as_ref() {
                "dxf" if in_dxfs => dxfs.push(String::new()),
                local if current.is_some() => {
                    if let Some(tokens) = &mut current {
                        tokens.push(dxf_token(local, &event));
                    }
                }
                _ => {}
            },
            Ok(Event::End(event)) => match event.local_name().as_ref() {
                "dxf" if in_dxfs => {
                    if let Some(tokens) = current.take() {
                        dxfs.push(tokens.join("|"));
                    }
                }
                "dxfs" => in_dxfs = false,
                _ => {}
            },
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    dxfs
}

fn dxf_token(local: &str, event: &quick_xml::events::BytesStart<'_>) -> String {
    let mut attributes = Vec::new();
    for attribute in event.attributes().with_checks(false).flatten() {
        let key = attribute.key.as_ref();
        if key.starts_with("xmlns") {
            continue;
        }
        let value = attribute
            .normalized_value(XmlVersion::Implicit1_0)
            .unwrap_or_default()
            .into_owned();
        attributes.push(format!("{key}={value}"));
    }
    attributes.sort();
    if attributes.is_empty() {
        local.to_string()
    } else {
        format!("{local}({})", attributes.join(","))
    }
}

fn read_conditional_formats(xml: &str, dxfs: &[String]) -> BTreeSet<String> {
    let mut formats = BTreeSet::new();
    let mut sqref = String::new();
    let mut rule: Option<String> = None;
    let mut formulas: Vec<String> = Vec::new();
    let mut in_formula = false;
    let mut formula_text = String::new();
    let mut reader = XmlReader::from_str(xml);
    reader.config_mut().trim_text(false);
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => match event.local_name().as_ref() {
                "conditionalFormatting" => {
                    sqref = attribute_value(&event, "sqref").unwrap_or_default();
                }
                "cfRule" => {
                    rule = Some(conditional_rule_string(&event, dxfs));
                    formulas.clear();
                }
                "formula" if rule.is_some() => {
                    in_formula = true;
                    formula_text.clear();
                }
                _ => {}
            },
            Ok(Event::Empty(event)) => {
                if event.local_name().as_ref() == "cfRule" {
                    let entry = format!("{sqref}|{}", conditional_rule_string(&event, dxfs));
                    formats.insert(entry);
                }
            }
            Ok(Event::Text(event)) if in_formula => {
                formula_text.push_str(&event.xml_content(XmlVersion::Implicit1_0));
            }
            Ok(Event::End(event)) => match event.local_name().as_ref() {
                "formula" if in_formula => {
                    let formula = formula_text.trim();
                    if !formula.is_empty() {
                        formulas.push(formula.to_string());
                    }
                    in_formula = false;
                }
                "cfRule" => {
                    if let Some(rule) = rule.take() {
                        let formulas = if formulas.is_empty() {
                            String::new()
                        } else {
                            format!("|formula={}", formulas.join(";"))
                        };
                        formats.insert(format!("{sqref}|{rule}{formulas}"));
                    }
                }
                "conditionalFormatting" => sqref.clear(),
                _ => {}
            },
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    formats
}

fn conditional_rule_string(event: &quick_xml::events::BytesStart<'_>, dxfs: &[String]) -> String {
    let mut parts = Vec::new();
    for key in [
        "type",
        "operator",
        "priority",
        "dxfId",
        "text",
        "timePeriod",
        "rank",
        "stdDev",
        "stopIfTrue",
        "aboveAverage",
        "bottom",
        "percent",
        "equalAverage",
    ] {
        if let Some(value) = attribute_value(event, key) {
            parts.push(format!("{key}={value}"));
            if key == "dxfId"
                && let Ok(index) = value.parse::<usize>()
                && let Some(dxf) = dxfs.get(index)
            {
                parts.push(format!("dxf={dxf}"));
            }
        }
    }
    parts.join(",")
}

fn read_sheet_charts(
    package: &Package<'_>,
    sheet_part: &str,
    drawings: &BTreeSet<String>,
) -> BTreeMap<String, String> {
    let mut charts = BTreeMap::new();
    if drawings.is_empty() {
        return charts;
    }
    let file = sheet_part.rsplit('/').next().unwrap_or(sheet_part);
    let directory = sheet_part
        .rsplit_once('/')
        .map(|(directory, _)| directory)
        .unwrap_or("xl/worksheets");
    let rels_name = format!("{directory}/_rels/{file}.rels");
    let rels = read_relationships(package, &rels_name, directory);
    for drawing_id in drawings {
        let Some(drawing_part) = rels.get(drawing_id) else {
            continue;
        };
        let Ok(drawing_xml) = package.read_text(drawing_part) else {
            continue;
        };
        let base = drawing_part
            .rsplit_once('/')
            .map(|(directory, _)| directory)
            .unwrap_or("xl/drawings");
        let drawing_file = drawing_part
            .rsplit('/')
            .next()
            .unwrap_or(drawing_part.as_str());
        let drawing_rels_name = format!("{base}/_rels/{drawing_file}.rels");
        let chart_rels = read_relationships(package, &drawing_rels_name, base);
        let mut reader = XmlReader::from_str(&drawing_xml);
        reader.config_mut().trim_text(true);
        loop {
            match reader.read_event() {
                Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                    if event.local_name().as_ref() != "chart" {
                        continue;
                    }
                    let Some(id) = attribute_value(&event, "id") else {
                        continue;
                    };
                    let Some(target) = chart_rels.get(&id) else {
                        continue;
                    };
                    let chart_part = resolve_part_path(base, target);
                    if let Ok(chart_xml) = package.read_text(&chart_part) {
                        charts.insert(chart_part, hex_digest(chart_xml.as_bytes()));
                    }
                }
                Ok(Event::Eof) => break,
                Ok(_) => {}
                Err(_) => break,
            }
        }
    }
    charts
}

fn read_relationships(
    package: &Package<'_>,
    rels_name: &str,
    base: &str,
) -> BTreeMap<String, String> {
    let Ok(xml) = package.read_text(rels_name) else {
        return BTreeMap::new();
    };
    let mut rels = BTreeMap::new();
    let mut reader = XmlReader::from_str(&xml);
    reader.config_mut().trim_text(true);
    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) | Ok(Event::Empty(event)) => {
                if event.local_name().as_ref() != "Relationship" {
                    continue;
                }
                let id = attribute_value(&event, "Id").unwrap_or_default();
                let target = attribute_value(&event, "Target").unwrap_or_default();
                let external = attribute_value(&event, "TargetMode")
                    .is_some_and(|mode| mode.eq_ignore_ascii_case("External"));
                if !id.is_empty() && !target.is_empty() && !external {
                    rels.insert(id, resolve_part_path(base, &target));
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(_) => return rels,
        }
    }
    rels
}
fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn attribute_value(event: &quick_xml::events::BytesStart<'_>, local: &str) -> Option<String> {
    for attribute in event.attributes().with_checks(false).flatten() {
        let key = attribute.key.as_ref();
        if key == local || key.ends_with(&format!(":{local}")) {
            return attribute
                .normalized_value(XmlVersion::Implicit1_0)
                .ok()
                .map(|value| value.into_owned());
        }
    }
    None
}

fn resolve_part_path(base: &str, target: &str) -> String {
    let absolute = target.starts_with('/');
    let mut parts: Vec<&str> = if absolute {
        Vec::new()
    } else {
        base.split('/').filter(|part| !part.is_empty()).collect()
    };
    for part in target.trim_start_matches('/').split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

fn set_difference(left: &BTreeSet<String>, right: &BTreeSet<String>) -> (Vec<String>, Vec<String>) {
    let added = right.difference(left).cloned().collect();
    let removed = left.difference(right).cloned().collect();
    (added, removed)
}

fn compare_defined_names(
    left: Option<&XlsxMetadata>,
    right: Option<&XlsxMetadata>,
) -> Vec<FieldChange> {
    let empty = BTreeMap::new();
    let left = left.map(|meta| &meta.defined_names).unwrap_or(&empty);
    let right = right.map(|meta| &meta.defined_names).unwrap_or(&empty);
    let names: BTreeSet<&String> = left.keys().chain(right.keys()).collect();
    names
        .into_iter()
        .filter(|name| left.get(*name) != right.get(*name))
        .map(|name| FieldChange {
            path: format!("defined_name/{name}"),
            left: left.get(name).cloned(),
            right: right.get(name).cloned(),
        })
        .collect()
}

fn compare_chart_hashes(
    sheet: &str,
    left: &BTreeMap<String, String>,
    right: &BTreeMap<String, String>,
) -> Vec<FieldChange> {
    let names: BTreeSet<&String> = left.keys().chain(right.keys()).collect();
    names
        .into_iter()
        .filter(|name| left.get(*name) != right.get(*name))
        .map(|name| FieldChange {
            path: format!("{sheet}/chart[{name}]"),
            left: left
                .get(name)
                .map(|hash| hash[..hash.len().min(12)].to_string()),
            right: right
                .get(name)
                .map(|hash| hash[..hash.len().min(12)].to_string()),
        })
        .collect()
}

fn compare_hidden_dimensions(sheet: &str, left: &SheetMeta, right: &SheetMeta) -> Vec<FieldChange> {
    let mut changes = Vec::new();
    let rows: BTreeSet<&u32> = left
        .hidden_rows
        .iter()
        .chain(right.hidden_rows.iter())
        .collect();
    for row in rows {
        let left_hidden = left.hidden_rows.contains(row);
        let right_hidden = right.hidden_rows.contains(row);
        if left_hidden != right_hidden {
            changes.push(FieldChange {
                path: format!("{sheet}/row[{row}]/hidden"),
                left: Some(left_hidden.to_string()),
                right: Some(right_hidden.to_string()),
            });
        }
    }
    let columns: BTreeSet<&u32> = left
        .hidden_columns
        .iter()
        .chain(right.hidden_columns.iter())
        .collect();
    for column in columns {
        let left_hidden = left.hidden_columns.contains(column);
        let right_hidden = right.hidden_columns.contains(column);
        if left_hidden != right_hidden {
            changes.push(FieldChange {
                path: format!("{sheet}/column[{}]/hidden", column_name(*column)),
                left: Some(left_hidden.to_string()),
                right: Some(right_hidden.to_string()),
            });
        }
    }
    changes
}

struct Sheet {
    rows: u32,
    columns: u32,
    cells: Vec<Vec<Option<String>>>,
    formulas: Vec<Vec<Option<String>>>,
    hidden: bool,
}

impl Sheet {
    fn value(&self, row: u32, column: u32) -> Option<String> {
        self.cells
            .get(row as usize)
            .and_then(|values| values.get(column as usize))
            .cloned()
            .flatten()
    }

    fn formula(&self, row: u32, column: u32) -> Option<String> {
        self.formulas
            .get(row as usize)
            .and_then(|values| values.get(column as usize))
            .cloned()
            .flatten()
    }

    fn fingerprint(&self) -> String {
        let mut value = String::new();
        for row in 0..self.rows {
            for column in 0..self.columns {
                value.push_str(&self.value(row, column).unwrap_or_default());
                value.push('\u{1f}');
                value.push_str(&self.formula(row, column).unwrap_or_default());
                value.push('\u{1e}');
            }
        }
        value
    }
}

struct Workbook {
    /// Workbook order is meaningful: Excel and LibreOffice let users reorder
    /// sheets, and that order is visible in the tab bar. Keep the parser's
    /// order instead of sorting names alphabetically.
    sheets: Vec<(String, Sheet)>,
}

impl Workbook {
    fn names(&self) -> Vec<String> {
        self.sheets.iter().map(|(name, _)| name.clone()).collect()
    }

    fn sheet(&self, name: &str) -> Option<&Sheet> {
        self.sheets
            .iter()
            .find_map(|(sheet_name, sheet)| (sheet_name == name).then_some(sheet))
    }
}

fn read_workbook(bytes: &[u8]) -> std::result::Result<Workbook, String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut workbook = open_workbook_auto_from_rs(cursor).map_err(|error| error.to_string())?;
    let names = workbook.sheet_names().to_owned();
    let visibility: BTreeMap<String, bool> = workbook
        .sheets_metadata()
        .iter()
        .map(|sheet| {
            (
                sheet.name.clone(),
                !matches!(sheet.visible, SheetVisible::Visible),
            )
        })
        .collect();
    let mut sheets = Vec::with_capacity(names.len());

    for name in names {
        let range = match workbook.worksheet_range(&name) {
            Ok(range) => range,
            Err(error) => return Err(format!("sheet {name}: {error}")),
        };
        let formula_range = workbook.worksheet_formula(&name).ok();
        let (rows, columns) = range.get_size();
        let mut cells = Vec::with_capacity(rows);
        let mut formulas = Vec::with_capacity(rows);
        for row in 0..rows {
            let mut values = Vec::with_capacity(columns);
            let mut formula_values = Vec::with_capacity(columns);
            for column in 0..columns {
                let value = range
                    .get_value((row as u32, column as u32))
                    .map(data_to_string)
                    .filter(|value| !value.is_empty());
                values.push(value);
                let formula = formula_range
                    .as_ref()
                    .and_then(|range| range.get_value((row as u32, column as u32)))
                    .cloned()
                    .filter(|value| !value.is_empty());
                formula_values.push(formula);
            }
            cells.push(values);
            formulas.push(formula_values);
        }
        sheets.push((
            name.clone(),
            Sheet {
                rows: rows as u32,
                columns: columns as u32,
                cells,
                formulas,
                hidden: visibility.get(&name).copied().unwrap_or(false),
            },
        ));
    }

    Ok(Workbook { sheets })
}

fn data_to_string(data: &Data) -> String {
    match data {
        Data::Empty => String::new(),
        Data::String(value) => value.clone(),
        Data::Float(value) => {
            if value.fract() == 0.0 {
                format!("{value:.0}")
            } else {
                value.to_string()
            }
        }
        Data::Int(value) => value.to_string(),
        Data::Bool(value) => value.to_string(),
        Data::DateTime(value) => format!("{value:?}"),
        Data::DateTimeIso(value) => value.clone(),
        Data::DurationIso(value) => value.clone(),
        Data::Error(value) => format!("#{value:?}"),
    }
}

fn cell_address(row: u32, column: u32) -> String {
    format!("{}{}", column_name(column), row + 1)
}

fn column_name(mut column: u32) -> String {
    let mut chars = Vec::new();
    loop {
        chars.push((b'A' + (column % 26) as u8) as char);
        if column < 26 {
            break;
        }
        column = column / 26 - 1;
    }
    chars.iter().rev().collect()
}

#[cfg(test)]
#[path = "../../tests/unit/diff/workbook.rs"]
mod tests;
