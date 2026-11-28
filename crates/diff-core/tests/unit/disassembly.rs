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
fn archive_code_presence_scans_past_entry_2000() {
    use std::io::Write as _;

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("late-dex.apk");
    let mut buffer = Vec::new();
    {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(&mut buffer));
        let options = zip::write::SimpleFileOptions::default();
        for index in 0..2_100 {
            writer
                .start_file(format!("res/{index:05}.txt"), options)
                .unwrap();
            writer.write_all(b"x").unwrap();
        }
        writer.start_file("classes.dex", options).unwrap();
        writer.write_all(b"dex\n035\0").unwrap();
        writer.finish().unwrap();
    }
    std::fs::write(&path, &buffer).unwrap();
    let presence = archive_code_presence(&path);
    assert!(presence.dex, "{presence:?}");
    assert!(!presence.native);
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
