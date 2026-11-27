use crate::diff::text;
use crate::model::DecompilationDiff;
use object::{Object, ObjectSection, ObjectSymbol, SymbolKind};
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;
use yaxpeax_arch::{Decoder, Reader, U8Reader};
use zip::ZipArchive;

const DEFAULT_MAX_LINES: usize = 20_000;

#[derive(Debug, Clone)]
pub struct DisassemblyOptions {
    /// Optional external objdump override. When absent, the embedded pure-Rust
    /// decoders are used.
    pub tool: Option<PathBuf>,
    pub context: usize,
    pub max_hunks: usize,
    pub max_lines: usize,
}

impl DisassemblyOptions {
    pub fn new(tool: impl Into<Option<PathBuf>>, context: usize, max_hunks: usize) -> Self {
        Self {
            tool: tool.into(),
            context,
            max_hunks,
            max_lines: DEFAULT_MAX_LINES,
        }
    }
}

/// Compare decompiled assembly for two files.
pub fn compare_paths(
    left: &Path,
    right: &Path,
    options: &DisassemblyOptions,
) -> Result<DecompilationDiff, String> {
    let left_output = if let Some(tool) = &options.tool {
        disassemble_external(tool, left)?
    } else {
        disassemble_file(left)?
    };
    let right_output = if let Some(tool) = &options.tool {
        disassemble_external(tool, right)?
    } else {
        disassemble_file(right)?
    };
    build_diff(left_output, right_output, options)
}

/// Compare decompiled assembly for two in-memory files (archive entries).
pub fn compare_bytes(
    left: &[u8],
    right: &[u8],
    options: &DisassemblyOptions,
) -> Result<DecompilationDiff, String> {
    if options.tool.is_some() {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let left_path = directory.path().join("left.bin");
        let right_path = directory.path().join("right.bin");
        std::fs::write(&left_path, left).map_err(|error| error.to_string())?;
        std::fs::write(&right_path, right).map_err(|error| error.to_string())?;
        return compare_paths(&left_path, &right_path, options);
    }
    let left_output = disassemble_bytes(left)?;
    let right_output = disassemble_bytes(right)?;
    build_diff(left_output, right_output, options)
}

fn build_diff(
    left_output: String,
    right_output: String,
    options: &DisassemblyOptions,
) -> Result<DecompilationDiff, String> {
    let (left_text, left_truncated, left_lines) =
        normalize_output(left_output.as_bytes(), options.max_lines);
    let (right_text, right_truncated, right_lines) =
        normalize_output(right_output.as_bytes(), options.max_lines);
    let mut diff = text::compare_text(&left_text, &right_text, options.context, options.max_hunks);
    diff.format = "disassembly".to_string();
    diff.encoding_left = "utf-8".to_string();
    diff.encoding_right = "utf-8".to_string();
    diff.truncated |= left_truncated || right_truncated;
    Ok(DecompilationDiff {
        tool: options
            .tool
            .as_ref()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "embedded".to_string()),
        left_lines,
        right_lines,
        truncated: left_truncated || right_truncated,
        text: diff,
    })
}

fn disassemble_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    disassemble_bytes(&bytes)
}

