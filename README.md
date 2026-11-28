# DiffEverything

**Current release: 1.3.0 (Toyger)**

A Rust CLI that compares heterogeneous files by normalizing them into one diff tree. It does not replace `diff`; it adds format-aware comparison and multiple report formats.

The design draws on diffoscope, Graphtage, difftastic, Open Diff, uni-diff, diff-pdf, nbdime, HDiffPatch and others. See [docs/REFERENCES.md](docs/REFERENCES.md).

## Features

| Area | Formats | Capabilities |
|---|---|---|
| Text / code | Any text, UTF-8 / UTF-16 / GBK, `.reg` exports | Line diff, context, word-level highlights, ignore whitespace / case |
| Syntax-aware | Rust, Python, JS/JSX, TS/TSX, C, C++, Go, Java, Bash, Ruby, C#, Swift, Lua | tree-sitter token diff, full syntax-tree diff (`--mode tree`), bounded syntax-event graph diff (`--mode graph`, including non-local moves), comment ignoring |
| Structured data | JSON / JSON5 / YAML / TOML / INI / XML / HTML / Plist (XML + binary) / Pickle / Gettext (.mo) / OpenAPI | Semantic tree diff with path-level added / removed / changed, equal-subtree short-circuiting, array prefix/suffix trimming, stable fingerprint alignment, OpenAPI breaking-change markers |
| Serialized data | Parquet / Avro / ORC / CBOR / MessagePack / Protobuf | Parquet/Avro schema and row values; ORC metadata, stripe statistics and payload hashes; CBOR/MessagePack values; generic Protobuf wire fields; `--detail concise` caps changes, `full` shows every change |
| Databases | SQLite 3 `.db` / `.sqlite` | Semantic diff of tables, columns, rows, indexes and triggers; equal databases with different page sizes or layouts compare equal |
| Cross-format | JSON ↔ YAML ↔ TOML ↔ INI ↔ XML ↔ HTML ↔ Plist ↔ Pickle | Parse both sides into a common tree, e.g. `YAML+JSON`, `PLIST+JSON`, `PICKLE+JSON` |
| Tables | CSV / TSV | Header, row and cell diff; explicit `--key`, otherwise a unique column is inferred as an implicit key (with a warning), LCS fallback, row moves, duplicate keys, encoding/BOM detection, ignored columns and numeric tolerance |
| Spreadsheets | XLSX / XLS / ODS | Worksheet and cell diff; sheet add/remove/reorder/rename, hidden sheets/rows/columns, merged ranges, defined names, drawing/chart references, conditional-formatting rules, cached values plus optional formulas, row insertion/deletion alignment, and moved row/column detection without cascading cell changes |
| Documents | PDF / OFD / DOCX / PPTX | PDF/OFD page-aligned text-block diff with positions, page sizes/metadata and page add/remove/move detection; DOCX paragraph/heading/list/table blocks with precise run/paragraph font metrics and media hashes; PPTX presentation-order slides with shape text/tables/media/notes, geometry, font metrics and hidden-slide state; PDF↔DOCX/ODT and DOCX↔ODT/PPTX↔ODP block mapping |
| Open documents / mail | ODT / ODP / EPUB / RTF / EML | ODT/ODP block and table diff with metadata/media hashes; EPUB metadata, spine and chapter text; RTF paragraphs and control words; EML headers, text/HTML bodies, attachments and hashes |
| Images / OCR | PNG / JPEG / GIF / WebP / BMP / TIFF / ICO / CUR | Pixel difference ratio, YIQ perceptual tolerance (`--image-threshold`), max channel delta, bounding box, changed regions, heatmap, side-by-side image, Tesseract OCR, scanned-PDF OCR |
| Audio | MP3 / FLAC / OGG / M4A / WAV / AAC / OPUS / WMA | Duration, bitrate, sample rate, channels and tag metadata (lofty); RMS waveform windows in full mode, capped in concise mode |
| Video / waveform | MP4 / M4V / MKV / WebM / MOV / AVI / WMV / FLV | Container tracks, codecs, dimensions, frame counts, packet counts/bytes and per-track packet hashes; audio-track RMS waveform; full mode shows more track/packet and waveform detail |
| Disk images / packages | SquashFS / Snap / AppImage / Flatpak / DMG / MSIX / AAB / ext4 | SquashFS/Snap/AppImage entry trees, modes and contents; ext4 inodes, modes, owners, xattrs and file data; DMG UDIF/block-map metadata; MSIX manifest/block map; AAB protobuf bundle/manifests; Flatpak ref/commit metadata and payload hash |
| Binary / patches | Any binary / directory | Hex line diff, exact differing byte count, first difference offset, single-file bsdiff, directory ZIP patch, pure-Rust HDIFF13 encode/apply (none/zstd/zlib/bzip2/lzma2), pure-Rust HDIFF19 directory patch encode/apply (fadler64 checksums, same-file dedup, optional compression, hpatchz-compatible) |
| Android | `.apk`, binary `AndroidManifest.xml` | Recursive ZIP plus binary XML string-pool/chunk parsing; semantic element/namespace/attribute/typed-value diff; `--detail full -y` decompiles shared `classes.dex` classes to Java, summarizes large one-sided class sets by descriptor, and disassembles embedded `.so` files; damaged manifests fall back to binary |
| Executables | ELF / PE / Mach-O / COFF | Architecture, entry point, section digest, file offset, symbols, imports; fat Mach-O slices; damaged files fall back to binary; `--detail full` adds normalized disassembly for ELF/PE/Mach-O and `.so` (y/n prompt, `-y` accepts) |
| Other formats | Java class / JAR, WebAssembly, TTF / OTF / TTC / WOFF / WOFF2, Git index, OpenSSH public keys, ICO / CUR, IPK | Class, Wasm, font, Git index and key semantics; WOFF/WOFF2 unpacked to SFNT; damaged inputs fall back to binary |
| Archives / images / packages | ZIP (bzip2/LZMA/zstd), 7Z, TAR, TAR.GZ/BZ2/XZ/ZST/LZ4, CPIO, CAB, XAR, AR, IPK, GZ, ZST, BZ2, XZ, LZ4, DEB, RPM, ISO | Recursive unpacking and per-entry dispatch; DEB (ar) and RPM (header + payload); ISO9660 trees; CAB (MSZIP/LZX), XAR, AR and TAR links; single-file compressed inputs are decompressed and compared |
| Notebooks | `.ipynb` | Cell-level diff, global cell-id alignment, output/execution-count changes, three-way cell merge |
| Merge | Text / notebooks / directories | Three-way merge; clean or conflict exit codes; directory output to a new directory |
| Reports | — | ANSI, interactive TUI, JSON, Markdown, self-contained HTML, unified diff |
| Workflow | — | Exit codes, `--name-only`, `--quiet`, `--summary-only`, format/capability discovery with `--support`, streaming fallback for large files, directory sync, session save/load, external comparator delegation, parallel directory/archive comparison (`--jobs`), terminal progress, packed-application legal prompt, completely-different (>99%) verdict, optional Kaspersky security scan |
| Linked input | HTTP(S) files, Git repository URLs | Download with embedded aria2 (100 MiB confirmation guard, `-y`), compare locally; `--git-history` compares refs, HEAD and commit metadata (up to 2,000 commits); Kaspersky scan runs by default for linked inputs |
| Remote input | WebDAV / HTTP(S) / SFTP / SMB | Download files or recursive directories, then compare locally; URL credentials or `--remote-user` / `--remote-password` |

