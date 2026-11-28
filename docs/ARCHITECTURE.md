# DiffEverything Architecture

## Modules

```text
crates/
├─ diff-core/
│  ├─ src/detect.rs          extension + magic bytes + text heuristics
│  ├─ src/model.rs           report model (DiffReport / DiffNode / DiffBody)
│  ├─ src/engine.rs          orchestration: read, dispatch, recursion, stats, limits
│  ├─ src/merge.rs           three-way text merge
│  ├─ src/patch.rs           bsdiff patch creation/apply
│  ├─ src/image_export.rs    heatmaps and side-by-side images
│  ├─ src/ocr.rs             Tesseract image/PDF OCR
│  ├─ src/pdf_visual.rs      poppler page rendering, per-page visual diff, HTML/PDF reports
│  ├─ src/progress.rs        thread-safe stderr progress sink
│  ├─ src/security.rs        Kaspersky avp.com detection, scanning and report parsing
│  ├─ src/packers.rs         packed executable/APK detection and legal-prompt metadata
│  ├─ src/session.rs         session save/load
│  ├─ src/sync.rs            directory sync plan/apply/backup
│  ├─ src/diff/
│  │  ├─ text.rs             decoding, line diff, word-level spans, ignore whitespace/case
│  │  ├─ tokens.rs           tree-sitter token diff
│  │  ├─ tree_diff.rs        recursive tree-sitter syntax-tree node diff
│  │  ├─ notebook.rs         cell-level diff / three-way merge
│  │  ├─ structured.rs       semantic JSON/JSON5/YAML/TOML/INI/XML/HTML diff
│  │  ├─ table.rs            CSV/TSV row alignment and table diff
│  │  ├─ workbook.rs         XLSX/XLS/ODS sheet, formula and cell diff
│  │  ├─ package.rs          one reusable open ZIP per Office/OpenDocument side
│  │  ├─ documents.rs        PDF/OFD/DOCX/PPTX page, block and shape diff
│  │  ├─ documents_extra.rs  ODT/ODP/EPUB/RTF/EML text and metadata
│  │  ├─ image.rs            pixel-level diff
│  │  ├─ media.rs            video metadata/packet hashes and audio RMS waveform
│  │  ├─ packages.rs         SquashFS/Snap/AppImage/Flatpak/DMG/MSIX/AAB/ext4
│  │  ├─ serialization.rs    CBOR/MessagePack/generic protobuf wire decoding
│  │  ├─ tabular.rs          Parquet/Avro/ORC schema and record comparison
│  │  └─ binary.rs           hex / byte-level diff
│  ├─ src/render/
│  │  ├─ ansi.rs             terminal report
│  │  ├─ json.rs             JSON report
│  │  ├─ markdown.rs         Markdown report
│  │  ├─ html.rs             self-contained HTML report
│  │  └─ unified.rs          GNU unified diff
│  └─ tests/
│     ├─ integration.rs      single integration-test target
│     ├─ cases/              engine / formats / robustness / merge / patch cases
│     └─ unit/               unit tests mounted by each source module via #[path]
└─ diff-cli/
   ├─ src/main.rs            argument parsing, exit codes, merge/patch/image branches
   ├─ src/error_report.rs    structured runtime errors, IO translation and hints
   ├─ src/link.rs            aria2/built-in HTTP downloads, 100 MiB guard, Git history
   ├─ src/remote.rs          WebDAV / SFTP / SMB remote inputs
   ├─ src/support.rs         supported-format and per-format capability catalog
   ├─ src/tui.rs             interactive ANSI viewer
   ├─ tests/cli.rs           CLI end-to-end tests
   └─ tests/unit/            unit tests mounted by each source module via #[path]
```

## Data Flow

```text
Path / bytes
    │
    ▼
detect_file / detect_bytes        FileKind
    │
    ▼
read + size guard                 Vec<u8> / streaming
    │
    ▼
normalize                         text / JSON tree / pixels / table / workbook / document pages, blocks and slides / bytes
    │
    ▼
compare                           DiffBody
    │
    ▼
DiffNode tree + Summary
    │
    ▼
renderer                          ANSI / JSON / Markdown / HTML / unified
```