fn disassemble_external(tool: &Path, path: &Path) -> Result<String, String> {
    let output = Command::new(tool)
        .arg("-d")
        .arg("--demangle")
        .arg("--no-show-raw-insn")
        .arg(path)
        .output()
        .map_err(|error| format!("failed to run {}: {error}", tool.display()))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "{} failed for {}: {}",
            tool.display(),
            path.display(),
            detail.trim()
        ));
    }
    if output.stdout.is_empty() {
        return Err(format!(
            "{} produced no disassembly for {}",
            tool.display(),
            path.display()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

macro_rules! decode_section {
    ($decoder:expr, $data:expr, $address:expr, $symbols:expr, $output:expr) => {{
        let decoder = $decoder;
        let mut reader = U8Reader::new($data);
        loop {
            let offset = <U8Reader as Reader<u64, u8>>::total_offset(&mut reader);
            let address = $address.saturating_add(offset);
            if let Some(name) = $symbols.get(&address) {
                let _ = writeln!($output, "<{name}>:");
            }
            match decoder.decode(&mut reader) {
                Ok(instruction) => {
                    let _ = writeln!($output, "{address:x}: {instruction}");
                }
                Err(_) => break,
            }
        }
    }};
}

/// Disassemble an ELF/PE/Mach-O image with embedded pure-Rust decoders.
///
/// Only executable sections are emitted. Function symbols become labels and
/// each instruction is prefixed with its virtual address; the normalizer later
/// removes addresses so layout shifts do not dominate the diff.
pub fn disassemble_bytes(bytes: &[u8]) -> Result<String, String> {
    let file = object::File::parse(bytes).map_err(|error| error.to_string())?;
    let mut symbols: BTreeMap<u64, String> = BTreeMap::new();
    for symbol in file.symbols() {
        if symbol.kind() == SymbolKind::Text
            && let Ok(name) = symbol.name()
            && !name.is_empty()
        {
            symbols
                .entry(symbol.address())
                .or_insert_with(|| name.to_string());
        }
    }

    let mut output = String::new();
    for section in file.sections() {
        if section.kind() != object::SectionKind::Text {
            continue;
        }
        let Ok(data) = section.data() else {
            continue;
        };
        if data.is_empty() {
            continue;
        }
        let name = section.name().unwrap_or("<invalid>");
        let _ = writeln!(output, "Disassembly of section {name}:");
        let address = section.address();
        match file.architecture() {
            object::Architecture::X86_64 => {
                decode_section!(
                    yaxpeax_x86::amd64::InstDecoder::default(),
                    data,
                    address,
                    symbols,
                    output
                );
            }
            object::Architecture::I386 => {
                decode_section!(
                    yaxpeax_x86::protected_mode::InstDecoder::default(),
                    data,
                    address,
                    symbols,
                    output
                );
            }
            object::Architecture::Aarch64 => {
                decode_section!(
                    <yaxpeax_arm::armv8::a64::ARMv8 as yaxpeax_arch::Arch>::Decoder::default(),
                    data,
                    address,
                    symbols,
                    output
                );
            }
            object::Architecture::Arm => {
                decode_section!(
                    <yaxpeax_arm::armv7::ARMv7 as yaxpeax_arch::Arch>::Decoder::default(),
                    data,
                    address,
                    symbols,
                    output
                );
            }
            other => {
                return Err(format!("embedded disassembler does not support {other:?}"));
            }
        }
    }
    if output.is_empty() {
        return Err("no executable sections found".to_string());
    }
    Ok(output)
}

fn normalize_output(bytes: &[u8], max_lines: usize) -> (String, bool, usize) {
    let text = String::from_utf8_lossy(bytes);
    let total = text.lines().count();
    let mut out = String::new();
    let mut kept = 0usize;
    for line in text.lines() {
        if kept >= max_lines {
            break;
        }
        let normalized = normalize_line(line);
        out.push_str(&normalized);
        out.push('\n');
        kept += 1;
    }
    (out, total > kept, total)
}

fn normalize_line(line: &str) -> String {
    let stripped = strip_address(line);
    replace_hex_addresses(&stripped)
}

fn replace_hex_addresses(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'0' && bytes.get(index + 1) == Some(&b'x') && index + 6 <= bytes.len() {
            let digits = bytes[index + 2..]
                .iter()
                .take_while(|byte| byte.is_ascii_hexdigit())
                .count();
            if digits >= 4 {
                out.push_str("0xADDR");
                index += 2 + digits;
                continue;
            }
        }
        out.push(bytes[index] as char);
        index += 1;
    }
    out
}

fn strip_address(line: &str) -> String {
    let trimmed = line.trim_start();
    let Some(end) = trimmed.find(|ch: char| ch == ':' || ch.is_whitespace()) else {
        return line.to_string();
    };
    let token = &trimmed[..end];
    if token.is_empty() || !token.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return line.to_string();
    }
    let rest = &trimmed[end..];
    if let Some(rest) = rest.strip_prefix(':') {
        return rest.trim_start().to_string();
    }
    let rest = rest.trim_start();
    if rest.starts_with('<') {
        return rest.to_string();
    }
    line.to_string()
}

/// Return true when a ZIP/APK contains code-bearing entries.
pub fn archive_contains_code(path: &Path) -> bool {
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let Ok(mut archive) = ZipArchive::new(file) else {
        return false;
    };
    for index in 0..archive.len().min(2_000) {
        let Ok(mut entry) = archive.by_index(index) else {
            continue;
        };
        let name = entry.name().to_ascii_lowercase();
        if name.ends_with(".dex")
            || name.ends_with(".so")
            || name.ends_with(".dll")
            || name.ends_with(".exe")
            || name.ends_with(".elf")
        {
            return true;
        }
        let mut header = [0u8; 4];
        use std::io::Read as _;
        if entry.read_exact(&mut header).is_ok()
            && (header == *b"\x7fELF"
                || header == *b"dex\n"
                || (header[0] == b'M' && header[1] == b'Z'))
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
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
}