## Quick Start

Requires Rust 1.88+ (MSRV). The current release/CI toolchain is Rust 1.100.1.

```powershell
cargo build --release -p diff-cli
.\target\release\diff-everything.exe old new
```

Common examples:

```powershell
# Version, codename and build metadata (-V prints the short version only)
.\target\release\diff-everything.exe --version

# List supported formats, or inspect one format's diff capabilities
.\target\release\diff-everything.exe --support
.\target\release\diff-everything.exe --support parquet

# Text
.\target\release\diff-everything.exe fixtures\text\old.txt fixtures\text\new.txt

# Structured data and cross-format comparison
.\target\release\diff-everything.exe fixtures\structured\old.yaml fixtures\structured\new.yaml
.\target\release\diff-everything.exe fixtures\structured\cross.yaml fixtures\structured\cross.json
.\target\release\diff-everything.exe fixtures\plist\old.plist fixtures\plist\old-binary.plist

# Structured documents, slides and spreadsheets
.\target\release\diff-everything.exe fixtures\docs\old.docx fixtures\docs\new.docx
.\target\release\diff-everything.exe fixtures\docs\old.pptx fixtures\docs\new.pptx
.\target\release\diff-everything.exe fixtures\workbook\old.xlsx fixtures\workbook\new.xlsx --compare-formulas

# CSV row alignment, ignored columns and numeric tolerance
.\target\release\diff-everything.exe fixtures\table\old.csv fixtures\table\new.csv --key id
.\target\release\diff-everything.exe old.csv new.csv --ignore-columns notes --numeric-tolerance 0.01

# Syntax token / full syntax tree
.\target\release\diff-everything.exe fixtures\code\old.rs fixtures\code\new.rs --mode token --ignore-comments
.\target\release\diff-everything.exe fixtures\code\old.rs fixtures\code\new.rs --mode tree --ignore-comments

# Images and OCR
.\target\release\diff-everything.exe fixtures\images\old.png fixtures\images\new.png --image-heatmap heatmap.png --image-side-by-side side-by-side.png
.\target\release\diff-everything.exe fixtures\ocr\old.png fixtures\ocr\new.png --ocr

# PDF visual diff and scanned-PDF OCR
.\target\release\diff-everything.exe fixtures\pdf\old.pdf fixtures\pdf\new.pdf --pdf-visual --pdf-output-dir pdf-report
.\target\release\diff-everything.exe fixtures\pdf\old.pdf fixtures\pdf\new.pdf --ocr --pdf-max-pages 5

# Archives and packages
.\target\release\diff-everything.exe fixtures\zip\old.zip fixtures\zip\new.zip
.\target\release\diff-everything.exe fixtures\deb\old.deb fixtures\deb\new.deb
.\target\release\diff-everything.exe fixtures\iso\old.iso fixtures\iso\new.iso

# APK: full mode unpacks the ZIP, decompiles classes.dex and disassembles embedded .so files
.\target\release\diff-everything.exe old.apk new.apk --detail full -y

# Executable code comparison (prompts unless -y is passed)
.\target\release\diff-everything.exe a.so b.so --detail full
.\target\release\diff-everything.exe a.exe b.exe --detail full -y

# Concise (default) and full reports
.\target\release\diff-everything.exe old.txt new.txt --detail concise
.\target\release\diff-everything.exe old.txt new.txt --detail full

# Rich terminal layout (auto on a TTY; force it for captures/logs)
.\target\release\diff-everything.exe old.txt new.txt --style rich
.\target\release\diff-everything.exe old.txt new.txt --style plain

# Override or remove the per-file detailed-diff limit (default 64 MiB)
.\target\release\diff-everything.exe old.iso new.iso --max-file-bytes 536870912
.\target\release\diff-everything.exe old.iso new.iso --no-file-size-limit

# Remove entry limits, or every analysis limit at once
.\target\release\diff-everything.exe old.zip new.zip --no-entry-limit
.\target\release\diff-everything.exe old.iso new.iso --no-limits

# Head / tail / filter
.\target\release\diff-everything.exe old.txt new.txt --head 20
.\target\release\diff-everything.exe old.txt new.txt --last 20
.\target\release\diff-everything.exe left\ right\ --filter "*.json"
.\target\release\diff-everything.exe old.txt new.txt --filter "needle" --context 2

# Three-way merge
.\target\release\diff-everything.exe left.txt right.txt --base base.txt --merge-output merged.txt
.\target\release\diff-everything.exe ours.ipynb theirs.ipynb --base base.ipynb --merge-output merged.ipynb
.\target\release\diff-everything.exe ours-folder theirs-folder --base base-folder --merge-output merged-folder

# Directory sync (plan only unless --sync-apply is used)
.\target\release\diff-everything.exe left-folder right-folder --sync mirror-left-to-right
.\target\release\diff-everything.exe left-folder right-folder --sync mirror-left-to-right --sync-apply --backup-dir backup

# Sessions and reports
.\target\release\diff-everything.exe a.json b.json --save-session session.json
.\target\release\diff-everything.exe --load-session session.json
.\target\release\diff-everything.exe fixtures\dir\left fixtures\dir\right --format html --output report.html

# Patches
.\target\release\diff-everything.exe old.bin new.bin --patch-output change.patch
.\target\release\diff-everything.exe old-folder new-folder --patch-output folder.patch
.\target\release\diff-everything.exe old-folder --apply-patch folder.patch --merge-output patched-folder

# Serialized data
.\target\release\diff-everything.exe old.parquet new.parquet --detail full
.\target\release\diff-everything.exe old.avro new.avro
.\target\release\diff-everything.exe old.orc new.orc
.\target\release\diff-everything.exe old.cbor new.cbor
.\target\release\diff-everything.exe old.msgpack new.msgpack
.\target\release\diff-everything.exe old.pb new.pb

# Open documents and mail
.\target\release\diff-everything.exe old.odt new.odt
.\target\release\diff-everything.exe old.odp new.odp
.\target\release\diff-everything.exe old.epub new.epub
.\target\release\diff-everything.exe old.rtf new.rtf
.\target\release\diff-everything.exe old.eml new.eml

# Disk images and packages
.\target\release\diff-everything.exe old.squashfs new.squashfs
.\target\release\diff-everything.exe old.snap new.snap
.\target\release\diff-everything.exe old.appimage new.appimage
.\target\release\diff-everything.exe old.flatpak new.flatpak
.\target\release\diff-everything.exe old.dmg new.dmg
.\target\release\diff-everything.exe old.msix new.msix
.\target\release\diff-everything.exe old.aab new.aab
.\target\release\diff-everything.exe old.ext4 new.ext4

# Video and audio waveform
.\target\release\diff-everything.exe old.mp4 new.mp4 --detail full
.\target\release\diff-everything.exe old.wav new.wav --detail full

# Parallel comparison with forced progress
.\target\release\diff-everything.exe left\ right\ --jobs 8 --progress

# Kaspersky security scan (auto-detect; /i0 report-only, no disinfection)
.\target\release\diff-everything.exe old new --security-scan
.\target\release\diff-everything.exe old new --security-scan --kaspersky "C:\Program Files (x86)\Kaspersky Lab\Kaspersky 21.27\avp.com"

# Linked files: aria2-backed download, automatic Kaspersky scan, 100 MiB guard
.\target\release\diff-everything.exe https://host/old.zip https://host/new.zip
.\target\release\diff-everything.exe https://host/large-old.bin https://host/large-new.bin -y
.\target\release\diff-everything.exe https://host/old.bin https://host/new.bin --no-security-scan --aria2 C:\tools\aria2c.exe

# Git repository histories (two .git URLs enable this automatically)
.\target\release\diff-everything.exe https://host/owner/old.git https://host/owner/new.git
.\target\release\diff-everything.exe ..\repo old\repo --git-history --detail full

# Remote input
.\target\release\diff-everything.exe webdav://user:pass@host/old.txt https://host/new.txt
.\target\release\diff-everything.exe sftp://user:pass@host:22/old.txt sftp://user:pass@host:22/new.txt
.\target\release\diff-everything.exe smb://user:pass@host:445/share/old.txt smb://host/share/new.txt --remote-user user --remote-password pass
```