The core principle is the same as diffoscope: **detect → normalize → compare → present**. Separating format parsing from diff presentation makes it possible to add formats without changing renderers.

## Core Model

`DiffReport` is the only input to renderers:

```rust
pub struct DiffReport {
    pub tool: &'static str,
    pub version: &'static str,
    pub left: ArtifactInfo,
    pub right: ArtifactInfo,
    pub status: DiffStatus,
    pub summary: Summary,
    pub root: DiffNode,
    pub warnings: Vec<String>,
    pub elapsed_ms: u128,
}
```

`DiffNode` is a recursive tree:

```rust
pub struct DiffNode {
    pub label: String,      // relative path inside a directory/archive; empty for the root
    pub status: DiffStatus,
    pub body: DiffBody,
}
```

`DiffBody` variants:

| Variant | Meaning |
|---|---|
| `Text(TextDiff)` | Text/document/OCR line diff, including inline `InlineSpan` |
| `Tokens(TokenDiff)` | tree-sitter token sequence diff |
| `Tree(TreeDiff)` | tree-sitter syntax-tree node diff |
| `Notebook(NotebookDiff)` | Notebook cell-level diff / moves |
| `Audio(AudioDiff)` | Audio properties and tags |
| `Structured(StructuredDiff)` | Path-level JSON/YAML/TOML/INI/XML diff |
| `Document(DocumentDiff)` | Page/block/table/media diff for PDF, OFD, DOCX |
| `Presentation(PresentationDiff)` | Slide/shape/table/media/notes diff for PPTX |
| `Table(TableDiff)` | CSV/TSV row/column/cell diff with moves and duplicate keys |
| `Workbook(WorkbookDiff)` | Sheet/cell/value/formula/visibility diff |
| `Image(ImageDiff)` | Pixel statistics and bounding boxes |
| `Binary(BinaryDiff)` | Byte statistics and hex rows |
| `Directory(DirectoryDiff)` | Directory container |
| `Archive(ArchiveDiff)` | ZIP/TAR container |
| `OneSided` | Exists on one side only |
| `Equal` | Identical |
| `Unsupported` | Not supported |

Design constraints:

- Renderers do not read files or detect types.
- Counts (lines, bytes, pixels, cells) are calculated during comparison.
- Container status is aggregated from child nodes.
- Warnings live at the top level of the report to avoid duplication.
- All formats share size, recursion and entry limits.

## Adding a Format

Using 7z as an example:

1. `detect.rs`: extension / magic → `FileKind::SevenZip`.
2. `diff/`: add a parser that turns 7z into `BTreeMap<String, Vec<u8>>`.
3. `engine.rs`: call the shared `compare_archive_entries` from `compare_bytes_internal`.
4. `model.rs`: reuse `ArchiveDiff`; no new `DiffBody` variant is needed.
5. Renderers: reuse `Archive`; no changes are needed.
6. Tests: entry differences, corrupted archive fallback, entry limits.

For a new leaf format (for example OCR text), reuse `TextDiff` / `StructuredDiff` instead of rewriting rendering logic for every format.

## Text Diff

1. `decode_text`: BOM → UTF-8 → chardetng statistical detection (UTF-16/GBK supported).
2. `compare_text_options`:
   - optionally ignore case and whitespace;
   - match using normalized text;
   - restore original lines when rendering so normalized content is not displayed.
3. `annotate_word_changes`: word-level diff over consecutive deleted/inserted lines, producing `InlineSpan`.
4. Output preserves hunk line numbers and context.

## Syntax Token / Tree Diff

