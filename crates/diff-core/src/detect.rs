use crate::model::FileKind;
use std::path::Path;

/// Detect the broad file kind. Container/document extensions are checked
/// before magic bytes (e.g. .docx/.xlsx are ZIP files underneath).
pub fn detect_file(path: &Path) -> FileKind {
    if path.is_dir() {
        return FileKind::Directory;
    }

    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();

    if let Some(kind) = kind_from_name(&name)
        && is_container_extension(kind)
    {
        return kind;
    }

    let mut header = [0u8; 512];
    let read = std::fs::File::open(path)
        .and_then(|mut file| std::io::Read::read(&mut file, &mut header))
        .unwrap_or(0);
    let kind = detect_bytes(&name, &header[..read]);
    if kind != FileKind::Unknown && kind != FileKind::Text {
        return kind;
    }

    kind_from_name(&name).unwrap_or(kind)
}

/// Detect a file kind from a name and its leading bytes.
pub fn detect_bytes(name: &str, bytes: &[u8]) -> FileKind {
    let lower = name.to_ascii_lowercase();

    // Property lists can start with an XML declaration or the binary plist
    // magic, so the extension must win over generic XML detection.
    if lower.ends_with(".plist") {
        return FileKind::Plist;
    }
    // Java class files share 0xcafebabe with fat Mach-O archives. The major
    // version field disambiguates them: Java majors start at 45.
    if lower.ends_with(".class") {
        return FileKind::JavaClass;
    }

    if let Some(kind) = kind_from_name(&lower)
        && is_container_extension(kind)
    {
        return kind;
    }

    if is_png(bytes)
        || is_jpeg(bytes)
        || is_gif(bytes)
        || is_webp(bytes)
        || is_bmp(bytes)
        || is_tiff(bytes)
    {
        return FileKind::Image;
    }
    if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) || bytes.starts_with(&[0x00, 0x00, 0x02, 0x00])
    {
        return FileKind::Ico;
    }
    if bytes.starts_with(b"dex\n") {
        return FileKind::Dex;
    }
    if bytes.starts_with(b"DIRC") {
        return FileKind::GitIndex;
    }
    if is_ssh_public_key(bytes) {
        return FileKind::SshPublicKey;
    }
    if bytes.starts_with(&[0x03, 0x00, 0x08, 0x00]) {
        return FileKind::AndroidXml;
    }
    if bytes.len() >= 262 && &bytes[257..262] == b"ustar" {
        return FileKind::Tar;
    }
    if bytes.starts_with(b"\0asm") {
        return FileKind::Wasm;
    }
    if bytes.starts_with(b"%PDF-") {
        return FileKind::Pdf;
    }
    if bytes.starts_with(b"bplist00") {
        return FileKind::Plist;
    }
    if bytes.starts_with(&[0xde, 0x12, 0x04, 0x95]) || bytes.starts_with(&[0x95, 0x04, 0x12, 0xde])
    {
        return FileKind::Mo;
    }
    if bytes.starts_with(b"SQLite format 3\0") {
        return FileKind::Sqlite;
    }
    if bytes.starts_with(&[0xca, 0xfe, 0xba, 0xbe])
        && bytes.len() >= 8
        && u16::from_be_bytes([bytes[6], bytes[7]]) >= 45
    {
        return FileKind::JavaClass;
    }
    if is_executable(bytes) {
        return FileKind::Executable;
    }
    if is_font(bytes) {
        return FileKind::Font;
    }
    if bytes.starts_with(b"070701") || bytes.starts_with(b"070702") {
        return FileKind::Cpio;
    }
    if bytes.starts_with(&[0xED, 0xAB, 0xEE, 0xDB]) {
        return FileKind::Rpm;
    }
    if bytes.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
        return FileKind::Zstd;
    }
    if bytes.len() >= 2 && bytes[0] == 0x80 && (2..=5).contains(&bytes[1]) {
        return FileKind::Pickle;
    }
    if bytes.starts_with(b"MSCF") {
        return FileKind::Cab;
    }
    if bytes.starts_with(b"xar!") {
        return FileKind::Xar;
    }
    if bytes.starts_with(b"!<arch>\n") {
        return FileKind::Ar;
    }
    if is_zip(bytes) {
        return FileKind::Zip;
    }
    if bytes.starts_with(b"PAR1") {
        return FileKind::Parquet;
    }
    if bytes.starts_with(b"Obj\x01") {
        return FileKind::Avro;
    }
    if bytes.starts_with(b"ORC") {
        return FileKind::Orc;
    }
    if bytes.starts_with(b"hsqs") || bytes.starts_with(b"sqsh") {
        return FileKind::Squashfs;
    }
    if bytes.starts_with(b"{\\rtf") {
        return FileKind::Rtf;
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        return FileKind::Video;
    }
    if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return FileKind::Video;
    }
    if bytes.len() >= 1082 && bytes[1080..1082] == [0x53, 0xEF] {
        return FileKind::Ext4;
    }
    if bytes.starts_with(&[0x1F, 0x8B]) {
        return FileKind::Gzip;
    }
    if bytes.starts_with(b"BZh") {
        return FileKind::Bzip2;
    }
    if bytes.starts_with(&[0xFD, b'7', b'z', b'X', b'Z', 0]) {
        return FileKind::Xz;
    }
    if bytes.starts_with(&[0x04, 0x22, 0x4D, 0x18]) {
        return FileKind::Lz4;
    }
    if bytes.starts_with(b"<?xml") {
        return FileKind::Xml;
    }

    if let Some(kind) = kind_from_name(&lower)
        && kind != FileKind::Unknown
    {
        return kind;
    }

    if looks_like_text(bytes) {
        FileKind::Text
    } else {
        FileKind::Binary
    }
}