`--version` prints the codename plus build metadata; `-V` prints only the first line:

```text
diff-everything 1.3.0 (Toyger)
Commit:     <git-commit>
Build:      release / <target-triple>
Rust:       <rustc-version>
License:    MIT
Repository: https://github.com/ChidcGithub/DiffEverything
```

## Structured Documents, Slides and Spreadsheets

- DOCX is compared by block, not as one flattened text blob: paragraphs, headings, lists, tables, headers/footers, footnotes/endnotes, comments and embedded media each have a stable path such as `body/p[12]` or `body/tbl[1]`. Exact text that moves is reported as `moved`; run formatting and precise paragraph metrics (font family, point size, bold/italic/underline, color, alignment, line spacing, indents) are compared at block level.
- PPTX follows `ppt/presentation.xml` and its relationships, not `slideN.xml` file names. Slides added, removed, reordered or hidden are reported at slide level; text, tables, pictures, notes, shape geometry, layout references and run font metrics (point size, family, bold/italic/underline, color, baseline) are compared inside each slide.
- ODT/ODP use the same block model: paragraphs, headings, tables and slide pages have stable `body/...` or `slide[n]/...` paths, with metadata and embedded media hashes.
- PDF and OFD are page-aligned: `page[n]` blocks prevent an inserted page from making every later page look changed. PDF metadata, page sizes and positioned text blocks are reported. `--pdf-visual` now attaches its rendered-artifact summary to the same `DocumentDiff`/`DiffReport` (ANSI/JSON/HTML) while still writing HTML/PNG/PDF/`report.json` files.
- XLSX/XLS/ODS compare sheet order, renames, hidden sheets/rows/columns, merged ranges, defined names, drawing/chart references, conditional-formatting rules and cached cell values. `--compare-formulas` adds formula text; row insertion/deletion is aligned so later rows do not become thousands of shifted cell changes.
- CSV/TSV uses keyed alignment when `--key` is present and LCS row alignment otherwise. Duplicate keys are reported as `key#2` instead of being overwritten; encoding/BOM detection, `--ignore-columns` and `--numeric-tolerance` are supported.
- HTML reports include side-by-side tables for changed document blocks, slide titles/notes and workbook cells, in addition to the compact change list.
- Any parser failure falls back to the previous text or binary comparison and emits a warning. Document, slide, table and workbook limits propagate to `TRUNCATED` and exit code `3` when the comparison is equal but incomplete.
- To keep the CLI small, document and slide item limits share the existing `--max-entries` / `--no-entry-limit` / `--no-limits` controls instead of adding separate flags.

