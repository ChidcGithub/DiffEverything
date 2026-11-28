//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn strips_sgr_sequences() {
    assert_eq!(strip_ansi("\x1b[31mred\x1b[0m plain"), "red plain");
    assert_eq!(strip_ansi("no color"), "no color");
}
