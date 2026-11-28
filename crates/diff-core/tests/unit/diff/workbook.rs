//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn detects_sheet_reordering() {
    let left = vec!["Beta".to_string(), "Alpha".to_string()];
    let right = vec!["Alpha".to_string(), "Beta".to_string()];
    let moves = detect_sheet_moves(&left, &right, &left, &right);
    assert_eq!(moves.len(), 1, "{moves:?}");
    assert_eq!(moves[0].left_index + moves[0].right_index, 3);
}

#[test]
fn unchanged_sheet_order_is_not_a_move() {
    let left = vec!["Alpha".to_string(), "Beta".to_string()];
    let right = left.clone();
    assert!(detect_sheet_moves(&left, &right, &left, &right).is_empty());
}

#[test]
fn aligns_inserted_rows_before_comparing_cells() {
    let left = Sheet {
        rows: 2,
        columns: 2,
        cells: vec![
            vec![Some("Alice".into()), Some("1".into())],
            vec![Some("Bob".into()), Some("2".into())],
        ],
        formulas: vec![vec![None, None], vec![None, None]],
        hidden: false,
    };
    let right = Sheet {
        rows: 3,
        columns: 2,
        cells: vec![
            vec![Some("Zoe".into()), Some("0".into())],
            vec![Some("Alice".into()), Some("1".into())],
            vec![Some("Bob".into()), Some("2".into())],
        ],
        formulas: vec![vec![None, None], vec![None, None], vec![None, None]],
        hidden: false,
    };
    let (cells, truncated) = compare_sheet_cells(
        &left,
        &right,
        false,
        100,
        &std::collections::BTreeSet::new(),
    );
    assert!(!truncated);
    assert_eq!(cells.len(), 2, "{cells:?}");
    assert!(cells.iter().all(|cell| cell.left.is_none()));
    assert_eq!(cells[0].column, "A1");
    assert_eq!(cells[1].column, "B1");
}

#[test]
fn detects_xlsx_conditional_formatting() {
    use rust_xlsxwriter::{ConditionalFormatCell, ConditionalFormatCellRule, Format, Workbook};

    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.xlsx");
    let new = dir.path().join("new.xlsx");

    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.write(0, 0, 10).unwrap();
    let rule = ConditionalFormatCell::new()
        .set_rule(ConditionalFormatCellRule::GreaterThan(50))
        .set_format(Format::new().set_background_color("FFC7CE"));
    sheet.add_conditional_format(0, 0, 2, 0, &rule).unwrap();
    workbook.save(&old).unwrap();

    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.write(0, 0, 10).unwrap();
    workbook.save(&new).unwrap();

    let left = std::fs::read(&old).unwrap();
    let right = std::fs::read(&new).unwrap();
    let diff = compare_workbooks(&left, &right, "xlsx", 100, false, &[]).unwrap();
    assert!(
        diff.sheets
            .iter()
            .any(|sheet| !sheet.conditional_formats_removed.is_empty()),
        "{:?}",
        diff.sheets
    );
}

#[test]
fn detects_xlsx_conditional_format_style_changes() {
    use rust_xlsxwriter::{ConditionalFormatCell, ConditionalFormatCellRule, Format, Workbook};

    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.xlsx");
    let new = dir.path().join("new.xlsx");

    for (path, color) in [(&old, "FFC7CE"), (&new, "C6EFCE")] {
        let mut workbook = Workbook::new();
        let sheet = workbook.add_worksheet();
        sheet.write(0, 0, 10).unwrap();
        let rule = ConditionalFormatCell::new()
            .set_rule(ConditionalFormatCellRule::GreaterThan(50))
            .set_format(Format::new().set_background_color(color));
        sheet.add_conditional_format(0, 0, 2, 0, &rule).unwrap();
        workbook.save(path).unwrap();
    }

    let left = std::fs::read(&old).unwrap();
    let right = std::fs::read(&new).unwrap();
    let diff = compare_workbooks(&left, &right, "xlsx", 100, false, &[]).unwrap();
    assert!(
        diff.sheets.iter().any(|sheet| {
            !sheet.conditional_formats_added.is_empty()
                || !sheet.conditional_formats_removed.is_empty()
        }),
        "{:?}",
        diff.sheets
    );
}

#[test]
fn detects_xlsx_merges_and_defined_names() {
    use rust_xlsxwriter::{Format, Workbook};

    let dir = tempfile::tempdir().unwrap();
    let old = dir.path().join("old.xlsx");
    let new = dir.path().join("new.xlsx");

    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.write(0, 0, "Merged").unwrap();
    sheet
        .merge_range(0, 0, 1, 1, "Merged", &Format::new())
        .unwrap();
    workbook.define_name("Total", "=Sheet1!$A$1").unwrap();
    workbook.save(&old).unwrap();

    let mut workbook = Workbook::new();
    let sheet = workbook.add_worksheet();
    sheet.write(0, 0, "Merged").unwrap();
    workbook.save(&new).unwrap();

    let left = std::fs::read(&old).unwrap();
    let right = std::fs::read(&new).unwrap();
    let diff = compare_workbooks(&left, &right, "xlsx", 100, false, &[]).unwrap();
    assert!(
        diff.defined_names
            .iter()
            .any(|change| change.path == "defined_name/Total"),
        "{:?}",
        diff.defined_names
    );
    assert!(
        diff.sheets
            .iter()
            .any(|sheet| !sheet.merges_removed.is_empty()),
        "{:?}",
        diff.sheets
    );
}

#[test]
fn column_names_are_spreadsheet_style() {
    assert_eq!(column_name(0), "A");
    assert_eq!(column_name(25), "Z");
    assert_eq!(column_name(26), "AA");
    assert_eq!(column_name(27), "AB");
    assert_eq!(column_name(701), "ZZ");
    assert_eq!(column_name(702), "AAA");
}