pub fn looks_like_text(bytes: &[u8]) -> bool {
    if bytes.is_empty() {
        return true;
    }
    if bytes.contains(&0) {
        return false;
    }
    if std::str::from_utf8(bytes).is_ok() {
        return true;
    }
    // UTF-16 text is common on Windows; accept a BOM.
    bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF])
}

fn is_container_extension(kind: FileKind) -> bool {
    matches!(
        kind,
        FileKind::Zip
            | FileKind::SevenZip
            | FileKind::Tar
            | FileKind::Deb
            | FileKind::Rpm
            | FileKind::Iso
            | FileKind::Cpio
            | FileKind::Cab
            | FileKind::Xar
            | FileKind::Ar
            | FileKind::Pdf
            | FileKind::Ofd
            | FileKind::Docx
            | FileKind::Xlsx
            | FileKind::Pptx
            | FileKind::Office
            | FileKind::Squashfs
            | FileKind::Snap
            | FileKind::AppImage
            | FileKind::Flatpak
            | FileKind::Dmg
            | FileKind::Msix
            | FileKind::Aab
            | FileKind::Ext4
            | FileKind::Odt
            | FileKind::Odp
            | FileKind::Epub
            | FileKind::Gzip
            | FileKind::Zstd
            | FileKind::Bzip2
            | FileKind::Xz
            | FileKind::Lz4
    )
}

