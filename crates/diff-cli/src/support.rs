use anyhow::Result;
use diff_core::model::TOOL_VERSION;

struct FormatInfo {
    name: &'static str,
    category: &'static str,
    aliases: &'static [&'static str],
    summary: &'static str,
    features: &'static [&'static str],
    notes: &'static [&'static str],
}

const FORMATS: &[FormatInfo] = &[
    FormatInfo {
        name: "text",
        category: "Text and code",
        aliases: &["txt", "log", "md", "markdown", "reg", "plain"],
        summary: "Plain text and source files, including UTF-8, UTF-16 and GBK input.",
        features: &[
            "Line diff with configurable context",
            "Word-level inline highlights on changed lines",
            "Ignore whitespace, case or comments",
            "Tree-sitter token, syntax-tree and graph modes for supported languages",
            "Streaming summary fallback for files above the detailed-diff limit",
        ],
        notes: &[
            "Concise mode drops equal context and caps detail; full mode keeps everything.",
            "Capabilities shared with the `code` language families.",
        ],
    },
    FormatInfo {
        name: "code",
        category: "Text and code",
        aliases: &[
            "rust",
            "python",
            "javascript",
            "typescript",
            "js",
            "ts",
            "c",
            "cpp",
            "c++",
            "go",
            "java",
            "bash",
            "shell",
            "ruby",
            "csharp",
            "c#",
            "swift",
            "lua",
        ],
        summary: "Source code with language-aware token and syntax-tree comparison.",
        features: &[
            "Line, token, syntax-tree and graph-event modes",
            "Comment ignoring for supported grammars",
            "Syntax-aware matching across moved or reformatted nodes",
            "Word-level highlights in line mode",
        ],
        notes: &[
            "Language is selected from the file extension.",
            "`--mode tree` and `--mode graph` fall back to text when parsing fails.",
        ],
    },
    FormatInfo {
        name: "json",
        category: "Structured data",
        aliases: &["json", "json5", "openapi", "swagger"],
        summary: "JSON and JSON5 objects, arrays and OpenAPI documents.",
        features: &[
            "Semantic path-level added, removed and changed values",
            "Array similarity alignment instead of strict index matching",
            "OpenAPI breaking-change markers",
            "Cross-format comparison with YAML, TOML, INI, XML, HTML, Plist and Pickle",
        ],
        notes: &[
            "Concise caps changes; full shows every change up to the configured limit.",
            "Limits: `--max-rows`, `--no-limits`.",
        ],
    },
    FormatInfo {
        name: "yaml",
        category: "Structured data",
        aliases: &["yaml", "yml"],
        summary: "YAML mappings, sequences and scalar values.",
        features: &[
            "Semantic path-level diff",
            "Array similarity alignment",
            "Cross-format comparison with JSON and other structured formats",
        ],
        notes: &["Malformed YAML falls back to text comparison."],
    },
    FormatInfo {
        name: "toml",
        category: "Structured data",
        aliases: &["toml"],
        summary: "TOML tables, arrays and values.",
        features: &[
            "Semantic table and key diff",
            "Cross-format comparison with JSON and other structured formats",
        ],
        notes: &["Malformed TOML falls back to text comparison."],
    },
    FormatInfo {
        name: "ini",
        category: "Structured data",
        aliases: &["ini", "cfg", "conf"],
        summary: "INI-style sections and keys.",
        features: &[
            "Section/key-level added, removed and changed values",
            "Cross-format structured comparison",
        ],
        notes: &["Malformed INI falls back to text comparison."],
    },
    FormatInfo {
        name: "xml",
        category: "Structured data",
        aliases: &["xml"],
        summary: "XML element trees, attributes and text.",
        features: &[
            "Element and attribute path diff",
            "Cross-format structured comparison",
        ],
        notes: &["Malformed XML falls back to text comparison."],
    },
    FormatInfo {
        name: "html",
        category: "Structured data",
        aliases: &["html", "htm"],
        summary: "HTML documents parsed into a semantic tree.",
        features: &[
            "Element and attribute diff",
            "Cross-format structured comparison",
        ],
        notes: &["Malformed HTML falls back to text comparison."],
    },
    FormatInfo {
        name: "plist",
        category: "Structured data",
        aliases: &["plist", "mobileconfig"],
        summary: "XML and binary Apple property lists.",
        features: &[
            "Semantic property-list diff",
            "Cross-format structured comparison",
        ],
        notes: &["Both XML and binary plist encodings are supported."],
    },
    FormatInfo {
        name: "pickle",
        category: "Structured data",
        aliases: &["pickle", "pkl"],
        summary: "Python pickle objects with readable fallback for unknown globals.",
        features: &["Semantic object diff", "Cross-format structured comparison"],
        notes: &["Unsafe or unknown globals are rendered readably instead of executed."],
    },
    FormatInfo {
        name: "gettext",
        category: "Structured data",
        aliases: &["mo", "gettext"],
        summary: "Compiled gettext .mo message catalogs.",
        features: &["Header, charset, message and plural-form diff"],
        notes: &["Malformed catalogs fall back to binary comparison."],
    },
    FormatInfo {
        name: "structured",
        category: "Structured data",
        aliases: &["structured", "cross-format", "json+yaml", "yaml+json"],
        summary: "Cross-format semantic comparison (for example JSON against YAML).",
        features: &[
            "Parse both sides into one common tree",
            "Path-level semantic diff across formats",
        ],
        notes: &["Available whenever both inputs have a structured representation."],
    },
    FormatInfo {
        name: "notebook",
        category: "Structured data",
        aliases: &["ipynb", "notebook", "jupyter"],
        summary: "Jupyter notebooks with cell-level comparison.",
        features: &[
            "Cell-level source, output and execution-count diff",
            "Global cell-id alignment and moved-cell detection",
            "Three-way notebook merge",
        ],
        notes: &["Concise caps cells; full shows all changed cells."],
    },
    FormatInfo {
        name: "csv",
        category: "Tables and spreadsheets",
        aliases: &["csv", "tsv"],
        summary: "Delimited tables with row and cell comparison.",
        features: &[
            "Header, column, row and cell diff",
            "Row alignment with `--key <COLUMN>`",
            "Added, removed and changed row markers",
        ],
        notes: &["Limits: `--max-table-rows`, `--no-limits`."],
    },
    FormatInfo {
        name: "workbook",
        category: "Tables and spreadsheets",
        aliases: &["xlsx", "xls", "ods", "spreadsheet"],
        summary: "Excel and OpenDocument spreadsheets.",
        features: &[
            "Worksheet and cell diff by address",
            "Sheet added/removed detection",
        ],
        notes: &["Limits: `--max-workbook-cells`, `--no-limits`."],
    },
    FormatInfo {
        name: "sqlite",
        category: "Tables and spreadsheets",
        aliases: &["sqlite", "sqlite3", "db"],
        summary: "SQLite databases compared semantically.",
        features: &[
            "Table, column, row, index and trigger diff",
            "Physical-layout differences can compare equal",
        ],
        notes: &["Equal databases with different page sizes or layouts compare equal."],
    },
    FormatInfo {
        name: "pdf",
        category: "Documents and mail",
        aliases: &["pdf"],
        summary: "PDF text layer, page structure and optional visual comparison.",
        features: &[
            "Page count and page-level text markers",
            "Text-layer diff",
            "`--pdf-visual` rendered page comparison with HTML/PNG/PDF reports",
            "`--ocr` scanned-page text extraction with Tesseract",
        ],
        notes: &[
            "Visual mode requires poppler `pdftoppm`; OCR requires Tesseract.",
            "Limits: `--pdf-max-pages`, `--no-limits`.",
        ],
    },
    FormatInfo {
        name: "ofd",
        category: "Documents and mail",
        aliases: &["ofd"],
        summary: "Open Fixed-layout Document page text.",
        features: &["ZIP container parsing", "Page text extraction and diff"],
        notes: &[],
    },
    FormatInfo {
        name: "docx",
        category: "Documents and mail",
        aliases: &["docx"],
        summary: "Word documents.",
        features: &["Body, header, footer, footnote, endnote and comment text diff"],
        notes: &["Legacy .doc files fall back to binary comparison."],
    },
    FormatInfo {
        name: "pptx",
        category: "Documents and mail",
        aliases: &["pptx"],
        summary: "PowerPoint presentations.",
        features: &["Slide and notes text extraction and diff"],
        notes: &["Legacy .ppt files fall back to binary comparison."],
    },
    FormatInfo {
        name: "office",
        category: "Documents and mail",
        aliases: &["doc", "ppt"],
        summary: "Legacy Word/PowerPoint files.",
        features: &["Binary comparison fallback"],
        notes: &["Use DOCX/PPTX for semantic text comparison."],
    },
    FormatInfo {
        name: "odt",
        category: "Documents and mail",
        aliases: &["odt"],
        summary: "OpenDocument text documents.",
        features: &["Text and metadata diff", "Embedded entry hashes"],
        notes: &[],
    },
    FormatInfo {
        name: "odp",
        category: "Documents and mail",
        aliases: &["odp"],
        summary: "OpenDocument presentations.",
        features: &["Text and metadata diff", "Embedded entry hashes"],
        notes: &[],
    },
    FormatInfo {
        name: "epub",
        category: "Documents and mail",
        aliases: &["epub"],
        summary: "EPUB books.",
        features: &["Metadata, spine and chapter text diff"],
        notes: &[],
    },
    FormatInfo {
        name: "rtf",
        category: "Documents and mail",
        aliases: &["rtf"],
        summary: "Rich Text Format documents.",
        features: &["Paragraph text and control-word extraction and diff"],
        notes: &[],
    },
    FormatInfo {
        name: "eml",
        category: "Documents and mail",
        aliases: &["eml", "email"],
        summary: "Email messages.",
        features: &["Header, text/HTML body, attachment and attachment-hash diff"],
        notes: &[],
    },
    FormatInfo {
        name: "image",
        category: "Images and media",
        aliases: &[
            "png", "jpg", "jpeg", "gif", "webp", "bmp", "tiff", "tif", "image",
        ],
        summary: "Raster images.",
        features: &[
            "Pixel difference ratio and changed-region bounding boxes",
            "YIQ perceptual tolerance with `--image-threshold`",
            "Heatmap and side-by-side image output",
            "Optional OCR text comparison",
        ],
        notes: &["OCR requires Tesseract."],
    },
    FormatInfo {
        name: "ico",
        category: "Images and media",
        aliases: &["ico", "cur"],
        summary: "Windows icon and cursor containers.",
        features: &["Image entry semantics, dimensions and payload comparison"],
        notes: &[],
    },
    FormatInfo {
        name: "audio",
        category: "Images and media",
        aliases: &[
            "mp3", "flac", "ogg", "m4a", "wav", "aac", "opus", "wma", "audio",
        ],
        summary: "Audio metadata and content.",
        features: &[
            "Format, duration, bitrate, sample rate, channels and tags",
            "RMS waveform window comparison in full mode",
        ],
        notes: &["Concise caps waveform/tag detail; full shows more."],
    },
    FormatInfo {
        name: "video",
        category: "Images and media",
        aliases: &["mp4", "mkv", "avi", "mov", "webm", "video"],
        summary: "Video container metadata and packet-level hashes.",
        features: &["Stream metadata, duration and packet-hash comparison"],
        notes: &["Comparison is metadata/packet-level; it does not decode frames."],
    },
    FormatInfo {
        name: "font",
        category: "Images and media",
        aliases: &["ttf", "otf", "ttc", "woff", "woff2", "font"],
        summary: "TrueType/OpenType collections and web fonts.",
        features: &["Name, metrics, glyph counts and advance-width comparison"],
        notes: &["WOFF/WOFF2 are unpacked to SFNT before comparison."],
    },
    FormatInfo {
        name: "archive",
        category: "Archives and packages",
        aliases: &[
            "zip", "jar", "7z", "sevenzip", "tar", "tar.gz", "tgz", "tar.bz2", "tar.xz", "tar.zst",
            "tar.lz4", "gz", "gzip", "bz2", "bzip2", "xz", "zst", "zstd", "lz4", "cpio", "cab",
            "xar", "pkg", "ar",
        ],
        summary: "Recursive archive containers and single-file compression.",
        features: &[
            "Recursive entry tree and per-entry format dispatch",
            "ZIP bzip2/LZMA/zstd, TAR.gz/bz2/xz/zst/lz4, CAB MSZIP/LZX, XAR/AR/CPIO",
            "Per-entry and entry-count limits",
        ],
        notes: &["Limits: `--max-entries`, `--no-entry-limit`, `--no-limits`."],
    },
    FormatInfo {
        name: "apk",
        category: "Archives and packages",
        aliases: &["apk", "android"],
        summary: "Android APK packages.",
        features: &[
            "Recursive ZIP comparison plus binary AndroidManifest semantics",
            "Full mode DEX-to-Java decompilation and embedded .so disassembly",
            "Large one-sided class sets summarized by descriptor",
            "Packed/APK-shell detection with legal confirmation",
        ],
        notes: &["Full DEX/native detail uses the decompilation prompt; `-y` accepts."],
    },
    FormatInfo {
        name: "deb",
        category: "Archives and packages",
        aliases: &["deb", "debian"],
        summary: "Debian packages.",
        features: &["ar container, control/data tar recursion and per-entry diff"],
        notes: &[],
    },
    FormatInfo {
        name: "rpm",
        category: "Archives and packages",
        aliases: &["rpm", "redhat"],
        summary: "RPM packages.",
        features: &["Metadata and gzip/xz/zstd payload comparison"],
        notes: &[],
    },
    FormatInfo {
        name: "iso",
        category: "Archives and packages",
        aliases: &["iso", "iso9660"],
        summary: "ISO9660 disc images.",
        features: &["Directory tree and file entry comparison"],
        notes: &[],
    },
    FormatInfo {
        name: "package-image",
        category: "Archives and packages",
        aliases: &["squashfs", "snap", "appimage", "flatpak", "dmg", "msix"],
        summary: "Application and disk image packages.",
        features: &[
            "SquashFS/Snap/AppImage entry trees, modes and contents",
            "Flatpak ref/commit metadata and payload hashes",
            "DMG UDIF/block-map metadata and MSIX manifest/block map",
        ],
        notes: &["Deep extraction is format-dependent; see the specific format notes."],
    },
    FormatInfo {
        name: "aab",
        category: "Archives and packages",
        aliases: &["aab", "android-app-bundle"],
        summary: "Android App Bundles.",
        features: &["Bundle protobuf, manifest and module entry comparison"],
        notes: &[],
    },
    FormatInfo {
        name: "ext4",
        category: "Archives and packages",
        aliases: &["ext4"],
        summary: "ext4 filesystem images.",
        features: &["Inode, mode, owner, xattr and file-data comparison"],
        notes: &[],
    },
    FormatInfo {
        name: "parquet",
        category: "Serialized data",
        aliases: &["parquet"],
        summary: "Apache Parquet columnar files.",
        features: &[
            "Schema and row-value comparison",
            "Compression codec handling",
        ],
        notes: &[],
    },
    FormatInfo {
        name: "avro",
        category: "Serialized data",
        aliases: &["avro"],
        summary: "Apache Avro containers.",
        features: &["Schema and record-value comparison"],
        notes: &[],
    },
    FormatInfo {
        name: "orc",
        category: "Serialized data",
        aliases: &["orc"],
        summary: "Apache ORC files.",
        features: &["Metadata, stripe statistics and payload-hash comparison"],
        notes: &[],
    },
    FormatInfo {
        name: "cbor",
        category: "Serialized data",
        aliases: &["cbor"],
        summary: "CBOR values.",
        features: &["Recursive value comparison and structured path diff"],
        notes: &[],
    },
    FormatInfo {
        name: "messagepack",
        category: "Serialized data",
        aliases: &["msgpack", "messagepack"],
        summary: "MessagePack values.",
        features: &["Recursive value comparison and structured path diff"],
        notes: &[],
    },
    FormatInfo {
        name: "protobuf",
        category: "Serialized data",
        aliases: &["protobuf", "pb", "proto"],
        summary: "Generic protobuf wire data.",
        features: &[
            "Generic field-number/value comparison",
            "Nested message and repeated-field diff",
        ],
        notes: &["Schema is inferred from wire data; use JSON for schema-aware comparison."],
    },
    FormatInfo {
        name: "java-class",
        category: "Other formats",
        aliases: &["class", "java-class", "jvm"],
        summary: "Java class files.",
        features: &["Fields, methods and bytecode semantic comparison"],
        notes: &[],
    },
    FormatInfo {
        name: "wasm",
        category: "Other formats",
        aliases: &["wasm", "webassembly"],
        summary: "WebAssembly modules.",
        features: &["Types, imports, exports and code-section comparison"],
        notes: &[],
    },
    FormatInfo {
        name: "dex",
        category: "Other formats",
        aliases: &["dex", "dalvik"],
        summary: "Dalvik executable files.",
        features: &[
            "Class/method bytecode semantic diff",
            "Optional Java source decompilation in full mode",
        ],
        notes: &[],
    },
    FormatInfo {
        name: "git-index",
        category: "Other formats",
        aliases: &["git-index", "git-index-file"],
        summary: "Git index (DIRC) files.",
        features: &["Path, mode, stage, SHA and stat comparison"],
        notes: &[],
    },
    FormatInfo {
        name: "ssh-key",
        category: "Other formats",
        aliases: &["pub", "openssh", "ssh-key"],
        summary: "OpenSSH public keys.",
        features: &["Key type, comment and field comparison"],
        notes: &[],
    },
    FormatInfo {
        name: "executable",
        category: "Other formats",
        aliases: &[
            "exe",
            "dll",
            "so",
            "dylib",
            "elf",
            "mach-o",
            "coff",
            "executable",
        ],
        summary: "Native executables and shared libraries.",
        features: &[
            "Architecture, entry point, section digest, symbols and imports",
            "Fat Mach-O slices",
            "Full mode normalized disassembly via embedded pure-Rust decoders",
            "Packed-application detection and legal confirmation",
        ],
        notes: &["Optional `--objdump` override; `-y` accepts decompilation prompts."],
    },
    FormatInfo {
        name: "binary",
        category: "Other formats",
        aliases: &["bin", "dat", "binary", "unknown"],
        summary: "Any unrecognized byte stream.",
        features: &[
            "Hex rows, differing-byte count and first difference offset",
            "Streaming summary for large files",
        ],
        notes: &["Unknown formats never fail; they fall back to this comparator."],
    },
    FormatInfo {
        name: "directory",
        category: "Other formats",
        aliases: &["dir", "folder", "directory"],
        summary: "Local directory trees.",
        features: &[
            "Recursive file table and per-file dispatch",
            "Exclude globs, entry limits and parallel comparison",
        ],
        notes: &["Symbolic links are not followed."],
    },
    FormatInfo {
        name: "git-history",
        category: "Inputs",
        aliases: &["git", "repo", "repository", "git-history"],
        summary: "Git repository histories.",
        features: &[
            "Refs, HEAD, commit count and commit metadata comparison",
            "Bare-mirror clone with partial-clone fallback",
        ],
        notes: &["Use `--git-history`; two .git URLs enable it automatically."],
    },
    FormatInfo {
        name: "linked-input",
        category: "Inputs",
        aliases: &["link", "url", "http", "https", "linked"],
        summary: "HTTP(S) file links downloaded before comparison.",
        features: &[
            "aria2-backed download with built-in HTTP fallback",
            "100 MiB confirmation guard and `-y`",
            "Automatic Kaspersky scan of linked inputs",
        ],
        notes: &["Limits: `--download-timeout`, `--no-security-scan`."],
    },
    FormatInfo {
        name: "remote-input",
        category: "Inputs",
        aliases: &["webdav", "webdavs", "sftp", "smb", "remote"],
        summary: "WebDAV, SFTP and SMB remote inputs.",
        features: &[
            "File and recursive directory download",
            "URL/CLI credentials and SFTP known_hosts verification",
        ],
        notes: &["`--sync` is not supported for remote inputs."],
    },
];

