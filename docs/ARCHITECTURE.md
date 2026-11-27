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
│  └─ tui.rs                 ratatui interactive viewer
│  └─ tests/                 engine / formats / robustness
└─ diff-cli/
   ├─ src/main.rs            argument parsing, exit codes, merge/patch/image branches
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

## Remote Input

- `webdav://` / `webdavs://` map to HTTP(S); `http(s)://` uses GET; `sftp://` reads through ssh2.
- Authentication comes from URL user info or `--remote-user` / `--remote-password`.
- Files are downloaded into `tempfile::TempDir`, kept alive during comparison, and removed afterward.
- Report paths keep the original URL; sessions store the original URL.
- Tests use local tiny_http WebDAV/HTTP, asyncssh SFTP and impacket SMB test servers when those maintainer-local helpers are available.

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

- **ANSI**: local CLI and CI logs; `NO_COLOR` / non-TTY disables colors.
- **JSON**: stable machine interface, validated by parsing in CLI tests.
- **Markdown**: PR comments and document archives.
- **HTML**: self-contained single file for sharing.
- **unified**: compatible with `diff`/`patch` workflows.

## Performance and Safety Boundaries

- Files below `max_file_bytes` are read into memory at once.
- Larger files use 64 KiB streaming comparison and emit a summary only.
- Directories build a file table before comparison for statistics and truncation.
- ZIP/TAR entries are read in memory with per-entry limits to avoid zip bombs.
- Symbolic links are not followed.
- All limits are exposed through `DiffOptions`.
