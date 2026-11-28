//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;
use std::path::PathBuf;

fn fixture(relative: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.join("fixtures").join(relative)
}

#[test]
fn extracts_elf_sections_symbols_and_libraries() {
    let bytes = std::fs::read(fixture("executable/hello-old")).unwrap();
    let value = parse_executable(&bytes).unwrap();
    assert_eq!(value["format"], "elf");
    assert_eq!(value["architecture"], "X86_64");
    assert!(
        matches!(value["kind"].as_str(), Some("Executable" | "Dynamic")),
        "{}",
        value["kind"]
    );
    assert!(
        value["sections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|section| section["name"] == ".text" && section["size"].as_u64().unwrap() > 0)
    );
    assert!(value["symbols"].is_array());
    assert!(value["imports"].as_array().unwrap().iter().any(|import| {
        import["library"]
            .as_str()
            .is_some_and(|name| name.contains("libc"))
    }));
    assert!(
        value["libraries"]
            .as_array()
            .unwrap()
            .iter()
            .any(|library| library.as_str().is_some_and(|name| name.contains("libc")))
    );
}

#[test]
fn rejects_non_executable_bytes() {
    assert!(parse_executable(b"not an executable").is_err());
}
