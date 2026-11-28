//! Unit tests for this module, compiled as its child through `#[path]`.
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
