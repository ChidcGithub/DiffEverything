use crate::diff::text::decode_text;
use crate::model::{CellChange, ColumnMove, RowChangeKind, TableDiff, TableRowChange};
use std::collections::{BTreeMap, BTreeSet};

struct ParsedTable {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    column_index: BTreeMap<String, usize>,
    delimiter: u8,
}

#[derive(Debug, Clone, Default)]
pub struct TableCompareOptions<'a> {
    pub key_column: Option<&'a str>,
    pub ignore_columns: &'a [String],
    pub numeric_tolerance: f64,
}

pub fn compare_tables(
    left: &[u8],
    right: &[u8],
    format: &str,
    key_column: Option<&str>,
    max_rows: usize,
) -> std::result::Result<TableDiff, String> {
    compare_tables_with_options(
        left,
        right,
        format,
        &TableCompareOptions {
            key_column,
            ..TableCompareOptions::default()
        },
        max_rows,
    )
}

pub fn compare_tables_with_options(
    left: &[u8],
    right: &[u8],
    format: &str,
    options: &TableCompareOptions<'_>,
    max_rows: usize,
) -> std::result::Result<TableDiff, String> {
    let left_table = parse_table(left, format)?;
    let right_table = parse_table(right, format)?;
    let ignored: BTreeSet<&str> = options.ignore_columns.iter().map(String::as_str).collect();

    let left_headers: BTreeSet<&String> = left_table.headers.iter().collect();
    let right_headers: BTreeSet<&String> = right_table.headers.iter().collect();
    let columns_added: Vec<String> = right_headers
        .difference(&left_headers)
        .filter(|header| !ignored.contains(header.as_str()))
        .map(|header| (*header).clone())
        .collect();
    let columns_removed: Vec<String> = left_headers
        .difference(&right_headers)
        .filter(|header| !ignored.contains(header.as_str()))
        .map(|header| (*header).clone())
        .collect();
    let common_columns: Vec<String> = left_table
        .headers
        .iter()
        .filter(|header| {
            right_table.column_index.contains_key(*header) && !ignored.contains(header.as_str())
        })
        .cloned()
        .collect();
    let columns_moved = detect_column_moves(&left_table.headers, &right_table.headers, &ignored);

    let mut rows = Vec::new();
    let mut truncated = false;
    let mut duplicate_keys = Vec::new();
    let key_indexes = options.key_column.and_then(|key| {
        left_table
            .column_index
            .get(key)
            .zip(right_table.column_index.get(key))
    });

    if let Some((left_key_index, right_key_index)) = key_indexes {
        // Keep every occurrence instead of letting a duplicate key overwrite
        // the previous row. Occurrences are paired by position and reported
        // with a `#n` suffix when there is more than one.
        let mut left_rows: BTreeMap<String, Vec<&Vec<String>>> = BTreeMap::new();
        let mut right_rows: BTreeMap<String, Vec<&Vec<String>>> = BTreeMap::new();
        for (index, row) in left_table.rows.iter().enumerate() {
            let key = row_key(row, *left_key_index, index);
            left_rows.entry(key).or_default().push(row);
        }
        for (index, row) in right_table.rows.iter().enumerate() {
            let key = row_key(row, *right_key_index, index);
            right_rows.entry(key).or_default().push(row);
        }
        let keys: BTreeSet<&String> = left_rows.keys().chain(right_rows.keys()).collect();
        for key in keys {
            let left_group = left_rows.get(key).map(Vec::as_slice).unwrap_or(&[]);
            let right_group = right_rows.get(key).map(Vec::as_slice).unwrap_or(&[]);
            if left_group.len() > 1 || right_group.len() > 1 {
                duplicate_keys.push(key.clone());
            }
            let occurrences = left_group.len().max(right_group.len());
            for occurrence in 0..occurrences {
                let display_key = if occurrence == 0 {
                    key.clone()
                } else {
                    format!("{key}#{}", occurrence + 1)
                };
                match (
                    left_group.get(occurrence).copied(),
                    right_group.get(occurrence).copied(),
                ) {
                    (Some(left_row), Some(right_row)) => {
                        let cells = changed_cells(
                            left_row,
                            right_row,
                            &common_columns,
                            &left_table.column_index,
                            &right_table.column_index,
                            options.numeric_tolerance,
                        );
                        if !cells.is_empty() {
                            push_row(
                                &mut rows,
                                &mut truncated,
                                max_rows,
                                TableRowChange {
                                    key: display_key,
                                    kind: RowChangeKind::Changed,
                                    cells,
                                },
                            );
                        }
                    }
                    (Some(left_row), None) => push_row(
                        &mut rows,
                        &mut truncated,
                        max_rows,
                        TableRowChange {
                            key: display_key,
                            kind: RowChangeKind::Removed,
                            cells: row_cells(left_row, &left_table.headers, true, &ignored),
                        },
                    ),
                    (None, Some(right_row)) => push_row(
                        &mut rows,
                        &mut truncated,
                        max_rows,
                        TableRowChange {
                            key: display_key,
                            kind: RowChangeKind::Added,
                            cells: row_cells(right_row, &right_table.headers, false, &ignored),
                        },
                    ),
                    (None, None) => {}
                }
            }
        }
    } else {
        compare_unaligned_rows(
            &left_table,
            &right_table,
            &common_columns,
            &ignored,
            options.numeric_tolerance,
            max_rows,
            &mut rows,
            &mut truncated,
        );
    }

    Ok(TableDiff {
        format: format.to_string(),
        delimiter: left_table.delimiter as char,
        left_rows: left_table.rows.len(),
        right_rows: right_table.rows.len(),
        columns_added,
        columns_removed,
        columns_moved,
        rows,
        duplicate_keys,
        truncated,
    })
}

