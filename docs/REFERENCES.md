# Reference Projects and Implementation Map

This project is based on **studying the features and algorithms** of the open-source projects below. Reference repositories are shallow-cloned into `references/` (ignored by `.gitignore`). They are not compiled and no third-party code is copied into `diff-core`.

## Repository Versions

| Project | Local path | Commit | License |
|---|---|---|---|
| diffoscope | `references/diffoscope` | `dcfffcb` | GPL-3.0 |
| 7-Zip / sevenz-rust | crates.io / `testdata/tools/7zr.exe` | - | LGPL / MIT |
| Graphtage | `references/graphtage` | `1f6688d` | LGPL-3.0 |
| difftastic | `references/difftastic` | `4b834ef` | MIT |
| Open Diff | `references/open-diff` | `f42cd1a` | Apache-2.0 |
| uni-diff | `references/uni-diff` | `8418e97` | MIT |
| diff-pdf | `references/diff-pdf` | `092e6e3` | GPL-2.0 |
| nbdime | `references/nbdime` | `744c978` | BSD-3-Clause |
| HDiffPatch | `references/HDiffPatch` | `0696136` | MIT |
| gocompare | `references/gocompare` | `79539f5` | MIT |
| doc_diff_software | `references/doc-diff-software` | `e3e3f81` | Not declared in repository |
| Meld | `references/meld` | `35d6164` | GPL-2.0 |
| KDiff3 | `references/kdiff3` | `6c4eb7e` | GPL-2.0 |
| diff-so-fancy | `references/diff-so-fancy` | `cb21bb0` | MIT |
| jsdiff | `references/jsdiff` | `7aac04a` | BSD-3-Clause |
| diff-match-patch | `references/diff-match-patch` | `62f2e68` | Apache-2.0 |
| pixelmatch | `references/pixelmatch` | `40558aa` | ISC |
| openapi-diff | `references/openapi-diff` | `594183e` | Apache-2.0 |
| csvdiff | `references/csvdiff` | `1007bf3` | MIT |
| daff | `references/daff` | `ad5a02e` | MIT |
| diffsitter | `references/diffsitter` | `a5a6681` | MIT |

> GPL / LGPL projects were read for design only; no code was copied. DiffEverything's Rust implementation is independent.

## Additional References

- **Meld / KDiff3**: file/directory three-way merge and GUI workflows, used to validate CLI merge/sync semantics.
- **diff-so-fancy**: layout and colors for human-readable ANSI diffs.
- **jsdiff / diff-match-patch**: text diff algorithms and character-level diff ideas.
- **pixelmatch**: image pixel tolerance and difference counts.
- **openapi-diff**: semantic OpenAPI diff; DiffEverything detects OpenAPI automatically and marks breaking changes on the structured diff.
- **csvdiff / daff**: CSV table diff, primary-key alignment and cell differences.
- **diffsitter**: tree-sitter structural diff; cross-reference for token/tree modes.

## Specific Borrowed Ideas

### diffoscope (recursive comparison and report layering)

Files read:

- `references/diffoscope/diffoscope/comparators/__init__.py`
- `references/diffoscope/diffoscope/comparators/utils/file.py`
- `references/diffoscope/diffoscope/presenters/`
- `references/diffoscope/diffoscope/main.py`

Borrowed:

- choose a comparator by file type;
- recursively unpack container formats and dispatch by entry type;
- decouple comparison results from presenters;
- text / HTML / JSON reports;
- resource limits for recursion depth, entry counts and file size.

DiffEverything mapping:

- `crates/diff-core/src/engine.rs`: `compare_bytes_internal` dispatch, recursive ZIP/7Z/TAR (gz/bz2/xz/zst/lz4)/CPIO/DEB/RPM/ISO, single-file gz/zst/bz2/xz/lz4 decompression, directory recursion;
- `crates/diff-core/src/model.rs`: `DiffReport` / `DiffNode` / `DiffBody`;
- `crates/diff-core/src/render/`: ANSI / JSON / Markdown / HTML / unified;
- `DiffOptions`: `max_file_bytes`, `max_dir_entries`, `max_zip_entries`, `max_depth`.

Implemented: DEB (ar container → control/data tar → recursive entry dispatch), RPM (metadata + gzip/xz/zstd payload), ISO9660 trees and CPIO newc recursion, CAB (MSZIP/LZX), XAR (TOC zlib + zlib/bzip2/xz/lzma/raw entries), AR (GNU/BSD long names, symbol tables) and TAR symlink/hardlink recursion, semantic SQLite table/column/row/schema comparison, ELF/PE/Mach-O section/symbol/import semantics, TTF/OTF/TTC/WOFF/WOFF2 font metadata and advance comparison, WebAssembly type/import/export/code comparison, Java class field/method/bytecode comparison, Android binary XML manifest comparison, Git index entry comparison, ICO/CUR container comparison, recursive IPK packages, OpenSSH public-key field comparison, APK embedded DEX/.so decompilation, and a generic external-tool comparator (`--external-tool`). PDF page visual diff is available through optional poppler rendering; `report.html` embeds base64 PNGs.