## CLI Options

| Option | Description |
|---|---|
| **General** | |
| `-h, --help` / `-V, --version` | Grouped help / short version; `--version` adds commit, build, Rust, license and repository metadata |
| `--support [FORMAT]` | List every supported format, or show all diff capabilities for one format |
| **Input and comparison** | |
| `<LEFT>` `<RIGHT>` | Two files or directories; HTTP(S) file links, Git repository URLs, and WebDAV / SFTP / SMB URLs are also accepted |
| `--git-history` | Compare the refs, HEAD and commit metadata of two Git repositories or repository URLs (auto-enabled for two `.git` URLs) |
| `--format <ansi\|tui\|json\|markdown\|html\|unified>` | Output format |
| `--mode <text\|token\|tree\|graph>` | Line text / tree-sitter tokens / full syntax tree / syntax-event graph |
| `--context <N>` | Context lines for text (default 3) |
| `--ignore-comments` | Ignore comments; supported languages use token mode automatically |
| `--ignore-whitespace` / `--ignore-case` | Ignore whitespace / case in text comparison |
| `--key <COLUMN>` | CSV/TSV row alignment key |
| `--ignore-columns <COLUMNS>` | Comma-separated CSV/TSV column names or spreadsheet column letters (for example `B,D`) excluded from comparison |
| `--numeric-tolerance <N>` | Absolute tolerance when both table cells parse as numbers; default `0` (exact) |
| `--compare-formulas` | Compare spreadsheet formula text in addition to cached values |
| `--exclude <GLOB>` | Exclude paths in directory comparisons; repeatable |
| `--image-threshold <0..1>` | YIQ perceptual tolerance for images; higher ignores subtle color changes, default 0 |
| **Output and filtering** | |
| `-o, --output <FILE>` | Write the report to a file |
| `--detail <N\|concise\|full>` | Concise removes equal context and keeps at most N changed units (default 10); numeric values set a custom limit; full keeps the complete report and enables decompilation |
| `--style <auto\|plain\|rich>` | ANSI/TUI layout; `auto` uses rich only on a terminal, `plain` keeps the compact piped layout, `rich` forces the decorated layout |
| `--head <N>` / `--last <N>` | Keep only the first/last N detail units per file or entry |
| `--filter <PATTERN>` | Match paths/labels or content; repeatable, glob supported; text keeps matching lines and `--context` context |
| `--name-only` / `--quiet` / `--summary-only` | Changed paths only / no output / summary and changed paths only |
| `--no-color` | Disable ANSI colors |
| `--progress` / `--no-progress` | Force/hide progress on stderr; auto when stderr is a terminal |
| **Performance** | |
| `--jobs <N>` | Worker threads for directory/archive entries; default 4, `0` = all cores, `1` = sequential |
| `--aria2 <FILE>` | aria2c executable for linked files; Windows uses the embedded build, other platforms use `aria2c` from PATH, then the built-in HTTP client |
| `--download-timeout <SECONDS>` | Timeout for linked downloads and Git clones, default 600 |
| **Security** | |
| `--security-scan` | Scan both inputs with installed Kaspersky before comparing |
| `--no-security-scan` | Skip the automatic Kaspersky scan for linked inputs |
| `--kaspersky <FILE>` | Kaspersky `avp.com` path; auto-detected when omitted |
| `--scan-timeout <SECONDS>` | Kaspersky scan timeout, default 300 |
| **Resource and recursion limits** | |
| `--max-rows <N>` / `--max-table-rows <N>` / `--max-workbook-cells <N>` | Detail rows, table rows and workbook cell limits |
| `--max-file-bytes <BYTES>` / `--no-file-size-limit` | Override or remove the 64 MiB per-file detailed-diff limit; removing it may exhaust memory |
| `--max-entries <N>` / `--no-entry-limit` | Directory/archive/document/slide entry limit, or remove it entirely |
| `--max-expanded-bytes <BYTES>` | Total unpacked bytes (archive entries and decompressed streams, every nesting level, both sides) one comparison may materialize; default 1 GiB |
| `--max-inflight-bytes <BYTES>` | Maximum bytes of file contents held in flight at the same time by the parallel workers; default 256 MiB, `0` disables |
| `--no-limits` | Remove all analysis limits: file size, expanded bytes, in-flight bytes, entries, rows, cells, depth and PDF pages |
| `--max-depth <N>` | Container nesting limit counted from the input root, for directories, archives and compressed streams; default 32 (1..1024) |
| **Merge and sync** | |
| `--base <FILE\|DIR>` | Three-way merge base (a directory base enables directory merge) |
| `--merge-output <FILE\|DIR>` | Merge output; a directory must be empty or new |
| `--sync <MODE>` / `--sync-apply` / `--backup-dir <DIR>` | Directory sync plan / apply / backup before overwrite or delete |
| **Patches** | |
| `--patch-output <FILE>` | Create a single-file bsdiff or directory patch |
| `--apply-patch <FILE>` | Apply a directory patch or HDiffPatch to LEFT, writing to `--merge-output` |
| `--patch-format <bsdiff\|hdiff\|hdiff-rust>` | Patch format; `hdiff` uses hdiffz/hpatchz, `hdiff-rust` uses the pure-Rust HDIFF13/HDIFF19 implementation |
| `--patch-compress <none\|zstd\|zlib\|lzma2\|bzip2>` | Compression for `hdiff-rust` file/directory patches |
| `--hdiffz <FILE>` / `--hpatchz <FILE>` | HDiffPatch tool paths; PATH is used when omitted |
| **Sessions and remote** | |
| `--save-session <FILE>` / `--load-session <FILE>` | Save / load a comparison session |
| `--remote-user` / `--remote-password` | WebDAV / SFTP / SMB credentials |
| `--sftp-known-hosts <FILE>` | Verify SFTP host keys against OpenSSH known_hosts |
| **Documents, images and OCR** | |
| `--image-heatmap <FILE>` / `--image-side-by-side <FILE>` | Image heatmap / side-by-side image |
| `--pdf-visual` / `--pdftoppm <FILE>` / `--pdf-dpi <N>` / `--pdf-max-pages <N>` / `--pdf-output-dir <DIR>` | Rendered PDF visual diff attached to the document report/JSON, with HTML/PNG/PDF artifacts |
| `--ocr` / `--tesseract <FILE>` / `--tessdata <DIR>` / `--ocr-language <LANG>` | Image/PDF OCR and tool configuration |
| **Decompilation and external tools** | |
| `-y, --yes` | Accept decompilation, large-download (>100 MiB) and packed-application prompts |
| `--objdump <FILE>` | Optional external llvm-objdump/objdump; embedded pure-Rust disassemblers are the default |
| `--external-tool <PROGRAM>` / `--external-arg <ARG>` | Delegate comparison to an external program; LEFT/RIGHT are appended |