pub fn print(format: Option<&str>) -> Result<()> {
    println!("DiffEverything {TOOL_VERSION}");
    match format.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("all") => print_all(),
        Some(name) => print_one(name)?,
    }
    Ok(())
}

fn print_all() {
    println!("\nSupported formats\n");
    let mut categories: Vec<&str> = Vec::new();
    for info in FORMATS {
        if !categories.contains(&info.category) {
            categories.push(info.category);
        }
    }
    for category in categories {
        println!("{category}");
        for info in FORMATS.iter().filter(|info| info.category == category) {
            println!("  {:<22} {}", info.name, info.aliases.join(", "));
            println!("      {}", info.summary);
        }
        println!();
    }
    println!("Run `diff-everything --support <format>` for detailed diff capabilities.");
}

fn print_one(name: &str) -> Result<()> {
    let query = name.to_ascii_lowercase();
    let query = query.trim_start_matches('.');
    let Some(info) = FORMATS
        .iter()
        .find(|info| info.name == query || info.aliases.contains(&query))
    else {
        anyhow::bail!(
            "unknown format `{name}`; run `diff-everything --support` to list supported formats"
        );
    };
    println!("\n{} [{}]", info.name.to_uppercase(), info.category);
    println!("Aliases: {}", info.aliases.join(", "));
    println!("Summary: {}", info.summary);
    println!("\nDiff capabilities:");
    for feature in info.features {
        println!("  - {feature}");
    }
    if !info.notes.is_empty() {
        println!("\nNotes:");
        for note in info.notes {
            println!("  - {note}");
        }
    }
    println!("\nDetail: concise caps units; full keeps the complete comparison.");
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/support.rs"]
mod tests;
