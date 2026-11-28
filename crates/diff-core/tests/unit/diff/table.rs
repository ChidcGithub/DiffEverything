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
fn handles_tsv() {
    let left = b"id\tname\n1\tAlice\n";
    let right = b"id\tname\n1\tAlicia\n";
    let diff = compare_tables(left, right, "tsv", None, 100).unwrap();
    assert_eq!(diff.delimiter, '\t');
    assert_eq!(diff.rows[0].cells[0].column, "name");
}