fn kind_from_name(name: &str) -> Option<FileKind> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".tar.gz")
        || lower.ends_with(".tgz")
        || lower.ends_with(".tar.bz2")
        || lower.ends_with(".tbz2")
        || lower.ends_with(".tar.xz")
        || lower.ends_with(".txz")
        || lower.ends_with(".tar.zst")
        || lower.ends_with(".tzst")
        || lower.ends_with(".tar.lz4")
        || lower.ends_with(".tlz4")
    {
        return Some(FileKind::Tar);
    }

    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or(&lower);
    let kind = match ext {
        "json" | "json5" | "har" => FileKind::Json,
        "ipynb" => FileKind::Notebook,
        "yaml" | "yml" => FileKind::Yaml,
        "toml" => FileKind::Toml,
        "ini" | "cfg" | "conf" => FileKind::Ini,
        "xml" | "svg" => FileKind::Xml,
        "html" | "htm" => FileKind::Html,
        "plist" => FileKind::Plist,
        "pkl" | "pickle" => FileKind::Pickle,
        "mo" | "gmo" => FileKind::Mo,
        "wasm" => FileKind::Wasm,
        "class" => FileKind::JavaClass,
        "dex" => FileKind::Dex,
        "db" | "sqlite" | "sqlite3" => FileKind::Sqlite,
        "exe" | "dll" | "so" | "dylib" | "elf" | "macho" | "o" => FileKind::Executable,
        "ttf" | "otf" | "ttc" | "woff" | "woff2" => FileKind::Font,
        "csv" | "tsv" => FileKind::Csv,
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "tif" | "tiff" | "avif" => {
            FileKind::Image
        }
        "ico" | "cur" => FileKind::Ico,
        "mp3" | "flac" | "ogg" | "oga" | "m4a" | "wav" | "aac" | "opus" | "wma" | "aiff"
        | "ape" => FileKind::Audio,
        "pdf" => FileKind::Pdf,
        "ofd" => FileKind::Ofd,
        "docx" => FileKind::Docx,
        "doc" => FileKind::Office,
        "xlsx" | "xlsm" | "xls" | "ods" => FileKind::Xlsx,
        "pptx" => FileKind::Pptx,
        "ppt" => FileKind::Office,
        "odt" => FileKind::Odt,
        "odp" => FileKind::Odp,
        "epub" => FileKind::Epub,
        "rtf" => FileKind::Rtf,
        "eml" => FileKind::Eml,
        "squashfs" | "sqfs" | "sfs" => FileKind::Squashfs,
        "snap" => FileKind::Snap,
        "appimage" => FileKind::AppImage,
        "flatpak" | "flatpakref" | "flatpakrepo" => FileKind::Flatpak,
        "dmg" => FileKind::Dmg,
        "msix" | "appx" => FileKind::Msix,
        "aab" => FileKind::Aab,
        "ext4" => FileKind::Ext4,
        "parquet" => FileKind::Parquet,
        "avro" => FileKind::Avro,
        "orc" => FileKind::Orc,
        "cbor" => FileKind::Cbor,
        "msgpack" | "mpack" => FileKind::MessagePack,
        "pb" | "protobuf" | "pbf" => FileKind::Protobuf,
        "mp4" | "m4v" | "mkv" | "webm" | "mov" | "avi" | "wmv" | "flv" => FileKind::Video,
        "zip" | "jar" | "apk" | "whl" => FileKind::Zip,
        "7z" => FileKind::SevenZip,
        "tar" | "tgz" => FileKind::Tar,
        "deb" => FileKind::Deb,
        "rpm" => FileKind::Rpm,
        "iso" => FileKind::Iso,
        "cpio" => FileKind::Cpio,
        "cab" => FileKind::Cab,
        "xar" | "pkg" => FileKind::Xar,
        "a" | "ar" | "lib" => FileKind::Ar,
        "gz" => FileKind::Gzip,
        "ipk" => FileKind::Gzip,
        "zst" => FileKind::Zstd,
        "bz2" => FileKind::Bzip2,
        "xz" => FileKind::Xz,
        "lz4" => FileKind::Lz4,
        "txt" | "text" | "md" | "markdown" | "rst" | "log" | "css" | "scss" | "sass" | "less"
        | "js" | "jsx" | "mjs" | "cjs" | "ts" | "tsx" | "vue" | "svelte" | "rs" | "py" | "pyi"
        | "rb" | "go" | "java" | "kt" | "kts" | "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh"
        | "cs" | "php" | "swift" | "m" | "mm" | "sh" | "bash" | "zsh" | "fish" | "ps1" | "bat"
        | "cmd" | "sql" | "graphql" | "gql" | "proto" | "gradle" | "lock" | "gitignore"
        | "dockerfile" | "makefile" | "cmake" | "env" | "tex" | "lua" | "r" | "dart" | "scala"
        | "clj" | "ex" | "exs" | "erl" | "hs" | "zig" | "nim" | "v" | "sol" | "asm" | "s"
        | "jsonl" | "ndjson" | "reg" => FileKind::Text,
        "bin" | "dat" | "pyc" => FileKind::Binary,
        _ => FileKind::Unknown,
    };
    Some(kind)
}

