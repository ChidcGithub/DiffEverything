# Verification Record

## Local Environment

- Windows / PowerShell
- Rust 1.97.1 (project MSRV 1.88)
- Architecture: `diff-core` library + `diff-everything` CLI

## Release Verification (v1.0-4596984fe5db)

On 2026-11-27 13:27 CST, the local verification run passed fmt check, clippy, 279 tests, the release build and 189 smoke tests in sequence. Later commits add DEX, serialization, package, document and media tests, parallel comparison, progress, Kaspersky scanning, linked HTTP(S) downloads with embedded aria2, Git repository history comparison and a DEX performance fix; the current workspace test count is 327.

Final release artifact: `target/release/diff-everything.exe`, 50,950,656 bytes (48.59 MiB), SHA-256 `5B4CA0427B707BB9B384D7FC3ABA4E748FBBD981D3CAA531EB35E4B9EBA48E7D`.

## Extended Format Verification

On 2026-11-27, `cargo test --workspace` and `cargo test --workspace --release` each passed 327 tests; `cargo clippy --workspace --all-targets -- -D warnings` passed; the smoke suite passed 189 cases. The 21 format integration tests cover SquashFS, Snap, AppImage, Flatpak, ext4, DMG, MSIX, AAB, Parquet, Avro, ORC, CBOR, MessagePack, Protobuf, ODT, ODP, EPUB, RTF, EML, video, and audio waveform comparison; an additional CLI table test runs every new format through the binary. Each format test also verifies that `--detail full` exposes more detail than `concise`. Parallel and sequential directory/archive reports are compared for equality, forced progress is verified on stderr, and Kaspersky integration is covered by report-parser tests, a mock `avp.com` scan, and a real EICAR detection run (threat exit code 2).

## Linked Input Verification

The linked-input tests use local `tiny_http` servers and real temporary Git repositories:

- `linked_download_over_100_mib_requires_yes` serves a 101 MiB `Content-Length` and confirms a non-interactive run without `-y` exits with code 2 before downloading.
- `linked_inputs_scan_with_kaspersky_by_default` compares two HTTP links without `--security-scan` and verifies that the mock Kaspersky scanner is invoked automatically; a second run with `--no-security-scan` verifies the opt-out.
- `git_history_compares_commits_with_concise_and_full_detail` creates two repositories with different commit histories and verifies concise truncation (4 changes) versus full detail (more than 4 changes), plus `git-history` report type and structured-change counts.
- `git_history_clones_url_inputs` passes `file://` repository URLs, exercises the bare-mirror clone path, and verifies that the original URLs are retained in the report.
- Unit tests cover Git/HTTP URL recognition, safe download file names (traversal and Windows reserved names), `Content-Range` size probing, and the `-y` override for the 100 MiB guard.

Manual release verification on Windows also downloaded two HTTP files with the embedded aria2 1.37.0 binary and compared them successfully. The embedded executable was extracted to `%TEMP%\diff-everything-tools\aria2c-1.37.0-<pid>.exe` with the expected 5,649,408-byte size. A release build comparison of the same repository history through `file://` URLs reported `2 -> 7 commit(s)`.

## Terminal UI Verification

- `help_is_grouped_and_includes_examples` runs `--help`, verifies the stable usage line, every task heading, `--style`, `--git-history`, and the examples/exit-code footer.
- `rich_style_adds_banner_rules_and_line_numbers` runs with `--style rich --no-color --detail full`, verifies the banner/rule/status layout, the line-number gutter, and confirms that auto mode still emits the compact plain layout when stdout is piped.
- Existing forced-progress coverage verifies the spinner message on stderr; the progress renderer now adds a fixed-width `[####----]` bar, counts and percentage without changing the test-visible label.
- The rich layout is ASCII-only (banner rule `=`, section rules `-`, gutter `|`, markers `* + - ~ !`) so it remains readable in legacy Windows code pages; color is added separately through ANSI sequences.

## Test Results

```text
cargo test --workspace
10 CLI binary unit tests
67 CLI tests
144 diff-core unit tests
7 engine integration tests
21 extended format integration tests
58 format integration tests
15 robustness tests
3 folder merge integration tests
2 folder patch integration tests
= 327 tests, all passing
```

```text
cargo test --workspace --release
(327 tests, all passing)
```

```text
cargo clippy --workspace --all-targets -- -D warnings
Finished `dev` profile ... (0 warnings)

cargo fmt --all -- --check
(no diff)
```

```text
cargo build --release -p diff-cli
Finished `release` profile [optimized]
```

```text
smoke test
189 passed, 0 failed
```

Smoke coverage:

