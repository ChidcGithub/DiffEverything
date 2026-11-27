# DiffEverything

A Rust CLI that compares heterogeneous files by normalizing them into one diff tree. It does not replace `diff`; it adds format-aware comparison and multiple report formats.

Current version: **v1.0-4596984fe5db**. The design draws on diffoscope, Graphtage, difftastic, Open Diff, uni-diff, diff-pdf, nbdime, HDiffPatch and others. See [docs/REFERENCES.md](docs/REFERENCES.md).

## Features

| Area | Formats | Capabilities |
|---|---|---|
| Text / code | Any text, UTF-8 / UTF-16 / GBK, `.reg` exports | Line diff, context, word-level highlights, ignore whitespace / case |
| Syntax-aware | Rust, Python, JS/JSX, TS/TSX, C, C++, Go, Java, Bash, Ruby, C#, Swift, Lua | tree-sitter token diff, full syntax-tree diff (`--mode tree`), bounded syntax-event graph diff (`--mode graph`, including non-local moves), comment ignoring |
| Structured data | JSON / JSON5 / YAML / TOML / INI / XML / HTML / Plist (XML + binary) / Pickle / Gettext (.mo) / OpenAPI | Semantic tree diff with path-level added / removed / changed, array similarity alignment, OpenAPI breaking-change markers |
| Databases | SQLite 3 `.db` / `.sqlite` | Semantic diff of tables, columns, rows, indexes and triggers; equal databases with different page sizes or layouts compare equal |
| Cross-format | JSON ↔ YAML ↔ TOML ↔ INI ↔ XML ↔ HTML ↔ Plist ↔ Pickle | Parse both sides into a common tree, e.g. `YAML+JSON`, `PLIST+JSON`, `PICKLE+JSON` |
| Tables | CSV / TSV | Header, row and cell diff; align rows with `--key` |
| Spreadsheets | XLSX / XLS / ODS | Worksheet and cell diff (calamine) |
| Documents | PDF / OFD / DOCX / PPTX | PDF text layer, page count, page-level text blocks and rendered visual diff (HTML/PNG/PDF reports); OFD page text; DOCX body/headers/footers; PPTX slides/notes |
| Images / OCR | PNG / JPEG / GIF / WebP / BMP / TIFF / ICO / CUR | Pixel difference ratio, YIQ perceptual tolerance (`--image-threshold`), max channel delta, bounding box, changed regions, heatmap, side-by-side image, Tesseract OCR, scanned-PDF OCR |
| Audio | MP3 / FLAC / OGG / M4A / WAV / AAC / OPUS / WMA | Duration, bitrate, sample rate, channels and tag metadata (lofty) |
| Binary / patches | Any binary / directory | Hex line diff, exact differing byte count, first difference offset, single-file bsdiff, directory ZIP patch, pure-Rust HDIFF13 encode/apply (none/zstd/zlib/bzip2/lzma2), pure-Rust HDIFF19 directory patch encode/apply (fadler64 checksums, same-file dedup, optional compression, hpatchz-compatible) |
| Android | `.apk`, binary `AndroidManifest.xml` | Recursive ZIP plus binary XML string-pool/chunk parsing; semantic element/namespace/attribute/typed-value diff; `--detail full -y` decompiles `classes.dex` to Java and disassembles embedded `.so` files; damaged manifests fall back to binary |
| Executables | ELF / PE / Mach-O / COFF | Architecture, entry point, section digest, file offset, symbols, imports; fat Mach-O slices; damaged files fall back to binary; `--detail full` adds normalized disassembly for ELF/PE/Mach-O and `.so` (y/n prompt, `-y` accepts) |
| Other formats | Java class / JAR, WebAssembly, TTF / OTF / TTC / WOFF / WOFF2, Git index, OpenSSH public keys, ICO / CUR, IPK | Class, Wasm, font, Git index and key semantics; WOFF/WOFF2 unpacked to SFNT; damaged inputs fall back to binary |
| Archives / images / packages | ZIP (bzip2/LZMA/zstd), 7Z, TAR, TAR.GZ/BZ2/XZ/ZST/LZ4, CPIO, CAB, XAR, AR, IPK, GZ, ZST, BZ2, XZ, LZ4, DEB, RPM, ISO | Recursive unpacking and per-entry dispatch; DEB (ar) and RPM (header + payload); ISO9660 trees; CAB (MSZIP/LZX), XAR, AR and TAR links; single-file compressed inputs are decompressed and compared |
| Notebooks | `.ipynb` | Cell-level diff, global cell-id alignment, output/execution-count changes, three-way cell merge |
| Merge | Text / notebooks / directories | Three-way merge; clean or conflict exit codes; directory output to a new directory |
| Reports | — | ANSI, interactive TUI, JSON, Markdown, self-contained HTML, unified diff |
| Workflow | — | Exit codes, `--name-only`, `--quiet`, `--summary-only`, streaming fallback for large files, directory sync, session save/load, external comparator delegation |
| Remote input | WebDAV / HTTP(S) / SFTP / SMB | Download files or recursive directories, then compare locally; URL credentials or `--remote-user` / `--remote-password` |

## Quick Start

Requires Rust 1.88+.

```powershell
cargo build --release -p diff-cli
.\target\release\diff-everything.exe old new
```

Common examples:

