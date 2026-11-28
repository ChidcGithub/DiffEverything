//! Unit tests for this module, compiled as its child through `#[path]`.
use super::*;

#[test]
fn strips_instruction_and_label_addresses() {
    assert_eq!(strip_address("      1139: pushq %rbp"), "pushq %rbp");
    assert_eq!(strip_address("0000000000001139 <main>:"), "<main>:");
    assert_eq!(
        strip_address("Disassembly of section .text:"),
        "Disassembly of section .text:"
    );
}

#[test]
fn disassembles_x86_elf_with_embedded_decoder() {
    let bytes = std::fs::read(fixture("executable/hello-old")).unwrap();
    let text = disassemble_bytes(&bytes).unwrap();
    assert!(text.contains("Disassembly of section .text:"), "{text}");
    assert!(text.contains("ret"), "{text}");
}

fn fixture(relative: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.pop();
    path.pop();
    path.join("fixtures").join(relative)
}