### Graphtage (structured semantic trees and cross-format comparison)

Files read:

- `references/graphtage/graphtage/__main__.py`
- `references/graphtage/graphtage/edits.py`
- `references/graphtage/graphtage/json.py`
- `references/graphtage/graphtage/csv.py`
- `references/graphtage/graphtage/ini.py`
- `references/graphtage/graphtage/xml.py`
- `references/graphtage/graphtage/yaml.py`

Borrowed:

- parse different formats into a shared tree, then generate path-level edits;
- allow different input formats to be compared;
- keep stable path output.

DiffEverything mapping:

- `crates/diff-core/src/diff/structured.rs`: JSON / JSON5 / YAML / TOML / INI / XML / HTML → `serde_json::Value`, unified `compare_values`; HTML builds a DOM tree with scraper;
- `FileKind::Structured` and `effective_kind`: cross-format JSON vs YAML etc.;
- `render/ansi.rs` / `markdown.rs` / `html.rs`: path-level `+/-/~` output;
- `--format json` exposes the structured edit list.

Implemented plist (XML and binary) and pickle (protocol 2-5), both comparable with JSON/YAML across formats. Arrays use similarity dynamic programming so inserts/deletes do not mark every following element as changed; large arrays fall back to Myers. Graphtage's full edit-distance graph search was not reproduced line by line. HTML DOMs are parsed into a shared tree with scraper.

### difftastic (tree-sitter syntax awareness)

Files read:

- `references/difftastic/src/parse/tree_sitter_parser.rs`
- `references/difftastic/src/diff/tree_diff.rs`
- `references/difftastic/src/options.rs`
- `references/difftastic/src/words.rs`

Borrowed:

- parse source with tree-sitter and take tokens from syntax leaves;
- comment-ignoring option;
- syntax-level comparison and exit-code conventions.

DiffEverything mapping:

- `crates/diff-core/src/diff/tokens.rs`: `Language` mapping for 13 languages, leaf-token collection, `--ignore-comments`;
- `compare_token_streams`: Myers token-sequence comparison;
- `crates/diff-core/src/diff/tree_diff.rs`: build tree-sitter syntax trees, align recursively by node type/child signatures, emit added/removed/modified nodes;
- `DiffBody::Tokens` / `DiffBody::Tree` + five renderers;
- `--mode token` / `--mode tree`, with `--ignore-comments` automatically enabling token mode.

Differences: tree mode uses content signatures, optimal bipartite matching and order-sliding normalization, with Myers fallback for very large nodes. Graph mode linearizes syntax trees into enter/token/exit events and runs a bounded shortest path; tokens are equivalent by text across depth, priced by node type, added/removed or heavily modified subtrees are grouped, exact moved subtrees and stable subtrees become non-local zero-cost anchors, and LCS (with a minimal-index-shift slider) detects moved subtrees, including moves followed by modifications, emitting `Moved`. It does not reproduce difftastic's full Dijkstra graph search and whole-graph slider costs line by line. Supported languages: Rust/Python/JS/TS/C/C++/Go/Java/Bash/Ruby/C#/Swift/Lua.

### Open Diff (workflows and multiple modes)

Files read:

- `references/open-diff/README.md`
- `references/open-diff/src/api/diff.ts`
- `references/open-diff/src/api/folderMerge.ts`
- `references/open-diff/src/api/sync.ts`
- `references/open-diff/src/api/script.ts`

Borrowed:

- unified workflows for text / tables / images / hex / binary / directories;
- three-way merge (text, notebooks, directories);
- HTML reports, ignore whitespace/case/comments, exclusion rules;
- CLI exit codes and scripting.

DiffEverything mapping:

- CLI modes for all corresponding formats;
- `diff-core/src/merge.rs` (diffy three-way merge);
- `diff-core/src/folder_merge.rs` (directory three-way merge plan/apply);
- `--ignore-whitespace`, `--ignore-case`, `--ignore-comments`, `--exclude`;
- `--format html`, `--detail concise|full`, `-y`/`--objdump` decompilation authorization, `--head` / `--last` / `--filter`, `--name-only`, `--quiet`, `--summary-only`.

Directory sync (plan/apply/backup), session save/load, WebDAV/SFTP/SMB remote files, aria2-backed HTTP(S) linked files and Git repository history comparison are implemented.

### uni-diff (document formats and output)

Files read:

- `references/uni-diff/README.md`
- `references/uni-diff/uni_diff/` (converter / renderer structure)

Borrowed:

- text extraction ideas for PDF / DOCX / XLSX / PPTX;
- multiple outputs: ANSI / JSON / HTML / PNG / TUI;
- block-level diff and location information.

DiffEverything mapping:

- `documents.rs`: PDF text + page count, DOCX body/headers/footers, PPTX slides/notes;
- `workbook.rs`: XLSX/XLS/ODS cells;
- `render/`: ANSI/JSON/Markdown/HTML/unified;
- `image_export.rs`: heatmaps and side-by-side images (PNG).