```powershell
# Text
.\target\release\diff-everything.exe fixtures\text\old.txt fixtures\text\new.txt

# Structured data and cross-format comparison
.\target\release\diff-everything.exe fixtures\structured\old.yaml fixtures\structured\new.yaml
.\target\release\diff-everything.exe fixtures\structured\cross.yaml fixtures\structured\cross.json
.\target\release\diff-everything.exe fixtures\plist\old.plist fixtures\plist\old-binary.plist

# CSV row alignment
.\target\release\diff-everything.exe fixtures\table\old.csv fixtures\table\new.csv --key id

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

# Remote input
.\target\release\diff-everything.exe webdav://user:pass@host/old.txt https://host/new.txt
.\target\release\diff-everything.exe sftp://user:pass@host:22/old.txt sftp://user:pass@host:22/new.txt
.\target\release\diff-everything.exe smb://user:pass@host:445/share/old.txt smb://host/share/new.txt --remote-user user --remote-password pass
```

## CLI Options

| Option | Description |
|---|---|
| **Input and comparison** | |
| `<LEFT>` `<RIGHT>` | Two files or directories; WebDAV / HTTP(S) / SFTP / SMB URLs are also accepted |
| `--format <ansi\|tui\|json\|markdown\|html\|unified>` | Output format |
| `--mode <text\|token\|tree\|graph>` | Line text / tree-sitter tokens / full syntax tree / syntax-event graph |
| `--context <N>` | Context lines for text (default 3) |
| `--ignore-comments` | Ignore comments; supported languages use token mode automatically |
| `--ignore-whitespace` / `--ignore-case` | Ignore whitespace / case in text comparison |
| `--key <COLUMN>` | CSV/TSV row alignment key |
| `--exclude <GLOB>` | Exclude paths in directory comparisons; repeatable |
| `--image-threshold <0..1>` | YIQ perceptual tolerance for images; higher ignores subtle color changes, default 0 |
| **Output and filtering** | |
| `-o, --output <FILE>` | Write the report to a file |
| `--detail <concise\|full>` | Concise removes equal context and limits detail; full keeps the complete report and enables decompilation; default `concise` |
| `--head <N>` / `--last <N>` | Keep only the first/last N detail units per file or entry |
| `--filter <PATTERN>` | Match paths/labels or content; repeatable, glob supported; text keeps matching lines and `--context` context |
| `--name-only` / `--quiet` / `--summary-only` | Changed paths only / no output / summary and changed paths only |
| `--no-color` | Disable ANSI colors |
| **Resource and recursion limits** | |
| `--max-rows <N>` / `--max-table-rows <N>` / `--max-workbook-cells <N>` | Detail rows, table rows and workbook cell limits |
| `--max-file-bytes <N>` / `--max-entries <N>` | Single-file byte limit / directory and archive entry limit |
| `--max-depth <N>` | Recursive directory/archive depth limit, default 32 (1..1024) |
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
| `--pdf-visual` / `--pdftoppm <FILE>` / `--pdf-dpi <N>` / `--pdf-max-pages <N>` / `--pdf-output-dir <DIR>` | Rendered PDF visual diff |
| `--ocr` / `--tesseract <FILE>` / `--tessdata <DIR>` / `--ocr-language <LANG>` | Image/PDF OCR and tool configuration |
| **Decompilation and external tools** | |
| `-y, --yes` | Accept decompilation prompts in full mode |
| `--objdump <FILE>` | Optional external llvm-objdump/objdump; embedded pure-Rust disassemblers are the default |
| `--external-tool <PROGRAM>` / `--external-arg <ARG>` | Delegate comparison to an external program; LEFT/RIGHT are appended |

Filtering affects display only; it does not change comparison status, exit codes or summary counts. `--head` / `--last` mark truncated output with `(truncated)`.

Exit codes: `0` same, `1` differences or merge conflicts, `2` runtime error.

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
# 279 Rust tests (unit + integration + CLI)
cargo test --workspace

# Strict lint
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings

# Download external fixtures
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\fetch-testdata.ps1

# Regenerate SQLite / ELF fixtures
python scripts\make-fixtures.py

# 189 end-to-end smoke tests
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\smoke-test.ps1

# fmt + clippy + tests + release build + smoke
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\verify-all.ps1
```

Coverage includes every leaf format and key path/cell/line assertions, invalid-input fallbacks, UTF-16 and large-file paths, archive limits, CLI exit codes and report validity, image reports, three-way merges, bsdiff and HDIFF13/HDIFF19 patches, syntax-tree diff, sync/session flows, notebook merges, remote inputs, OCR/PDF visual reports, OpenAPI breaking changes, external comparators, and real downloaded PDF/Office/archive fixtures. Full results are in [docs/VERIFICATION.md](docs/VERIFICATION.md).

## Limitations

- PDF uses the text layer by default; `--pdf-visual` requires poppler `pdftoppm`, and `--ocr` requires Tesseract. Portable builds can be downloaded by the scripts.
- PDF visual output includes `report.pdf` (side-by-side + heatmap), `report-overlay.pdf` (raster red overlay), `report-vector.pdf` (vector rectangles appended to page content), plus HTML/PNG; `report.html` embeds base64 PNGs and is self-contained.
- WebDAV / HTTP(S) / SFTP / SMB support file input; WebDAV / SFTP / SMB support recursive directories.
- RPM supports gzip/xz/zstd payloads; ZIP supports bzip2, LZMA (method 14) and zstd entries.
- `--sync` only applies to local directories; remote inputs are rejected to avoid modifying temporary copies.
- `--external-tool` receives resolved local paths; remote inputs are downloaded to a temporary directory first.
- SFTP does not verify host fingerprints by default. Use `--sftp-known-hosts <FILE>` in production; missing or mismatched keys are rejected.
- Legacy `.doc` / `.ppt` fall back to binary; `.xls` / `.ods` are supported by calamine.
- Default limits are 64 MiB per file, 10,000 directory entries and 2,000 archive entries; all are configurable.
- Encrypted ZIPs and archives with unsupported compression fall back to binary with a warning.

## License

MIT. Reference projects were used to study algorithms and design; GPL/LGPL code from diffoscope, diff-pdf, Graphtage and others was not copied.
