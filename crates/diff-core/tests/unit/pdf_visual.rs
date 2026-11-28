//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn page_numbers_are_sorted() {
    assert_eq!(page_number(Path::new("page-1.png")), Some(1));
    assert_eq!(page_number(Path::new("page-12.png")), Some(12));
    assert_eq!(page_number(Path::new("page.png")), None);
}
