//! Unit tests for this module, compiled as its child through `#[path]`.
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
fn decodes_bom_and_keeps_duplicate_keys() {
    let left = b"\xEF\xBB\xBFid,name\n1,Alice\n1,Alice2\n";
    let right = b"id,name\n1,Alice\n1,Alicia\n";
    let diff = compare_tables(left, right, "csv", Some("id"), 100).unwrap();
    assert_eq!(diff.duplicate_keys, vec!["1".to_string()]);
    let row = diff
        .rows
        .iter()
        .find(|row| row.key == "1#2")
        .expect("second occurrence should be compared");
    assert_eq!(row.kind, RowChangeKind::Changed);
    assert_eq!(row.cells[0].column, "name");
    assert_eq!(row.cells[0].left.as_deref(), Some("Alice2"));
    assert_eq!(row.cells[0].right.as_deref(), Some("Alicia"));
}

#[test]
fn infers_a_unique_column_for_row_alignment() {
    let left = b"id,name\n1,Alice\n2,Bob\n";
    let right = b"name,id\nBob,2\nAlice,1\n";
    let diff = compare_tables(left, right, "csv", None, 100).unwrap();
    assert_eq!(diff.implicit_key.as_deref(), Some("id"));
    assert!(diff.rows.is_empty(), "{:?}", diff.rows);
}

#[test]
fn aligns_inserted_rows_without_shifting_later_rows() {
    let left = b"id,name\n1,Alice\n2,Bob\n3,Carol\n";
    let right = b"id,name\n0,Zoe\n1,Alice\n2,Bob\n3,Carol\n";
    let diff = compare_tables(left, right, "csv", None, 100).unwrap();
    assert_eq!(diff.rows.len(), 1, "{:?}", diff.rows);
    assert_eq!(diff.rows[0].kind, RowChangeKind::Added);
    assert_eq!(diff.rows[0].key, "0");
}

#[test]
fn reports_column_moves() {
    let left = b"id,name,age\n1,Alice,30\n";
    let right = b"name,id,age\nAlice,1,30\n";
    let diff = compare_tables(left, right, "csv", None, 100).unwrap();
    assert!(diff.rows.is_empty(), "{:?}", diff.rows);
    assert_eq!(diff.columns_moved.len(), 1, "{:?}", diff.columns_moved);
}

#[test]
fn handles_tsv() {
    let left = b"id\tname\n1\tAlice\n";
    let right = b"id\tname\n1\tAlicia\n";
    let diff = compare_tables(left, right, "tsv", None, 100).unwrap();
    assert_eq!(diff.delimiter, '\t');
    assert_eq!(diff.rows[0].cells[0].column, "name");
}
