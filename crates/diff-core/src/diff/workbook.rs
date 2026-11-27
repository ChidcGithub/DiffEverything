use crate::model::{CellChange, SheetDiff, WorkbookDiff};
use calamine::{Data, Reader, open_workbook_auto_from_rs};
use std::collections::BTreeSet;

pub fn compare_workbooks(
    left: &[u8],
    right: &[u8],
    format: &str,
    max_cells: usize,
) -> std::result::Result<WorkbookDiff, String> {
    let left_book = read_workbook(left)?;
    let right_book = read_workbook(right)?;

    let left_names: BTreeSet<String> = left_book.sheets.keys().cloned().collect();
    let right_names: BTreeSet<String> = right_book.sheets.keys().cloned().collect();
    let sheets_added: Vec<String> = right_names.difference(&left_names).cloned().collect();
    let sheets_removed: Vec<String> = left_names.difference(&right_names).cloned().collect();

    let mut sheets = Vec::new();
    let mut truncated = false;
    for name in left_names.intersection(&right_names) {
        let left_sheet = &left_book.sheets[name];
        let right_sheet = &right_book.sheets[name];
        let mut cells = Vec::new();
        let mut sheet_truncated = false;
        let rows = left_sheet.rows.max(right_sheet.rows);
        let columns = left_sheet.columns.max(right_sheet.columns);

        for row in 0..rows {
            for column in 0..columns {
                let left_value = left_sheet.value(row, column);
                let right_value = right_sheet.value(row, column);
                if left_value != right_value {
                    if cells.len() >= max_cells {
                        sheet_truncated = true;
                        truncated = true;
                        break;
                    }
                    cells.push(CellChange {
                        column: cell_address(row, column),
                        left: left_value,
                        right: right_value,
                    });
                }
            }
            if sheet_truncated {
                break;
            }
        }

        sheets.push(SheetDiff {
            name: name.clone(),
            left_rows: left_sheet.rows,
            left_columns: left_sheet.columns,
            right_rows: right_sheet.rows,
            right_columns: right_sheet.columns,
            cells,
            truncated: sheet_truncated,
        });
    }

    Ok(WorkbookDiff {
        format: format.to_string(),
        sheets_added,
        sheets_removed,
        sheets,
        truncated,
    })
}

struct Sheet {
    rows: u32,
    columns: u32,
    cells: Vec<Vec<Option<String>>>,
}

impl Sheet {
    fn value(&self, row: u32, column: u32) -> Option<String> {
        self.cells
            .get(row as usize)
            .and_then(|values| values.get(column as usize))
            .cloned()
            .flatten()
    }
}

struct Workbook {
    sheets: std::collections::BTreeMap<String, Sheet>,
}

fn read_workbook(bytes: &[u8]) -> std::result::Result<Workbook, String> {
    let cursor = std::io::Cursor::new(bytes);
    let mut workbook = open_workbook_auto_from_rs(cursor).map_err(|error| error.to_string())?;
    let names = workbook.sheet_names().to_owned();
    let mut sheets = std::collections::BTreeMap::new();

    for name in names {
        let range = match workbook.worksheet_range(&name) {
            Ok(range) => range,
            Err(error) => return Err(format!("sheet {name}: {error}")),
        };
        let (rows, columns) = range.get_size();
        let mut cells = Vec::with_capacity(rows);
        for row in 0..rows {
            let mut values = Vec::with_capacity(columns);
            for column in 0..columns {
                let value = range
                    .get_value((row as u32, column as u32))
                    .map(data_to_string)
                    .filter(|value| !value.is_empty());
                values.push(value);
            }
            cells.push(values);
        }
        sheets.insert(
            name,
            Sheet {
                rows: rows as u32,
                columns: columns as u32,
                cells,
            },
        );
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
mod tests {
    use super::*;

    #[test]
    fn column_names_are_spreadsheet_style() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(27), "AB");
        assert_eq!(column_name(701), "ZZ");
        assert_eq!(column_name(702), "AAA");
    }
}
