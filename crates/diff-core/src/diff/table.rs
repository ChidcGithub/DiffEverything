use crate::model::{CellChange, RowChangeKind, TableDiff, TableRowChange};
use std::collections::{BTreeMap, BTreeSet};

struct ParsedTable {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    column_index: BTreeMap<String, usize>,
    delimiter: u8,
}

pub fn compare_tables(
    left: &[u8],
    right: &[u8],
    format: &str,
    key_column: Option<&str>,
    max_rows: usize,
) -> std::result::Result<TableDiff, String> {
    let left_table = parse_table(left, format)?;
    let right_table = parse_table(right, format)?;

    let left_headers: BTreeSet<&String> = left_table.headers.iter().collect();
    let right_headers: BTreeSet<&String> = right_table.headers.iter().collect();
    let columns_added: Vec<String> = right_headers
        .difference(&left_headers)
        .map(|header| (*header).clone())
        .collect();
    let columns_removed: Vec<String> = left_headers
        .difference(&right_headers)
        .map(|header| (*header).clone())
        .collect();
    let common_columns: Vec<String> = left_table
        .headers
        .iter()
        .filter(|header| right_table.column_index.contains_key(*header))
        .cloned()
        .collect();

    let mut rows = Vec::new();
    let mut truncated = false;
    let key_indexes = key_column.and_then(|key| {
        left_table
            .column_index
            .get(key)
            .zip(right_table.column_index.get(key))
    });

    if let Some((left_key_index, right_key_index)) = key_indexes {
        let mut left_rows: BTreeMap<String, &Vec<String>> = BTreeMap::new();
        let mut right_rows: BTreeMap<String, &Vec<String>> = BTreeMap::new();
        for (index, row) in left_table.rows.iter().enumerate() {
            let key = row_key(row, *left_key_index, index);
            left_rows.insert(key, row);
        }
        for (index, row) in right_table.rows.iter().enumerate() {
            let key = row_key(row, *right_key_index, index);
            right_rows.insert(key, row);
        }
        let keys: BTreeSet<&String> = left_rows.keys().chain(right_rows.keys()).collect();
        for key in keys {
            match (left_rows.get(key), right_rows.get(key)) {
                (Some(left_row), Some(right_row)) => {
                    let cells = changed_cells(
                        left_row,
                        right_row,
                        &common_columns,
                        &left_table.column_index,
                        &right_table.column_index,
                    );
                    if !cells.is_empty() {
                        push_row(
                            &mut rows,
                            &mut truncated,
                            max_rows,
                            TableRowChange {
                                key: key.clone(),
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
                        key: key.clone(),
                        kind: RowChangeKind::Removed,
                        cells: row_cells(left_row, &left_table.headers, true),
                    },
                ),
                (None, Some(right_row)) => push_row(
                    &mut rows,
                    &mut truncated,
                    max_rows,
                    TableRowChange {
                        key: key.clone(),
                        kind: RowChangeKind::Added,
                        cells: row_cells(right_row, &right_table.headers, false),
                    },
                ),
                (None, None) => {}
            }
        }
    } else {
        let row_count = left_table.rows.len().max(right_table.rows.len());
        for index in 0..row_count {
            let key = format!("row {}", index + 1);
            match (left_table.rows.get(index), right_table.rows.get(index)) {
                (Some(left_row), Some(right_row)) => {
                    let cells = changed_cells(
                        left_row,
                        right_row,
                        &common_columns,
                        &left_table.column_index,
                        &right_table.column_index,
                    );
                    if !cells.is_empty() {
                        push_row(
                            &mut rows,
                            &mut truncated,
                            max_rows,
                            TableRowChange {
                                key,
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
                        key,
                        kind: RowChangeKind::Removed,
                        cells: row_cells(left_row, &left_table.headers, true),
                    },
                ),
                (None, Some(right_row)) => push_row(
                    &mut rows,
                    &mut truncated,
                    max_rows,
                    TableRowChange {
                        key,
                        kind: RowChangeKind::Added,
                        cells: row_cells(right_row, &right_table.headers, false),
                    },
                ),
                (None, None) => {}
            }
        }
    }

    Ok(TableDiff {
        format: format.to_string(),
        delimiter: left_table.delimiter as char,
        left_rows: left_table.rows.len(),
        right_rows: right_table.rows.len(),
        columns_added,
        columns_removed,
        rows,
        truncated,
    })
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
        if left_value != right_value {
            cells.push(CellChange {
                column: column.clone(),
                left: Some(left_value),
                right: Some(right_value),
            });
        }
    }
    cells
}

fn row_cells(row: &[String], headers: &[String], left: bool) -> Vec<CellChange> {
    headers
        .iter()
        .enumerate()
        .filter_map(|(index, header)| {
            let value = row.get(index)?.clone();
            Some(CellChange {
                column: header.clone(),
                left: left.then(|| value.clone()),
                right: (!left).then_some(value),
            })
        })
        .collect()
}

fn parse_table(bytes: &[u8], format: &str) -> std::result::Result<ParsedTable, String> {
    let delimiter = detect_delimiter(bytes, format);
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .from_reader(bytes);

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
mod tests {
    use super::*;

    #[test]
    fn compares_csv_cells() {
        let left = b"name,age,team\nAlice,30,red\nBob,25,blue\n";
        let right = b"name,age,team\nAlice,31,red\nBob,25,blue\n";
        let diff = compare_tables(left, right, "csv", None, 100).unwrap();
        assert_eq!(diff.rows.len(), 1);
        assert_eq!(diff.rows[0].kind, RowChangeKind::Changed);
        assert_eq!(diff.rows[0].cells[0].column, "age");
        assert_eq!(diff.rows[0].cells[0].left.as_deref(), Some("30"));
        assert_eq!(diff.rows[0].cells[0].right.as_deref(), Some("31"));
    }

    #[test]
    fn reports_added_and_removed_rows() {
        let left = b"id,name\n1,Alice\n2,Bob\n";
        let right = b"id,name\n1,Alice\n3,Carol\n";
        let diff = compare_tables(left, right, "csv", Some("id"), 100).unwrap();
        assert_eq!(diff.rows.len(), 2);
        assert!(
            diff.rows
                .iter()
                .any(|row| row.key == "2" && row.kind == RowChangeKind::Removed)
        );
        assert!(
            diff.rows
                .iter()
                .any(|row| row.key == "3" && row.kind == RowChangeKind::Added)
        );
    }

    #[test]
    fn aligns_columns_by_name() {
        let left = b"id,name\n1,Alice\n";
        let right = b"name,id\nAlice,1\n";
        let diff = compare_tables(left, right, "csv", None, 100).unwrap();
        assert!(diff.rows.is_empty());
    }

    #[test]
    fn handles_tsv() {
        let left = b"id\tname\n1\tAlice\n";
        let right = b"id\tname\n1\tAlicia\n";
        let diff = compare_tables(left, right, "tsv", None, 100).unwrap();
        assert_eq!(diff.delimiter, '\t');
        assert_eq!(diff.rows[0].cells[0].column, "name");
    }
}
