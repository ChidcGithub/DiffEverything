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
│  │  ├─ table.rs            CSV/TSV table diff
│  │  ├─ workbook.rs         XLSX/XLS/ODS cell diff
│  │  ├─ documents.rs        PDF/OFD/DOCX/PPTX text extraction
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
│  └─ tests/                 engine / formats / robustness
└─ diff-cli/
   ├─ src/main.rs            argument parsing, exit codes, merge/patch/image branches
   ├─ src/link.rs            aria2/built-in HTTP downloads, 100 MiB guard, Git history
   ├─ src/remote.rs          WebDAV / SFTP / SMB remote inputs
   ├─ src/tui.rs             interactive ANSI viewer
   └─ tests/cli.rs           CLI end-to-end tests
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
normalize                         text / JSON tree / pixels / table / workbook / document text / bytes
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
| `Table(TableDiff)` | CSV/TSV row/column/cell diff |
| `Workbook(WorkbookDiff)` | Worksheet/cell diff |
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

## Document Formats

- PDF: `pdf-extract` for text, `lopdf` for page count; failures fall back to binary.
- OFD: read `Doc_*/Pages/*/Content.xml` from the ZIP and extract `TextCode` text.
- Audio: lofty reads format/duration/bitrate/sample rate/channels/tags; metadata is compared and content changes are checked.
- DOCX: read `word/document.xml`, `header*.xml`, `footer*.xml`, footnotes/endnotes/comments from the ZIP; `quick-xml` extracts paragraph and table text.
- PPTX: read `ppt/slides/slideN.xml` and `notesSlides/notesSlideN.xml`; extract `<a:t>`.
- XLSX/XLS/ODS: calamine reads worksheets and cells, compared by address.
- These formats reuse `TextDiff` / `WorkbookDiff`, so report shapes stay consistent.

## Rendering Strategy

- **ANSI**: local CLI and CI logs. The rich layout (banner, aligned input/status/summary block, section rules, colored markers and line-number gutters) is selected automatically for a terminal; `--style rich` forces it and `--style plain` keeps the compact layout for pipes and scripts. `NO_COLOR` / `--no-color` removes colors but preserves the selected layout. The TUI always uses the rich layout.
- **JSON**: stable machine interface, validated by parsing in CLI tests.
- **Markdown**: PR comments and document archives.
- **HTML**: self-contained single file for sharing.
- **unified**: compatible with `diff`/`patch` workflows.
- CLI help is grouped by task headings and uses clap styling plus a terminal-width-aware template; examples and exit codes are appended as after-help.

## Parallelism, Progress and Security

- Directory and archive entries are compared with rayon. Each worker gets a forked `Ctx` (summary, warnings, excludes); summaries and warnings are merged in path order after collection, so output is deterministic.
- `--jobs` defaults to 4; `--jobs 0` uses the rayon global pool, `--jobs 1` runs sequentially, and larger values build a bounded local thread pool.
- `ProgressHandle` is a thread-safe stderr sink. It reports entry counts and accepts messages from the engine and Kaspersky scanner; `--progress` forces it, `--no-progress`/`--quiet` disable it.
- Kaspersky integration auto-detects `avp.com` under common installation paths, checks the `AVP*` service state, and runs `SCAN ... /i0 /R:<temp-report>`. `/i0` is report-only: no disinfection or deletion. Threats abort with exit code `2`; a missing installation or stopped service is a warning. Reports are decoded from UTF-8/UTF-16 and parsed for detection lines.
- Linked inputs (HTTP(S) files and Git repositories) run the Kaspersky scan by default after download/clone. `--no-security-scan` skips it; `--security-scan` forces a scan for local comparisons.

## Packed Application Detection

- `packers.rs` inspects executable section names and strong signatures (UPX, ASPack, Themida, VMProtect, WinLicense, Enigma, MPRESS, PECompact, NSPack, Armadillo, Obsidium, PELock, tElock, MoleBox, BoxedApp, RLPack). APK/AAB inputs are checked for known shell libraries (360 Jiagu, Tencent Legu, Bangcle, Alibaba, Baidu, Naqu, NetEase).
- Detection reads at most 64 MiB; larger executables are sampled at the head and tail so detection stays bounded.
- The CLI prints the detected packer and evidence, warns about license/legal implications, and prompts `Continue analysis? [y/N]`. `-y` approves; non-interactive runs without `-y` exit 2. Accepted detections are appended to report warnings.

## Completely Different Verdict

- `DiffReport.completely_different` is true when more than 90% of the compared content changed (`COMPLETELY_DIFFERENT_THRESHOLD = 0.90`).
- The ratio is format-aware: text changed lines / max line count, binary differing bytes / max byte length, image pixel ratio, table changed rows / max rows, workbook changed cells / max cells, notebook changed cells / max cells, audio changed waveform windows / total windows, and directory/archive changed entries / total entries.
- Top-level containers use their entry ratio rather than the maximum child ratio, so one heavily changed file in a large directory does not mislabel the whole comparison.
- ANSI, Markdown, HTML and JSON expose the verdict; the normal `DIFFERENT` status and exit code are unchanged.

## Version and Build Metadata

- The package version is `1.0.2`; the release codename is `Toyger`.
- `diff-cli/build.rs` captures the git commit, Cargo target/profile and `rustc --version` at compile time. Missing git data falls back to `unknown`.
- `-V` / `version` exposes the short semantic version; `--version` / `long_version` appends commit, build profile/target, rustc, license, repository and feature metadata. Build metadata is display-only and does not affect diff behavior.

## Performance and Safety Boundaries

- Files below `max_file_bytes` are read into memory at once.
- Larger files use 64 KiB streaming comparison and emit a summary only.
- Directories build a file table before comparison for statistics and truncation.
- ZIP/TAR entries are read in memory with per-entry limits to avoid zip bombs.
- Symbolic links are not followed.
- All limits are exposed through `DiffOptions`. `--max-file-bytes` overrides the 64 MiB per-file detailed-diff limit; `--no-file-size-limit` sets `UNLIMITED_FILE_BYTES` (`u64::MAX / 2`) so the same code paths keep overflow-safe `limit + 1` reads while allowing arbitrarily large real files.
- `--no-entry-limit` sets directory and archive entry limits to `usize::MAX`. `--no-limits` additionally removes row, hunk, structured-change, table, workbook, depth and PDF page limits; the individual limit flags conflict with it. These options intentionally trade memory/runtime for completeness.
- The release profile enables fat LTO, one codegen unit, `opt-level = 3` and symbol stripping. The embedded Windows aria2 binary is zstd-compressed and decompressed on first use.
- Directory/archive workers share an `Arc<GlobSet>` exclusion matcher, merge sorted keys in linear time, and avoid per-entry key clones and progress formatting when progress is disabled.
- Both workspace crates forbid `unsafe` code. ANSI output sanitizes untrusted control characters, the TUI has an RAII terminal cleanup guard, and Kaspersky report reads are capped at 4 MiB.