Filtering affects display only; it does not change comparison status, exit codes or summary counts. `--head` / `--last` mark truncated output with `(truncated)`.

Exit codes: `0` same, `1` differences or merge conflicts, `2` runtime error, `3` indeterminate (nothing differed among the compared part, but a limit omitted part of the input, so equality is not proven).

## Architecture

```text
CLI
 │
 ▼
diff-core
 │
 ├─ detect       extension + magic bytes + text heuristics
 ├─ read         in-memory read / streaming checks for large files
 ├─ normalize    text / JSON tree / pixels / table / workbook / document text / bytes
 ├─ compare      format-specific algorithms
 ├─ model        DiffReport → DiffNode tree → DiffBody
 └─ render       ANSI / JSON / Markdown / HTML / unified
```

The core model is a `DiffNode` tree: directories and archives are containers; text, structured data, tables, workbooks, images and binaries are leaves. Renderers consume only the model. Module details and format-extension steps are in [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## References

| Project | Borrowed idea | Implementation |
|---|---|---|
| [diffoscope](https://diffoscope.org) | Recursive unpacking, comparator/presenter layering, HTML/JSON reports | Recursive ZIP/TAR/CAB/XAR/AR, PDF/Office text, SQLite/ELF/font semantics, multi-format reports, resource limits |
| [Graphtage](https://github.com/trailofbits/graphtage) | Semantic JSON/YAML/TOML/INI/XML/CSV diff, cross-format comparison | Structured semantic diff, array similarity alignment, `YAML+JSON`, CSV table diff |
| [difftastic](https://github.com/Wilfred/difftastic) | tree-sitter parsing, comment ignoring, syntax-tree comparison | 13-language token diff and full syntax-tree diff (content signatures, Hungarian matching, order sliding, Myers fallback) |
| [Open Diff](https://github.com/kygo8/open-diff) | Text/table/image/hex/directory diff, merge, sync, sessions, HTML | Corresponding CLI features, bsdiff, heatmap/side-by-side, sync and sessions |
| [uni-diff](https://github.com/neardws/uni-diff) | PDF/DOCX/XLSX/PPTX, OCR, TUI, multiple outputs, block diff | Document formats, OCR, TUI, ANSI/JSON/Markdown/HTML, block hunks |
| [diff-pdf](https://github.com/vslavik/diff-pdf) | PDF visual comparison | Text layer, page count, poppler rendering, visual reports |
| [nbdime](https://github.com/jupyter/nbdime) | Notebook diff and merge | Cell-level diff, outputs/execution counts, three-way cell merge |
| [HDiffPatch](https://github.com/sisong/HDiffPatch) | Binary and directory diff/patch | bsdiff, directory ZIP patch, pure-Rust HDIFF13/HDIFF19 encode/apply, hdiffz/hpatchz integration |
| [gocompare](https://github.com/jhzhang09/gocompare) | Directory/table/image/merge/sync | Directory, table, image and three-way merge/sync; WebDAV/SFTP/SMB input added |

Twenty reference repositories were reviewed in total. Licenses and detailed mapping are in [docs/REFERENCES.md](docs/REFERENCES.md); per-feature evidence is in [docs/FEATURE-MATRIX.md](docs/FEATURE-MATRIX.md).

## Testing

```powershell
# 382 Rust tests (unit + integration + CLI)
cargo test --workspace

# Strict lint
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# Optimized shipping binary (also what `cargo test --release` builds against)
cargo build --release -p diff-cli
```

The release was also validated with 189 end-to-end smoke tests and real external fixtures. Maintainer fixture, smoke-test and release scripts are kept local and are not part of the public repository.

Coverage includes every leaf format and key path/cell/line assertions, invalid-input fallbacks, UTF-16 and large-file paths, archive limits, CLI exit codes and report validity, image reports, three-way merges, bsdiff and HDIFF13/HDIFF19 patches, syntax-tree diff, sync/session flows, notebook merges, remote and linked inputs, Git-history comparison, OCR/PDF visual reports, OpenAPI breaking changes, external comparators, and real downloaded PDF/Office/archive fixtures. Full results are in [docs/VERIFICATION.md](docs/VERIFICATION.md).

## Terminal UI, Parallelism, Progress and Security

- ANSI reports use a decorated layout automatically when stdout is a terminal: a labeled banner, aligned inputs, a colored status badge, summary/elapsed fields, section rules, styled markers and line-number gutters. `--style rich` forces it for redirected output; `--style plain` keeps the compact layout for scripts and logs. `--no-color` removes colors but keeps the rich layout.
- CLI help is grouped by task (`Comparison`, `Output & display`, `Progress`, `Limits`, `Images`, `PDF & OCR`, `Merge & sync`, `Patches`, `Sessions & remote`, `Performance`, `Security`, `Approvals & decompilation`, `External tools`), wraps to the terminal width, and ends with common examples and exit codes.
- Runtime errors use a structured layout: missing inputs show the left/right path and a spelling hint; other failures list a concise cause chain with targeted hints. Colors follow `--no-color` and `NO_COLOR`.
- Directory and archive entries are compared in parallel with rayon. `--jobs` defaults to `4`, `--jobs 0` uses all cores, `--jobs 1` is sequential, and larger values cap workers. Results are collected in path order, so reports remain deterministic.
- Progress is written to stderr, never to the report. It uses a spinner plus a `[####----]` bar, item counts and percentage. It is enabled automatically when stderr is a terminal; `--progress` forces it and `--no-progress` (or `--quiet`) disables it.
- Example terminal layout (colors omitted):

```text
DiffEverything 1.3.0 (Toyger)
========================================================================
left     fixtures\text\old.txt  [text | 105 B]
right    fixtures\text\new.txt  [text | 160 B]
status   [DIFFERENT]
summary  1 files/entries compared | 1 changed | +3 -1 lines
elapsed  7 ms

-- Changes -------------------------------------------------------------
~ text: +3 -1
  @@ -2,1 +2,3 @@
    2     | -- old text
        2 | +- new text
```

- Packed/protected executables and APKs are detected before comparison from executable section names and APK shell-library entries (UPX, ASPack, Themida, VMProtect, Enigma, MPRESS, PECompact, NSPack, Armadillo, Obsidium, PELock, tElock, MoleBox, BoxedApp, 360 Jiagu, Tencent Legu, Bangcle, Alibaba, Baidu, Naqu and NetEase shells). Raw strings inside the binary are not treated as evidence, so the tool's own signature table cannot trigger a false positive. The CLI lists the detected packer and warns that analyzing or unpacking it may violate its license or applicable law, then asks whether to continue. `-y` accepts; non-interactive runs without `-y` stop with exit code `2`.
- When more than 99% of a single file or top-level container changed, the report adds a `COMPLETELY DIFFERENT` verdict. Text and binary files use `(changed bytes / left bytes + changed bytes / right bytes) / 2`; images, tables, workbooks, notebooks, audio and containers use semantic ratios (pixels, rows, cells, windows or entries). JSON exposes it as `completely_different`.
- `--security-scan` uses only Kaspersky's local `avp.com` interface. Common installation paths are auto-detected; `--kaspersky <FILE>` overrides detection. Scans use `/i0` (report only), so files are never disinfected or deleted. Detected threats abort with exit code `2`; a missing Kaspersky installation or stopped service produces a warning and the diff continues. `--scan-timeout` bounds each scan.
- Linked inputs (HTTP(S) files and Git repository URLs) are scanned automatically with the same local Kaspersky interface. `--no-security-scan` disables this default; `--security-scan` still enables it manually for any comparison.

## Linked Files and Git History

- Any HTTP(S) URL can be used as LEFT or RIGHT. Files are downloaded into a temporary directory and compared locally; report paths keep the original URL.
- Windows builds embed aria2 1.37.0 and extract it to the temporary directory on first use. Other platforms use `aria2c` from PATH; if it is unavailable, the built-in HTTP client is used. `--aria2 <FILE>` selects a specific aria2c.
- A download whose declared size exceeds 100 MiB prints a warning and asks for confirmation. `-y` / `--yes` skips the prompt; non-interactive runs without `-y` stop with an error. `--download-timeout` bounds each download and clone.
- `--git-history` compares repository refs, HEAD, commit count and commit metadata (SHA, author, email, date, subject). Up to 2,000 commits are included per side; older history is summarized in a warning.
- Remote Git repositories are cloned as bare mirrors into the temporary directory. The clone first requests a blob-filtered mirror and retries a full mirror when the server does not support partial clones.
- Two `.git` URLs (HTTP(S), `file://`, `git://`, `ssh://` or `git@host:path`) enable Git-history mode automatically. One repository URL mixed with a local path requires `--git-history`.
- Linked inputs run the Kaspersky scan by default. Use `--no-security-scan` for an offline or non-Windows environment that has no Kaspersky installation.

## Performance, Size and Hardening

- The 1.3.0 engine avoids repeated parsing: PDF text, positioned blocks, metadata and page geometry come from one `lopdf::Document` per side; DOCX/PPTX/ODT/ODP and XLSX metadata share one open ZIP `Package` per side; left and right document/workbook parsing runs through `rayon::join`. The PDF path decodes each page content stream once and derives both the text layer and positioned blocks from it; Office/OpenDocument media hashes stream through one reused 64 KiB buffer. Measured on the benchmark corpus, a 1000-page PDF is 9.3× faster, multipart DOCX 4.0×, multipart ODT 3.6×, PPTX 2.4× and XLSX 1.57×. See [docs/PERFORMANCE.md](docs/PERFORMANCE.md).
- Text comparison strips common line prefixes/suffixes before running Myers/patience, switches to patience for large changed middles, stops building hunks at the detail cap, skips word-level spans above a threshold, and avoids a full normalization copy when `--ignore-whitespace`/`--ignore-case` are off. On a 49 MB file with ten changed lines this is about 10× faster with 47% lower peak RSS, and memory is 80% lower on the 4.8 MB variant.
- Structured comparison returns immediately for equal subtrees, trims equal array edges, and aligns large arrays on stable 128-bit content fingerprints rather than serialising every value. The 50k-record JSON benchmark is about 1.46× faster with 13% lower peak RSS.
- Directories and archives pair unmatched entries by `(size, SHA-256)` and report `renamed_from`, so a rename is one moved entry instead of add+remove. Only unmatched entries are hashed, directory hashes hold an in-flight permit sized to the file, duplicate content is paired stably, and explicit CSV keys still win over implicit keys. Rename detection never changes `completeness` or exit codes.
- ZIP comparison scans the central directory once, keeps only entry metadata, pre-checks unpacked sizes, reads payloads on demand, and materialises changed pairs in bounded 32 MiB batches. On the 10k-entry / 82 MB ZIP case this cuts peak RSS by 89% and is about 1.66× faster. Uncompressed TAR, CPIO and AR/DEB use the same catalog-plus-byte-range approach; 10k-entry TAR drops peak RSS by 48% and is 1.36× faster. Compressed TAR, 7z, RPM, CAB, ISO and XAR still materialise entries.
- `--detail concise` is enforced while comparing, not only after the full report is built: text hunks, table rows, workbook cells, document blocks and slides stop at the display cap (default 10, configurable with `--detail <N>`). The cap is display-only; it does not change `completeness` or turn an equal comparison into exit code `3`.
- `cargo build --release` is the only optimized profile: `opt-level = 3`, thin LTO, four codegen units and symbol stripping. Thin LTO keeps the binary close to a fat-LTO build while building roughly three times faster, and `cargo test --release` shares the same dependency artifacts, so the tested and the shipped binary come from one profile.
- `crates/diff-core/tests/integration.rs` is the single integration-test target; the six case modules are included from `tests/cases/` and no longer produce six separate link steps.
- Unit tests live outside `src/` in `tests/unit/<source path>.rs`; each source file mounts them with `#[cfg(test)] #[path]`, keeping private access without test code in the source files.
- Every comparison carries a shared `ResourceBudget`: archive readers and decompressors charge the bytes they unpack atomically, so rayon workers cannot race past the cap, and the budget latches when spent. Containers that omitted entries (archive entry caps, directory scan stop, depth limit, budget exhaustion) propagate a `ComparisonCompleteness` of `Truncated` up the `DiffNode` tree into `DiffReport.completeness`; the CLI then exits `3` instead of `0` when nothing differed but the input was not fully compared.
- The scheduler bounds the parallel work set instead of only the thread count: every file task acquires its footprint (left + right size) from an in-flight gate (`--max-inflight-bytes`, default 256 MiB) before its contents are read and releases it when the node is built, so more workers means more small tasks in flight, never more bytes; a task larger than the budget is clamped and runs alone. Finished entry results are aggregated chunk by chunk instead of being collected whole first.
- Format comparison is dispatched through a parser registry: every `FileKind` maps to a `FormatParser` (text, structured, notebook, sqlite, executable, dex, font, table, image, compression, archive, document, workbook, binary). Parsers charge unpacked bytes through the shared context, so a new format plugs in without touching the dispatcher or the resource model.
- The embedded Windows aria2 executable is stored zstd-compressed in the binary (2.07 MiB instead of 5.65 MiB) and decompressed to the temporary directory on first use. Raw binary SHA-256 is recorded in `THIRD_PARTY_LICENSES.md`.
- Directory and archive entry comparison shares one `Arc<GlobSet>` instead of cloning the matcher per entry, merges sorted key sets linearly, avoids a per-entry result-key clone, and skips progress-message formatting when progress is disabled.
- Every archive reader reads entries through one shared bounded read (`limit + 1` bytes), including CAB; XAR validates its declared table-of-contents size before allocating and bounds TOC and payload decoding; ZIP LZMA bounds the compressed stream as well as its output. `--max-depth` counts container levels from the input root: leaves compare at `depth <= max_depth`, containers expand only below it, and a directory at the limit is reported as one `directory depth limit reached` node instead of being scanned.
- Both crates use `#![forbid(unsafe_code)]`.
- ANSI reports replace C0/C1 control characters (except tab) in untrusted paths and content, preventing terminal escape injection from malicious file contents or archive entry names.
- The TUI restores cursor, alternate-screen and raw-mode state through an RAII guard, including early errors and unwinding panics.
- Kaspersky report parsing reads at most 4 MiB from the temporary report file.

## Limitations

- PDF uses the text layer by default; `--pdf-visual` requires poppler `pdftoppm`, and `--ocr` requires Tesseract. Portable builds can be installed separately.
- PDF visual output includes `report.pdf` (side-by-side + heatmap), `report-overlay.pdf` (raster red overlay), `report-vector.pdf` (vector rectangles appended to page content), `report.json` (machine-readable page summary), plus HTML/PNG; `report.html` embeds base64 PNGs and is self-contained.
- WebDAV / HTTP(S) / SFTP / SMB support file input; WebDAV / SFTP / SMB support recursive directories.
- aria2 is embedded only in Windows builds. The executable is stored zstd-compressed in the binary and extracted to the temporary directory on first use; Linux and macOS use `aria2c` from PATH when present and otherwise fall back to the built-in HTTP downloader.
- Git history requires `git` on PATH. Remote histories are capped at 2,000 commits per side, and refs/commit metadata are compared rather than every commit patch.
- RPM supports gzip/xz/zstd payloads; ZIP supports bzip2, LZMA (method 14) and zstd entries.
- `--sync` only applies to local directories; remote inputs are rejected to avoid modifying temporary copies.
- `--external-tool` receives resolved local paths; remote inputs are downloaded to a temporary directory first.
- SFTP does not verify host fingerprints by default. Use `--sftp-known-hosts <FILE>` in production; missing or mismatched keys are rejected.
- Legacy `.doc` / `.ppt` fall back to binary; `.xls` / `.ods` are supported by calamine.
- DOCX/PPTX comparison covers text, paragraph/table structure, media hashes, run formatting and precise font/paragraph metrics (family, point size, bold/italic/underline, color, baseline, line spacing, indents), shape geometry, layout relationships and hidden-slide state. Pixel-accurate rendered layout is not compared.
- XLSX/ODS comparison covers sheet order, visibility, values, formulas, row alignment, merged ranges, defined names, hidden dimensions, drawing references, chart XML hashes and conditional-formatting rule attributes/formulas. Differential-style (`dxf`) font/fill/border attributes are expanded into the rule details.
- PDF/OFD comparison is page- and text-block-aligned and includes PDF metadata, page sizes and positioned text blocks; embedded PDF font metrics/rendered layout are not compared. `--pdf-visual` artifacts are summarized inside the document report and JSON, and remain available as separate HTML/PNG/PDF/`report.json` files.
- Cross-format PDF↔DOCX/ODT/PPTX/ODP and DOCX↔ODT/PPTX↔ODP comparisons use normalized block paths where both sides expose them; remaining mixed document pairs (for example PDF vs EPUB) still use extracted text with a type-mismatch warning.
- Default limits are 64 MiB per file, 10,000 directory entries and 2,000 archive entries; all are configurable. `--no-file-size-limit` removes the per-file limit, `--no-entry-limit` removes directory/archive entry caps, and `--no-limits` removes all analysis limits. These flags can consume a lot of memory or time on huge inputs.
- Unpacked data is additionally bounded by a shared expanded-bytes budget (default 1 GiB): every archive reader and decompressor charges the bytes it materializes across both sides and all nesting levels, and once the budget is spent the remaining entries are skipped and the report is marked truncated. Directory scans also stop at the entry limit instead of enumerating trees that cannot be compared anyway.
- Encrypted ZIPs and archives with unsupported compression fall back to binary with a warning.

## License

MIT. See [LICENSE](LICENSE) and [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md) for the project license and third-party notices. The Windows build bundles the unmodified GPL-2.0-or-later aria2 1.37.0 executable and invokes it as a separate process; its GPL text, OpenSSL notice and authors are reproduced under `crates/diff-cli/assets/` and in `THIRD_PARTY_LICENSES.md`. Reference projects were used to study algorithms and design; GPL/LGPL code from diffoscope, diff-pdf, Graphtage and others was not copied.