fn is_ssh_public_key(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let Some(line) = text.lines().next() else {
        return false;
    };
    let mut parts = line.split_whitespace();
    let Some(kind) = parts.next() else {
        return false;
    };
    let Some(blob) = parts.next() else {
        return false;
    };
    (kind.starts_with("ssh-") || kind.starts_with("ecdsa-") || kind.starts_with("sk-"))
        && blob.len() > 20
        && blob
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '/' | '='))
}

fn is_png(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
}
fn is_jpeg(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\xFF\xD8\xFF")
}
fn is_gif(bytes: &[u8]) -> bool {
    bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")
}
fn is_webp(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP"
}
fn is_bmp(bytes: &[u8]) -> bool {
    bytes.starts_with(b"BM")
}
fn is_tiff(bytes: &[u8]) -> bool {
    bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*")
}
fn is_zip(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
}

fn is_font(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x00, 0x01, 0x00, 0x00])
        || bytes.starts_with(b"OTTO")
        || bytes.starts_with(b"true")
        || bytes.starts_with(b"typ1")
        || bytes.starts_with(b"ttcf")
        || bytes.starts_with(b"wOFF")
        || bytes.starts_with(b"wOF2")
}

fn is_executable(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"\x7fELF") {
        return true;
    }
    if bytes.len() >= 0x40 && bytes.starts_with(b"MZ") {
        let offset =
            u32::from_le_bytes([bytes[0x3c], bytes[0x3d], bytes[0x3e], bytes[0x3f]]) as usize;
        if offset
            .checked_add(4)
            .is_some_and(|end| end <= bytes.len() && &bytes[offset..end] == b"PE\0\0")
        {
            return true;
        }
    }
    const MACHO_MAGICS: [[u8; 4]; 7] = [
        [0xFE, 0xED, 0xFA, 0xCE],
        [0xFE, 0xED, 0xFA, 0xCF],
        [0xCE, 0xFA, 0xED, 0xFE],
        [0xCF, 0xFA, 0xED, 0xFE],
        [0xCA, 0xFE, 0xBA, 0xBE],
        [0xBE, 0xBA, 0xFE, 0xCA],
        [0xCA, 0xFE, 0xBA, 0xBF],
    ];
    MACHO_MAGICS.iter().any(|magic| bytes.starts_with(magic))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_formats() {
        assert_eq!(
            detect_bytes("a.png", b"\x89PNG\r\n\x1a\nrest"),
            FileKind::Image
        );
        assert_eq!(detect_bytes("a.json", b"{\"a\":1}"), FileKind::Json);
        assert_eq!(detect_bytes("a.pdf", b"%PDF-1.7"), FileKind::Pdf);
        assert_eq!(detect_bytes("a.zip", b"PK\x03\x04"), FileKind::Zip);
        assert_eq!(detect_bytes("a.docx", b"PK\x03\x04"), FileKind::Docx);
        assert_eq!(detect_bytes("a.xlsx", b"PK\x03\x04"), FileKind::Xlsx);
        assert_eq!(detect_bytes("a.pptx", b"PK\x03\x04"), FileKind::Pptx);
        assert_eq!(detect_bytes("a.rs", b"fn main() {}"), FileKind::Text);
        assert_eq!(detect_bytes("a.bin", &[0, 1, 2]), FileKind::Binary);
    }

    #[test]
    fn detects_structured_and_tabular_formats() {
        assert_eq!(detect_bytes("a.yaml", b"name: x\n"), FileKind::Yaml);
        assert_eq!(detect_bytes("a.toml", b"name = 'x'\n"), FileKind::Toml);
        assert_eq!(detect_bytes("a.xml", b"<root/>"), FileKind::Xml);
        assert_eq!(
            detect_bytes("a.plist", b"<?xml version=\"1.0\"?><plist/>"),
            FileKind::Plist
        );
        assert_eq!(detect_bytes("a.plist", b"bplist00rest"), FileKind::Plist);
        assert_eq!(detect_bytes("a.pkl", b"\x80\x04rest"), FileKind::Pickle);
        assert_eq!(
            detect_bytes("model.pickle", b"\x80\x05rest"),
            FileKind::Pickle
        );
        assert_eq!(
            detect_bytes("binary-plist", b"bplist00rest"),
            FileKind::Plist
        );
        assert_eq!(detect_bytes("a.csv", b"a,b\n1,2\n"), FileKind::Csv);
        assert_eq!(detect_bytes("a.tsv", b"a\tb\n"), FileKind::Csv);
        assert_eq!(
            detect_bytes("a.ipynb", b"{\"cells\":[]}"),
            FileKind::Notebook
        );
        assert_eq!(detect_bytes("a.mp3", b"ID3"), FileKind::Audio);
    }

    #[test]
    fn detects_archives() {
        assert_eq!(detect_bytes("a.tar", b"far away"), FileKind::Tar);
        assert_eq!(detect_bytes("a.tar.gz", b"\x1f\x8b"), FileKind::Tar);
        assert_eq!(detect_bytes("a.tgz", b"\x1f\x8b"), FileKind::Tar);
        assert_eq!(
            detect_bytes("a.tar.zst", b"\x28\xb5\x2f\xfd"),
            FileKind::Tar
        );
        assert_eq!(detect_bytes("a.tzst", b"\x28\xb5\x2f\xfd"), FileKind::Tar);
        assert_eq!(
            detect_bytes("a.log.zst", b"\x28\xb5\x2f\xfd"),
            FileKind::Zstd
        );
        assert_eq!(
            detect_bytes("archive.zst", b"\x28\xb5\x2f\xfd"),
            FileKind::Zstd
        );
        assert_eq!(detect_bytes("data.bz2", b"BZh9rest"), FileKind::Bzip2);
        assert_eq!(detect_bytes("data.xz", b"\xfd7zXZ\0rest"), FileKind::Xz);
        assert_eq!(
            detect_bytes("data.lz4", b"\x04\x22\x4d\x18rest"),
            FileKind::Lz4
        );
        assert_eq!(
            detect_bytes("a.tar.lz4", b"\x04\x22\x4d\x18rest"),
            FileKind::Tar
        );
        assert_eq!(
            detect_bytes("a.7z", b"7z\xbc\xaf\x27\x1c"),
            FileKind::SevenZip
        );
        assert_eq!(detect_bytes("a.deb", b"!<arch>\n"), FileKind::Deb);
        assert_eq!(detect_bytes("a.iso", b"CD001"), FileKind::Iso);
        assert_eq!(detect_bytes("a.cpio", b"070701rest"), FileKind::Cpio);
        assert_eq!(
            detect_bytes("a.rpm", b"\xed\xab\xee\xdbrest"),
            FileKind::Rpm
        );
    }

    #[test]
    fn extensionless_utf8_is_text() {
        assert_eq!(detect_bytes("README", b"hello\nworld\n"), FileKind::Text);
    }

    #[test]
    fn xml_magic_is_detected() {
        assert_eq!(
            detect_bytes("unknown", b"<?xml version=\"1.0\"?><root/>"),
            FileKind::Xml
        );
    }
}