- Text / JSON / YAML / TOML / INI / XML / HTML / Plist (XML + binary) / Pickle (including readable fallback for unknown globals) / Gettext .mo / OpenAPI breaking changes / cross-format JSON-YAML, PLIST-JSON and PICKLE-JSON; SQLite table/row/schema semantic diff and physical-layout equality; structured array similarity alignment; ELF/PE/Mach-O semantic diff (including section file offsets, first-difference offset and hex snippets); WebAssembly module semantics; Java class field/method/bytecode semantics; Android APK binary manifest semantics; Git index entry semantics; ICO/CUR container semantics; recursive IPK package semantics; OpenSSH public-key field semantics (fat Mach-O is split by architecture); TTF/OTF/WOFF/WOFF2 font semantics; recursive CAB/XAR/AR/TAR archives (including TAR symlinks/hardlinks); external-tool comparator
- DEB packages (ar → control.tar.gz / data.tar.gz → recursive JSON/INI/text semantic diff)
- TAR.ZST archives and cross-compression equality between TAR.GZ and TAR.ZST; single-file ZST decompression before diff
- ISO9660 image directory trees; single-file BZ2/XZ decompression; CPIO newc / CPIO.GZ; RPM metadata + payload; TAR.LZ4 / single-file LZ4
- CSV / TSV / DOCX / PPTX / PDF / OFD / XLSX / XLS / ODS / WAV audio
- Images / binaries / directories / ZIP (including bzip2/LZMA/zstd entries) / 7Z / TAR.GZ / TAR.BZ2 / TAR.XZ / TAR.ZST / TAR.LZ4 / ZST / BZ2 / XZ / LZ4 / CPIO / CPIO.GZ / DEB / RPM / ISO / notebooks
- Token diff, full syntax-tree diff (content signatures + optimal bipartite matching + order-sliding normalization, Myers fallback for large nodes), bounded syntax-event graph shortest-path diff (`--mode graph`, non-local exact move edges + stable subtree anchors + depth-independent token equality + LCS slider + `Moved` detection), Notebook cell-level diff/move/merge, TUI fallback, 7Z archives, unified / Markdown / HTML / PDF reports
- `--head` / `--last` / `--filter` content filtering (limit leading/trailing units per file/entry, filter by path or content, glob supported), name-only, exclude, summary-only, quiet, ignore whitespace/case
- PDF text-layer page markers (`# PDF pages` / `# Page N`), image heatmaps / side-by-side images / YIQ perceptual tolerance (`--image-threshold`) / block-level changed regions
- Clean/conflicting three-way merges for text / notebooks / directories, single-file bsdiff, directory patch create/apply, pure-Rust HDIFF13 encode/apply (none/zstd/zlib/bzip2/lzma2), pure-Rust HDIFF19 directory patch encode/apply (fadler64 checksums, optional zstd/zlib/lzma2/bzip2 compression, applies official zstd/checksum directory patches), HDiffPatch-compatible file/folder patches; recursive CAB/XAR/AR archives
- Directory sync plan / apply / backup, session save / load
- Tesseract image OCR, PDF page OCR, poppler PDF visual reports (HTML with embedded base64 PNG + PNG + report.pdf + report-overlay.pdf + report-vector.pdf)
- WebDAV / SFTP / SMB remote inputs (local tiny_http, asyncssh and impacket servers); WebDAV/SFTP/SMB recursive directories (including empty directories); WebDAV listings reject cross-origin/parent links; same-name remote files do not overwrite each other (smoke uses the release binary); SFTP known_hosts match/missing/tampered cases
- Audio tag/property diff, OFD text diff
- Real downloaded PDF / DOCX / XLSX / XLS / ODS / CSV / YAML / XML / JSON / JPEG / TAR.GZ / ZIP / DEB files (Debian hello 2.10-2 and 2.10-3, 55 entries compared recursively)

## Real Test Files

The maintainer-local fixture download script downloads into `testdata/external/`; `MANIFEST.csv` records URL, SHA-256 and size. It also downloads optional tools: poppler (`pdftoppm`) and portable Tesseract (with tessdata). Sources include:

- W3C PDF test files
- Mozilla pdf.js test PDFs
- poppler Windows (`pdftoppm`)
- portable Tesseract 5.5.3 + tessdata_fast eng
- python-docx test documents
- tafia/calamine spreadsheet fixtures
- plotly iris.csv
- Kubernetes website nginx-app.yaml
- w3schools XML examples
- JSONPlaceholder
- picsum images
- GNU hello tar.gz
- Debian hello .deb (two versions)
- Arch Linux which .pkg.tar.zst (two versions)
- Fedora hello .rpm (gzip/zstd payload, two versions)
- CentOS which .rpm (xz payload, two versions)
- HDiffPatch v5.1.3 Windows binaries (hdiffz/hpatchz)
- Open Diff repository ZIP

Sources that return 403/404 are reported as warnings and skipped; current results are listed in `testdata/external/MANIFEST.csv`.

## Reference Repositories

Cloned under `references/`:

```text
diff-pdf          092e6e3
diffoscope        dcfffcb
difftastic        4b834ef
doc-diff-software e3e3f81
gocompare         79539f5
graphtage         1f6688d
HDiffPatch        0696136
nbdime            744c978
open-diff         f42cd1a
uni-diff          8418e97
meld              35d6164
kdiff3            6c4eb7e
diff-so-fancy     cb21bb0
jsdiff            7aac04a
diff-match-patch  62f2e68
pixelmatch        40558aa
openapi-diff      594183e
csvdiff           1007bf3
daff              ad5a02e
diffsitter        a5a6681
```

Sources, licenses, files read and feature mappings are in [REFERENCES.md](REFERENCES.md).

## Known Boundaries

- TUI is an interactive terminal viewer.
- PDF uses the text layer by default; visual diff requires optional poppler and OCR requires optional Tesseract (portable builds can be installed separately).
- WebDAV / HTTP(S) / SFTP / SMB file input is supported; WebDAV / SFTP / SMB support recursive directories.
- `--sync` supports local directories only; remote inputs are rejected before download so temporary copies are not modified.
- `--external-tool` receives resolved local paths; remote inputs are downloaded to a temporary directory first, and the external program sees temporary paths.
- ZIP LZMA (method 14) is decoded by rebuilding an LZMA-Alone header; unsupported or corrupted entries fall back to binary.
- SMB integration tests use a local SMB2 share provided by a maintainer-local Python impacket test server.
- SFTP optionally uses `--sftp-known-hosts`: missing or mismatched host keys are rejected. Verification is still off by default for temporary test servers.
- Legacy `.doc` / `.ppt` fall back to binary; `.xls` / `.ods` are supported by calamine.
- Encrypted ZIPs and unknown compression methods fall back to binary with a warning.
- Directory sync handles files only and does not follow symbolic links; file/directory type conflicts are reported as errors.