- Select a tree-sitter `Language` by file extension (13 languages: Rust/Python/JS/TS/C/C++/Go/Java/Bash/Ruby/C#/Swift/Lua).
- Token mode: recursively collect leaf nodes, compare token sequences with Myers, emit `TokenChunk`.
- Tree mode: recursively build `SyntaxNode`, align by node kind/child signatures, emit added/removed/modified nodes.
- Notebook: align by cell id/signature, detect moved cells, and align three-way merges by id while preserving the `ours` move order.
- `--ignore-comments` skips `*comment*` subtrees.
- Parse failures or limit overruns fall back to line-level text diff.

## PDF Visual Diff and OCR

- `--pdf-visual` renders pages to PNG with poppler `pdftoppm`.
- Pixel differences are computed by `diff-core/src/diff/image.rs`.
- Outputs include `heatmaps/`, `side-by-side/`, `overlays/`, `report.html`, `report.pdf`, raster overlay `report-overlay.pdf`, and `report-vector.pdf`, which appends vector red rectangles to the original PDF content stream via lopdf.
- `--ocr` calls Tesseract for images. For PDFs, pages are rendered with `pdftoppm`, OCR'd page by page, then compared as text.
- Tool paths can be set with `--tesseract` / `--pdftoppm` / environment variables; portable builds can be installed separately.

## Remote and Linked Input

- Plain `http(s)://` file URLs use the linked-input path in `diff-cli/src/link.rs`: HEAD is used to read `Content-Length`, aria2 performs the download when available, and a built-in streaming HTTP client is the fallback.
- Windows releases embed aria2 1.37.0 as a zstd-compressed asset (`crates/diff-cli/assets/aria2c.exe.zst`), decompress it on first use, and extract it to `%TEMP%\diff-everything-tools` with a per-process filename. `--aria2 <FILE>` overrides it; other platforms use `aria2c` from PATH.
- Downloads above 100 MiB require confirmation. `-y` accepts the prompt; non-interactive runs without `-y` fail before writing the file. `--download-timeout` bounds downloads and clones.
- `--git-history` treats both inputs as Git repositories. Remote URLs are cloned as bare mirrors with `--filter=blob:none`, then retried as a full mirror. The report compares refs, HEAD, commit count and commit metadata for the most recent 2,000 commits per side.
- `webdav://` / `webdavs://` map to HTTP(S) and use the WebDAV listing path; `sftp://` reads through ssh2 and `smb://` through smb2.
- Authentication comes from URL user info or `--remote-user` / `--remote-password` for both linked HTTP files and the other remote schemes.
- Files are downloaded into `tempfile::TempDir`, kept alive during comparison, and removed afterward.
- Report paths keep the original URL; sessions store the original URL.
- Tests use local tiny_http WebDAV/HTTP, file-URL Git clones, asyncssh SFTP and impacket SMB test servers when those maintainer-local helpers are available.

## Directory Sync and Sessions

- `sync` walks both directory trees and builds Copy/Delete plans based on content equality.
- Plans are printed only by default; `--sync-apply` executes them. `--backup-dir` backs up files before overwrite/delete.
- Mirror modes delete extra files on the target and remove empty directories.
- `--save-session` stores paths and the full `DiffOptions`; `--load-session` restores them and compares immediately.

## Document and Workbook Formats

- PDF: `lopdf` extracts text per page and positioned text-show operations (`Tm`/`Td`/`Tj`/`TJ`), producing `page[n]/block[m]` paths with `x,y` positions. Page insertion, removal and moves are aligned by content, so a new page does not shift every later page. PDF Info metadata and page sizes are compared. `pdf-extract` remains the text fallback, and OCR keeps its dedicated path. When `--pdf-visual` is enabled, the engine runs `pdf_visual` after the text node and stores a `DocumentVisual` summary in `DocumentDiff`, so ANSI/JSON/HTML reports carry the visual result while HTML/PNG/PDF/`report.json` artifacts are written to the output directory.
- OFD: reads `Doc_*/Pages/*/Content.xml` from the ZIP and emits one `page[n]` block per page.
- DOCX: `word/document.xml` is walked as paragraphs, headings, lists and tables; headers, footers, footnotes, endnotes and comments become separate `header[n]` / `footnote` blocks. Tables use stable `body/tbl[n]` paths. Run/paragraph properties drive precise font metrics (family, half-point size converted to points, bold/italic/underline, color, alignment, line spacing, indents), and media parts under `word/media/` are compared by SHA-256.
- PPTX: `ppt/presentation.xml` and its relationships determine slide order and hidden state; `slideN.xml` relationships map notes, layout and media. Each slide is compared by shape name, text, table, media hash, geometry, layout reference and run font metrics (size in points, family, bold/italic/underline, color, baseline), with slide add/remove/move reported before shape detail.
- ODT/ODP: `content.xml` is parsed into paragraph/heading/table blocks and slide-page blocks; metadata and `Pictures/` media are compared alongside the blocks.
- Cross-format PDF↔DOCX/ODT/PPTX/ODP and DOCX↔ODT/PPTX↔ODP comparisons normalize both sides to block paths before diffing; remaining mixed document pairs fall back to extracted text.
- XLSX/XLS/ODS: calamine reads sheet order, visibility, cached values and formulas; an XLSX OOXML pass adds merged ranges, hidden rows/columns, defined names, drawing references, chart XML hashes and conditional-formatting rules (range, type, operator, priority, expanded dxf font/fill/border attributes, text, formulas). Rows are aligned by signature before cells are compared, so a single inserted row does not mark every later cell as changed.
- EPUB/RTF/EML continue to use `StructuredDiff` / `TextDiff`; audio uses `AudioDiff`.
- Any structural parse failure falls back to extracted text or binary comparison with a warning. Limits that omit pages, blocks, slides, rows or cells propagate `Truncated` completeness.

## Rendering Strategy

- **ANSI**: local CLI and CI logs. The rich layout (banner, aligned input/status/summary block, section rules, colored markers and line-number gutters) is selected automatically for a terminal; `--style rich` forces it and `--style plain` keeps the compact layout for pipes and scripts. `NO_COLOR` / `--no-color` removes colors but preserves the selected layout. The TUI always uses the rich layout.
- **JSON**: stable machine interface, validated by parsing in CLI tests.
- **Markdown**: PR comments and document archives.
- **HTML**: self-contained single file for sharing; document blocks, slides and workbook cells get side-by-side tables in addition to compact change lists.
- **unified**: compatible with `diff`/`patch` workflows.
- CLI help is grouped by task headings and uses clap styling plus a terminal-width-aware template; examples and exit codes are appended as after-help.

## Parallelism, Progress and Security

- Directory and archive entries are compared with rayon. Each worker gets a forked `Ctx` (summary, warnings, excludes); summaries and warnings are merged in path order after collection, so output is deterministic.
- `--jobs` defaults to 4; `--jobs 0` uses the rayon global pool, `--jobs 1` runs sequentially, and larger values build a bounded local thread pool.
- `ProgressHandle` is a thread-safe stderr sink. It reports entry counts and accepts messages from the engine and Kaspersky scanner; `--progress` forces it, `--no-progress`/`--quiet` disable it.
- Kaspersky integration auto-detects `avp.com` under common installation paths, checks the `AVP*` service state, and runs `SCAN ... /i0 /R:<temp-report>`. `/i0` is report-only: no disinfection or deletion. Threats abort with exit code `2`; a missing installation or stopped service is a warning. Reports are decoded from UTF-8/UTF-16 and parsed for detection lines.
- Linked inputs (HTTP(S) files and Git repositories) run the Kaspersky scan by default after download/clone. `--no-security-scan` skips it; `--security-scan` forces a scan for local comparisons.

## Packed Application Detection

- `packers.rs` inspects executable section names (UPX0/UPX1, .aspack, .themida, .vmp0/.vmp1, .enigma*, .mpress*, .pec, .nsp*, .armadillo, .obsidium, .pelock, .telock, .molebox, .boxed, .rlpack). APK/AAB inputs are checked for known shell libraries (360 Jiagu, Tencent Legu, Bangcle, Alibaba, Baidu, Naqu, NetEase). Raw embedded strings are intentionally not used as evidence, preventing the tool's own signature table from flagging its executable.
- Detection reads at most 64 MiB; larger executables are sampled at the head and tail so detection stays bounded.
- The CLI prints the detected packer and evidence, warns about license/legal implications, and prompts `Continue analysis? [y/N]`. `-y` approves; non-interactive runs without `-y` exit 2. Accepted detections are appended to report warnings.

## Completely Different Verdict

- `DiffReport.completely_different` is true when more than 99% of the compared content changed (`COMPLETELY_DIFFERENT_THRESHOLD = 0.99`).
- Text and binary leaves use `(changed_bytes / left_bytes + changed_bytes / right_bytes) / 2`, where `changed_bytes` counts unequal positions plus the length difference; zero-length sides are handled explicitly.
- Images, tables, workbooks, notebooks, audio and directories/archives use semantic ratios: pixel ratio, changed rows / max rows, changed cells / max cells, changed waveform windows / total windows, and changed entries / total entries.
- Top-level containers use their entry ratio rather than the maximum child ratio, so one heavily changed file in a large directory does not mislabel the whole comparison.
- ANSI, Markdown, HTML and JSON expose the verdict; the normal `DIFFERENT` status and exit code are unchanged.

## Runtime Errors

- Input paths are checked after remote/link resolution. A missing input produces `error: input not found`, a `left:`/`right:` path line and a spelling hint instead of a raw localized OS error.
- Other failures are rendered as a concise cause chain. `std::io::ErrorKind` is translated to English descriptions (`file or directory not found`, `permission denied`, `invalid or malformed data`, `operation timed out`, and so on) with the OS error code retained.
- Hints cover missing paths, permissions, truncated data, timeouts and other common cases.
- ANSI styling is used only when stderr is a terminal and neither `--no-color` nor `NO_COLOR` is set.

## Supported Format Catalog

- `diff-cli/src/support.rs` contains a static catalog of format families, aliases, summaries, diff capabilities and notes.
- `--support` prints the catalog grouped by category; `--support FORMAT` resolves a case-insensitive name or alias and prints the full capability list.
- The catalog is display-only: it does not change format detection, dispatch or comparison behavior.
- `--version` intentionally omits the feature list; use `--support` for capability discovery.

## Version and Build Metadata

- The package/display version is `1.3.0`, codename `Toyger`.
- `diff-cli/build.rs` captures the git commit, Cargo target/profile and `rustc --version` at compile time. Missing git data falls back to `unknown`.
- `-V` / `version` exposes the short semantic version; `--version` / `long_version` appends commit, build profile/target, rustc, license, repository and feature metadata. Build metadata is display-only and does not affect diff behavior.

## Performance and Safety Boundaries

- Files below `max_file_bytes` are read into memory at once.
- Larger files use 64 KiB streaming comparison and emit a summary only.
- Directories build a file table before comparison for statistics and truncation.
- ZIP comparison is streaming: the central directory is scanned once into metadata, payloads are decompressed on demand, unpacked sizes are pre-checked, and changed pairs are materialised in bounded 32 MiB batches. Uncompressed TAR records payload byte ranges in its catalog; CPIO and AR/DEB use the same catalog-plus-slice reader. Compressed TAR, 7Z, RPM, CAB, ISO and XAR still materialise entries. Every archive reader (ZIP, 7Z, TAR, RPM, CAB, ISO, AR, XAR, CPIO) reads each entry through the same bounded read (`read_bounded`: at most `limit + 1` bytes) and skips entries that exceed the per-file limit. XAR checks the declared table-of-contents size against the limit before reserving memory (and caps the reservation), reads the compressed TOC and payloads through the bound, and ZIP LZMA bounds the compressed stream as well as its output.
- `max_depth` counts container levels from the input root (depth 0): leaves are compared while `depth <= max_depth`, and every container (directory, archive or compressed stream) expands only while `depth < max_depth`. A directory at the limit is reported as one `directory depth limit reached` node instead of being scanned, matching how archives at the limit are reported.
- Unpacked data is bounded by a shared `ResourceBudget` (`max_expanded_bytes`, default 1 GiB): every archive reader and decompressor charges the bytes it materializes atomically, so parallel workers cannot race past the cap, and the budget latches when spent. Directory scans stop at the entry limit instead of enumerating trees that cannot be compared anyway. Truncations propagate as `ComparisonCompleteness::Truncated` through `DiffNode::completeness_of()` into `DiffReport.completeness`, and the CLI exits `3` for an equal-but-truncated comparison. Files above the per-file limit are read through `limit + 1`-byte bounded reads, so growth between the metadata check and the read (TOCTOU) cannot push past the limit.
- Concise detail is a display-only cap (`--detail N`, default 10; `--detail full` disables it). The CLI puts the cap in `DiffOptions.concise_limit`; text hunk collection, table row comparison, workbook cell comparison, document block parsing and slide parsing stop at the effective display cap. Those parsers still compare enough to determine `Different`/`Equal`, but the cap never marks `ComparisonCompleteness::Truncated` or changes the exit code. The final `concise::apply` pass only handles remaining structures and nested lists.
- The scheduler bounds the parallel work set, not just the thread count: each file task acquires its footprint (left + right size) from an in-flight gate (`max_inflight_bytes`, default 256 MiB, `0` disables) before its contents are read. Leaf tasks release it when the node is built; container tasks release it before recursive expansion so a parent buffer cannot wait on children that need the same gate. Tasks larger than the budget are clamped and run alone. Entry results are aggregated chunk by chunk (`jobs x 8` keys per chunk, key order preserved) so only a bounded number of finished nodes is resident before their contexts merge.
- `Package` charges document part reads and media hashing to the shared `ResourceBudget`, enforces a per-entry bound, and returns a budget error that the engine converts into a truncated document node. XLSX metadata records the actual used-cell range during its existing XML scan; `read_workbook` rejects an oversized dense bounding box before Calamine allocates it. `max_workbook_cells` is a single workbook-wide budget. 7z uses a seen-entry counter with `ctx.omit()`, `DirectoryDiff.truncated` is the merged scan/comparison flag, and CSV/DOCX table readers stop at bounded input caps.
- Document parsing reuses one `diff/package.rs::Package` per side for DOCX/PPTX/ODT/ODP and XLSX metadata. Media hashing streams through a reused 64 KiB buffer. PDF text, positioned blocks, metadata and page geometry are derived from one `lopdf::Document` per side, and each page content stream is decoded once for both text and blocks; left/right document/workbook parsing runs through `rayon::join`.
- Text diffs strip common line prefixes/suffixes, use patience for large changed middles, stop building hunks at the detail cap, and skip word-level span generation above a configurable row threshold. Structured diffs short-circuit equal subtrees, trim equal array edges and fingerprint large arrays; directories and archives pair unmatched entries by `(size, SHA-256)` for rename/move reporting, hashing only unmatched entries and holding an in-flight permit while a directory file is hashed. CSV/TSV infer a unique implicit key when `--key` is absent and warn about it.
- Format dispatch goes through a parser registry: a `FormatParser` trait maps each `FileKind` to one of fourteen parsers (text, structured, notebook, sqlite, executable, dex, font, table, image, compression, archive, document, workbook, binary). Each parser owns its depth gate and reader and charges unpacked bytes through the shared context, so adding a format means implementing the trait and registering it — the dispatcher and resource model stay untouched.
- Symbolic links are not followed.
- All limits are exposed through `DiffOptions`. `--max-file-bytes` overrides the 64 MiB per-file detailed-diff limit; `--no-file-size-limit` sets `UNLIMITED_FILE_BYTES` (`u64::MAX / 2`) so the same code paths keep overflow-safe `limit + 1` reads while allowing arbitrarily large real files.
- `--no-entry-limit` sets directory, archive, document-block and slide limits to `usize::MAX`; `--max-entries` applies the same value to all four. `--no-limits` additionally removes row, hunk, structured-change, table, workbook, depth and PDF page limits; the individual limit flags conflict with it. These options intentionally trade memory/runtime for completeness.
- `cargo build --release` is the single optimized profile: `opt-level = 3`, thin LTO, four codegen units and symbol stripping. `cargo test --release` reuses the same dependency artifacts, so the shipped binary and the release test run come from one profile. The embedded Windows aria2 binary is zstd-compressed and decompressed on first use.
- Integration tests use one target (`tests/integration.rs`) with the six case modules under `tests/cases/`; this avoids six independent links of the full dependency graph.
- Unit tests live outside the source tree in `tests/unit/<source path>.rs` (for example `tests/unit/diff/text.rs` for `src/diff/text.rs`). Each source file mounts its tests with `#[cfg(test)] #[path = "..."] mod tests;`, so they keep private access while `src/` contains no test code.
- Directory/archive workers share an `Arc<GlobSet>` exclusion matcher, merge sorted keys in linear time, and avoid per-entry key clones and progress formatting when progress is disabled.
- Both workspace crates forbid `unsafe` code. ANSI output sanitizes untrusted control characters, the TUI has an RAII terminal cleanup guard, and Kaspersky report reads are capped at 4 MiB.