fn detect_column_moves(
    left_headers: &[String],
    right_headers: &[String],
    ignored: &BTreeSet<&str>,
) -> Vec<ColumnMove> {
    let right_set: BTreeSet<&str> = right_headers.iter().map(String::as_str).collect();
    let left_set: BTreeSet<&str> = left_headers.iter().map(String::as_str).collect();
    let common_left: Vec<String> = left_headers
        .iter()
        .filter(|header| right_set.contains(header.as_str()) && !ignored.contains(header.as_str()))
        .cloned()
        .collect();
    let common_right: Vec<String> = right_headers
        .iter()
        .filter(|header| left_set.contains(header.as_str()) && !ignored.contains(header.as_str()))
        .cloned()
        .collect();
    if common_left == common_right {
        return Vec::new();
    }
    let kept_names = lcs_names(&common_left, &common_right);
    let kept: BTreeSet<&str> = kept_names.iter().map(String::as_str).collect();
    let left_positions: BTreeMap<&str, usize> = left_headers
        .iter()
        .enumerate()
        .map(|(index, header)| (header.as_str(), index + 1))
        .collect();
    let right_positions: BTreeMap<&str, usize> = right_headers
        .iter()
        .enumerate()
        .map(|(index, header)| (header.as_str(), index + 1))
        .collect();
    common_left
        .iter()
        .filter(|header| !kept.contains(header.as_str()))
        .filter_map(|header| {
            Some(ColumnMove {
                name: header.clone(),
                left_index: *left_positions.get(header.as_str())?,
                right_index: *right_positions.get(header.as_str())?,
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

#[allow(clippy::too_many_arguments)]
fn compare_unaligned_rows(
    left_table: &ParsedTable,
    right_table: &ParsedTable,
    common_columns: &[String],
    ignored: &BTreeSet<&str>,
    tolerance: f64,
    max_rows: usize,
    rows: &mut Vec<TableRowChange>,
    truncated: &mut bool,
) {
    let left_signatures: Vec<String> = left_table
        .rows
        .iter()
        .map(|row| row_signature(row))
        .collect();
    let right_signatures: Vec<String> = right_table
        .rows
        .iter()
        .map(|row| row_signature(row))
        .collect();
    if left_signatures.len().saturating_mul(right_signatures.len()) > 4_000_000 {
        compare_rows_positionally(
            left_table,
            right_table,
            common_columns,
            ignored,
            tolerance,
            max_rows,
            rows,
            truncated,
        );
        return;
    }
    let mut table = vec![vec![0u32; right_signatures.len() + 1]; left_signatures.len() + 1];
    for left_index in (0..left_signatures.len()).rev() {
        for right_index in (0..right_signatures.len()).rev() {
            table[left_index][right_index] =
                if left_signatures[left_index] == right_signatures[right_index] {
                    table[left_index + 1][right_index + 1] + 1
                } else {
                    table[left_index + 1][right_index].max(table[left_index][right_index + 1])
                };
        }
    }
    let mut left_match = vec![None; left_signatures.len()];
    let mut right_match = vec![None; right_signatures.len()];
    let mut moved = BTreeSet::new();
    let (mut left_index, mut right_index) = (0, 0);
    while left_index < left_signatures.len() && right_index < right_signatures.len() {
        if left_signatures[left_index] == right_signatures[right_index] {
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
    for candidate in 0..left_signatures.len() {
        if left_match[candidate].is_some() {
            continue;
        }
        if let Some(target) = (0..right_signatures.len()).find(|target| {
            right_match[*target].is_none()
                && right_signatures[*target] == left_signatures[candidate]
        }) {
            left_match[candidate] = Some(target);
            right_match[target] = Some(candidate);
            moved.insert(candidate);
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

    for (right_row, matched) in right_match.iter().enumerate() {
        let Some(left_row) = matched else {
            push_row(
                rows,
                truncated,
                max_rows,
                TableRowChange {
                    key: format!("row {}", right_row + 1),
                    kind: RowChangeKind::Added,
                    cells: row_cells(
                        &right_table.rows[right_row],
                        &right_table.headers,
                        false,
                        ignored,
                    ),
                },
            );
            continue;
        };
        let left_row = *left_row;
        if moved.contains(&left_row) {
            push_row(
                rows,
                truncated,
                max_rows,
                TableRowChange {
                    key: format!("row {}", right_row + 1),
                    kind: RowChangeKind::Moved,
                    cells: Vec::new(),
                },
            );
            continue;
        }
        let cells = changed_cells(
            &left_table.rows[left_row],
            &right_table.rows[right_row],
            common_columns,
            &left_table.column_index,
            &right_table.column_index,
            tolerance,
        );
        if !cells.is_empty() {
            push_row(
                rows,
                truncated,
                max_rows,
                TableRowChange {
                    key: format!("row {}", right_row + 1),
                    kind: RowChangeKind::Changed,
                    cells,
                },
            );
        }
    }
    for (left_row, matched) in left_match.iter().enumerate() {
        if matched.is_none() {
            push_row(
                rows,
                truncated,
                max_rows,
                TableRowChange {
                    key: format!("row {}", left_row + 1),
                    kind: RowChangeKind::Removed,
                    cells: row_cells(
                        &left_table.rows[left_row],
                        &left_table.headers,
                        true,
                        ignored,
                    ),
                },
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn compare_rows_positionally(
    left_table: &ParsedTable,
    right_table: &ParsedTable,
    common_columns: &[String],
    ignored: &BTreeSet<&str>,
    tolerance: f64,
    max_rows: usize,
    rows: &mut Vec<TableRowChange>,
    truncated: &mut bool,
) {
    for index in 0..left_table.rows.len().max(right_table.rows.len()) {
        match (left_table.rows.get(index), right_table.rows.get(index)) {
            (Some(left_row), Some(right_row)) => {
                let cells = changed_cells(
                    left_row,
                    right_row,
                    common_columns,
                    &left_table.column_index,
                    &right_table.column_index,
                    tolerance,
                );
                if !cells.is_empty() {
                    push_row(
                        rows,
                        truncated,
                        max_rows,
                        TableRowChange {
                            key: format!("row {}", index + 1),
                            kind: RowChangeKind::Changed,
                            cells,
                        },
                    );
                }
            }
            (Some(left_row), None) => push_row(
                rows,
                truncated,
                max_rows,
                TableRowChange {
                    key: format!("row {}", index + 1),
                    kind: RowChangeKind::Removed,
                    cells: row_cells(left_row, &left_table.headers, true, ignored),
                },
            ),
            (None, Some(right_row)) => push_row(
                rows,
                truncated,
                max_rows,
                TableRowChange {
                    key: format!("row {}", index + 1),
                    kind: RowChangeKind::Added,
                    cells: row_cells(right_row, &right_table.headers, false, ignored),
                },
            ),
            (None, None) => {}
        }
    }
}

fn row_signature(row: &[String]) -> String {
    row.join("\u{1f}")
}

fn push_row(
    rows: &mut Vec<TableRowChange>,
    truncated: &mut bool,
    max_rows: usize,
    row: TableRowChange,
) {
    if rows.len() < max_rows {
        rows.push(row);
    } else {
        *truncated = true;
    }
}

fn row_key(row: &[String], index: usize, fallback: usize) -> String {
    row.get(index)
        .filter(|value| !value.is_empty())
        .cloned()
        .unwrap_or_else(|| format!("row {}", fallback + 1))
}

fn changed_cells(
    left_row: &[String],
    right_row: &[String],
    columns: &[String],
    left_index: &BTreeMap<String, usize>,
    right_index: &BTreeMap<String, usize>,
    tolerance: f64,
) -> Vec<CellChange> {
    let mut cells = Vec::new();
    for column in columns {
        let left_value = left_index
            .get(column)
            .and_then(|index| left_row.get(*index))
            .cloned()
            .unwrap_or_default();
        let right_value = right_index
            .get(column)
            .and_then(|index| right_row.get(*index))
            .cloned()
            .unwrap_or_default();
        if !values_equal(&left_value, &right_value, tolerance) {
            cells.push(CellChange {
                column: column.clone(),
                left: Some(left_value),
                right: Some(right_value),
                left_formula: None,
                right_formula: None,
            });
        }
    }
    cells
}

fn values_equal(left: &str, right: &str, tolerance: f64) -> bool {
    if left == right {
        return true;
    }
    if tolerance <= 0.0 {
        return false;
    }
    match (left.trim().parse::<f64>(), right.trim().parse::<f64>()) {
        (Ok(left), Ok(right)) => (left - right).abs() <= tolerance,
        _ => false,
    }
}

fn row_cells(
    row: &[String],
    headers: &[String],
    left: bool,
    ignored: &BTreeSet<&str>,
) -> Vec<CellChange> {
    headers
        .iter()
        .enumerate()
        .filter(|(_, header)| !ignored.contains(header.as_str()))
        .filter_map(|(index, header)| {
            let value = row.get(index)?.clone();
            Some(CellChange {
                column: header.clone(),
                left: left.then(|| value.clone()),
                right: (!left).then_some(value),
                left_formula: None,
                right_formula: None,
            })
        })
        .collect()
}

fn parse_table(bytes: &[u8], format: &str) -> std::result::Result<ParsedTable, String> {
    // Decode BOM/UTF-16/legacy encodings before handing the bytes to the CSV
    // reader; otherwise a GBK or UTF-16 file becomes invalid UTF-8 noise and
    // an UTF-8 BOM sticks to the first header.
    let decoded = decode_text(bytes);
    let delimiter = detect_delimiter(decoded.text.as_bytes(), format);
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(decoded.text.as_bytes());

    let raw_headers = reader.headers().map_err(|error| error.to_string())?;
    let headers = unique_headers(
        raw_headers
            .iter()
            .map(|header| header.trim().to_string())
            .collect(),
    );
    let column_index: BTreeMap<String, usize> = headers
        .iter()
        .enumerate()
        .map(|(index, header)| (header.clone(), index))
        .collect();

    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record.map_err(|error| error.to_string())?;
        rows.push(record.iter().map(|value| value.to_string()).collect());
    }

    Ok(ParsedTable {
        headers,
        rows,
        column_index,
        delimiter,
    })
}

fn unique_headers(headers: Vec<String>) -> Vec<String> {
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();
    headers
        .into_iter()
        .enumerate()
        .map(|(index, header)| {
            let base = if header.is_empty() {
                format!("column {}", index + 1)
            } else {
                header
            };
            let count = seen.entry(base.clone()).or_insert(0);
            *count += 1;
            if *count == 1 {
                base
            } else {
                format!("{base} #{}", *count)
            }
        })
        .collect()
}

fn detect_delimiter(bytes: &[u8], format: &str) -> u8 {
    if format == "tsv" {
        return b'\t';
    }
    let first_line = bytes
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or_default();
    let mut best = b',';
    let mut best_count = 0;
    for candidate in *b",\t;|" {
        let count = first_line.iter().filter(|byte| **byte == candidate).count();
        if count > best_count {
            best_count = count;
            best = candidate;
        }
    }
    best
}

#[cfg(test)]
#[path = "../../tests/unit/diff/table.rs"]
mod tests;