Implemented: OCR (images + rendered PDF pages), PDF page rendering to PNG heatmaps/side-by-side/HTML reports, PDF text page markers (`# Page N`), block-level changed regions for images, interactive TUI (falls back to ANSI when piped). Not implemented: character/coordinate-level visual block localization inside documents.

### diff-pdf (PDF visual comparison)

File read: `references/diff-pdf/diff-pdf.cc`.

Borrowed: render pages to bitmaps and compare pixels page by page; productized exit codes and `--output-diff`.

DiffEverything mapping: PDF text layer + page-count diff; `--pdf-visual` renders pages with poppler `pdftoppm` and produces per-page pixel statistics, heatmaps, side-by-side images and HTML reports; `--ocr` can OCR rendered pages.

Implemented: `--pdf-visual` emits `report.pdf` (side-by-side + heatmap), `report-overlay.pdf` (raster red overlay on original pages), `report-vector.pdf` (red vector rectangles appended to the original page content stream with lopdf), `report.html` (embedded base64 PNG) and PNG images.

### nbdime (notebooks)

Files read:

- `references/nbdime/nbdime/diffing/notebooks.py`
- `references/nbdime/nbdime/diff_format.py`
- `references/nbdime/nbdime/merging/`

Borrowed: notebooks are JSON, but cells/sources/outputs need stable paths.

DiffEverything mapping:

- `crates/diff-core/src/diff/notebook.rs`: cell-level signature alignment, source/outputs/execution_count diff;
- cell-level three-way merge: align by cell `id` first, use diffy for source merge, mark outputs/execution conflicts;
- `--base` automatically uses notebook merge for `.ipynb`;
- `DiffBody::Notebook` + five renderers.

Not implemented: nbdime's web rendering. Cell moves are globally aligned by id and included in diff status; cells without ids use content signatures to estimate moves, while complex merge-conflict heuristics remain conservative.

### HDiffPatch (binary delta)

Files read: `references/HDiffPatch/libHDiffPatch/HDiff/`, `references/HDiffPatch/hpatch/`.

Borrowed: binary patch/apply workflow.

DiffEverything mapping: `diff-core/src/patch.rs` uses `bsdiff` for single-file patches and implements a pure-Rust HDIFF13 encoder plus zstd/zlib/bzip2/lzma2 decode/apply. The same module implements pure-Rust HDIFF19 directory patch encode/apply (same-file pair dedup, fadler64 checksums, optional zstd/zlib/lzma2/bzip2 head/hdiff compression; the apply side reads official zstd/checksum directory patches). `diff-core/src/folder_patch.rs` uses a custom ZIP container (manifest + bsdiff/new files) for directory patch create/apply. `--patch-format hdiff` invokes official hdiffz/hpatchz for compressed/checksum directory patches.

Not implemented: none. HDiffPatch zstd/zlib/lzma2/bzip2 directory patches all have pure-Rust encode/apply; official hpatchz builds may lack individual plugins, but the pure-Rust applier can read them.

### doc_diff_software (OFD / documents)

Read: `references/doc-diff-software/README.md`.

DiffEverything mapping: read OFD as a ZIP container, extract `TextCode` text from `Doc_*/Pages/*/Content.xml`, then run text diff.

### gocompare / doc_diff_software

Read: `references/gocompare/README.md`, `references/doc-diff-software/README.md`.

Borrowed: a unified entry point for directories/tables/images/documents, and a safe "show differences first, then apply" workflow.

DiffEverything mapping: local directories, tables, images, documents, audio tags/properties, OFD, three-way merge, directory sync, WebDAV/HTTP(S)/SFTP/SMB remote files, WebDAV/SFTP/SMB recursive directories, aria2-backed linked files and Git repository history comparison.

Registry export `.reg` files are compared as text; OCR is implemented for images and rendered scanned-PDF pages.

## Bundled Tools

The Windows build bundles aria2 1.37.0 (`crates/diff-cli/assets/aria2c.exe`) for linked HTTP(S) downloads. It is the unmodified official 64-bit build from https://github.com/aria2/aria2/releases/tag/release-1.37.0, is invoked as a separate process, and is distributed under GPL-2.0-or-later. The GPL text, OpenSSL notice and authors file are next to the executable and reproduced in [THIRD_PARTY_LICENSES.md](../THIRD_PARTY_LICENSES.md#bundled-binaries).

## Test Comparison

- `cargo test --workspace`: 331 tests;
- maintainer-local smoke-test suite: 189 end-to-end cases;
- maintainer-local fixture download script: downloads real test files from W3C, Mozilla, python-docx, calamine, plotly, Kubernetes, w3schools, jsonplaceholder, picsum, GNU, Debian, Arch Linux, Fedora, CentOS and Open Diff;
- `testdata/external/MANIFEST.csv`: URL, SHA-256, size.

Third-party reference projects are used for research and feature comparison only. Reusing their code directly would require complying with the corresponding license and cannot be merged into MIT-licensed code without permission. The only third-party executable distributed with DiffEverything is the separate, unmodified aria2 binary described under Bundled Tools.
