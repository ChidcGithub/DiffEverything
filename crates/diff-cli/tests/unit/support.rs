//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn catalog_aliases_are_unique() {
    let mut seen = std::collections::BTreeSet::new();
    for info in FORMATS {
        assert!(seen.insert(info.name), "duplicate format {}", info.name);
        for alias in info.aliases {
            if *alias != info.name {
                assert!(seen.insert(*alias), "duplicate alias {alias}");
            }
        }
    }
}
