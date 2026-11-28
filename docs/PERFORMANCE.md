# Performance baseline and 1.3.0 results

This document records the M0 baseline and the M1–M4 engineering results. It is
meant to be reproducible: the corpus generator, runner and allocation harness
live outside the repository (`.private/bench/`) so the published tree stays
small, and the commands below rebuild the same inputs.

## Environment

| Item | Value |
|---|---|
| OS | Windows 11 Pro for Workstations (10.0.26300) |
| CPU | Intel Core i9-13900H, 14 cores / 20 logical processors |
| Memory | 96 GiB |
| Toolchain | rustc/cargo 1.100.1 |
| Binary | `cargo build --release` |
| Baseline | `1.2.0 (Toyger)`, commit `959cf7c` |
| Candidate | 1.3.0 working tree |
| CLI | `--quiet --jobs 4 --max-entries 20000` |

Wall time is the best of five warm-cache runs. Peak working set is sampled
every 5 ms while the process runs. Read bytes come from
`GetProcessIoCounters` (`ReadTransferCount`) on the measured run; expanded
bytes are the JSON report's `expanded_bytes`. The baseline executable was the
published 1.2.0 release binary; the candidate was rebuilt from this tree.

## Synthetic corpus

| Case | Shape |
|---|---|
| `text-prefix` | 200k lines (~4.8 MB), last 10 lines changed |
| `text-prefix-large` | 2M lines (~49 MB), last 10 lines changed |
| `text-insert` | 100k lines, one line inserted in the middle |
| `json-large` | 50k records (~3.3 MB), one field and one array tail changed |
| `directory-10k` | 10k small files, 10 changed, 100 renamed |
| `zip-10k` | 10k deflated entries, 82 MB unpacked, 10 changed |
| `tar-10k` | 10k uncompressed entries, 42 MB payload, 10 changed |
| `pdf-1000` | 1000 pages, one changed page |
| `docx-1000` | 1000 paragraphs in one document part |
| `docx-multipart` | 2000 paragraphs + 80 header/footer parts + 160 media entries |
| `odt-multipart` | 2000 paragraphs + 200 `Pictures/` entries |
| `pptx-500` | 500 slides, one changed |
| `xlsx-100` | 100 sheets × 30 × 10 cells, one cell changed |

The corpus generator uses only the Python standard library (ZIP/TAR/XML for
Office and archives, hand-written PDF objects), so it does not depend on
Office or a PDF toolchain.

## Results

| Case | 1.2.0 ms | 1.3.0 ms | Speedup | 1.2.0 peak MB | 1.3.0 peak MB | Δ memory | Read MB (old/new) | Expanded MB (old/new) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| `text-prefix` | 83.3 | 28.3 | 2.94× | 55.8 | 18.3 | −67% | 9.1 / 9.1 | 0 / 0 |
| `text-prefix-large` | 1390.9 | 160.5 | 8.67× | 548.3 | 291.0 | −47% | 93.1 / 93.1 | 0 / 0 |
| `text-insert` | 42.3 | 27.3 | 1.55× | 37.8 | 14.4 | −62% | 4.6 / 4.6 | 0 / 0 |
| `json-large` | 177.3 | 121.5 | 1.46× | 123.3 | 107.6 | −13% | 6.3 / 6.3 | 0 / 0 |
| `directory-10k` | 1720.6 | 1751.5 | 0.98× | 18.9 | 19.4 | +3% | 0.8 / 0.8 | 0 / 0 |
| `zip-10k` | 226.0 | 136.2 | 1.66× | 334.8 | 38.0 | −89% | 3.6 / 3.6 | 164 / 164 |
| `tar-10k` | 177.4 | 130.7 | 1.36× | 295.6 | 153.4 | −48% | 97.7 / 97.7 | 80.1 / 80.1 |
| `pdf-1000` | 248.1 | 26.7 | 9.29× | 15.3 | 18.0 | +18% | 0.5 / 0.5 | 0 / 0 |
| `docx-1000` | 11.6 | 10.6 | 1.09× | 3.8 | 3.9 | +3% | 0 / 0 | 0 / 0 |
| `docx-multipart` | 55.9 | 13.9 | 4.02× | 9.1 | 3.9 | −57% | 0.2 / 0.2 | 0 / 0 |
| `odt-multipart` | 48.4 | 13.4 | 3.61× | 9.3 | 3.8 | −59% | 0.1 / 0.1 | 0 / 0 |
| `pptx-500` | 29.8 | 12.6 | 2.37× | 9.1 | 3.7 | −59% | 0.3 / 0.3 | 0 / 0 |
| `xlsx-100` | 96.2 | 61.1 | 1.57× | 14.6 | 15.4 | +5% | 0.3 / 0.3 | 0 / 0 |

Representative real fixtures (best of five):

| Fixture | 1.2.0 ms | 1.3.0 ms | 1.2.0 peak MB | 1.3.0 peak MB |
|---|---:|---:|---:|---:|
| APK (`native-old`/`native-new`) | 18.3 | 16.5 | 4.6 | 4.6 |
| PDF (`old`/`new`) | 37.3 | 24.9 | 13.9 | 15.4 |
| DOCX (`old`/`new`) | 11.6 | 13.8 | 4.5 | 3.6 |
| XLSX (`workbook/old`/`new`) | 23.3 | 14.8 | 4.3 | 4.6 |
| ZIP (`zip/old`/`new`) | 14.7 | 11.5 | 4.2 | 4.1 |

## Allocation metrics

Allocation counts come from a local `#[global_allocator]` harness that calls
`DiffEngine::compare` directly with `jobs = 4` and `max_zip_entries = 20000`.
It counts every allocation/reallocation and the cumulative bytes requested;
this is allocation volume, not peak RSS. The harness is in
`.private/bench/alloc_harness/` and is built against either the current tree or
a detached `959cf7c` worktree for the baseline.

| Case | 1.2.0 allocations | 1.3.0 allocations | 1.2.0 allocated MB | 1.3.0 allocated MB |
|---|---:|---:|---:|---:|
| `text-prefix-large` | 4,000,584 | 471 | 1300.7 | 477.1 |
| `json-large` | 2,230,095 | 925,216 | 147.2 | 101.0 |
| `pdf-1000` | 484,641 | 149,647 | 132.1 | 30.9 |
| `docx-multipart` | 912,515 | 66,587 | 229.9 | 79.5 |
| `odt-multipart` | 366,437 | 19,633 | 157.4 | 61.0 |
| `zip-10k` | 481,843 | 522,576 | 1729.3 | 1749.2 |
| `tar-10k` | 358,125 | 240,866 | 586.7 | 375.0 |
| `directory-10k` | 401,977 | 405,856 | 52.6 | 55.1 |

ZIP allocates slightly more small objects in the streaming path, but holds far
less live memory (peak RSS −89%). TAR allocates 1.5× fewer objects and 36%
fewer cumulative bytes while cutting peak RSS by 48%.

## M1 acceptance

The previous 1.3.0 candidate used the original one-part DOCX benchmark and only
avoided repeated document loads. It measured PDF 1.85× and DOCX 1.05×, missing
the M1 targets. This round removes the remaining duplicate PDF content-stream
decode and adds realistic multipart Office corpora:

- PDF: text extraction and positioned blocks now share one decoded
  `Content` stream per page. **9.29×** on the 1000-page corpus (target ≥2×).
- DOCX: **4.02×** on the 2000-paragraph / 80-part / 160-media corpus
  (target ≥1.5×). The single-part 1000-paragraph file is at parity because it
  is only 3 KB and process startup dominates.
- ODT: **3.61×** on the 2000-paragraph / 200-media corpus (target ≥1.5×).
- PPTX: **2.37×**, XLSX: **1.57×**.

## M4 streaming coverage

- ZIP: central-directory scan once, on-demand payloads, 32 MiB changed-pair
  batches. Peak RSS −89% on 10k entries / 82 MB unpacked.
- Uncompressed TAR: header catalog stores byte ranges; payloads are sliced on
  demand. Compressed TAR streams still use the materialising reader because
  seeking inside gzip/bzip2/xz/zstd/lz4 is not generally possible.
- CPIO (newc/crc) and AR/DEB: byte-range catalogs with on-demand slices.
- 7z, RPM, CAB, ISO and XAR still materialise entries; their readers are
  random-access/decoder-specific and remain future work.
- Office/OpenDocument `Package` hashes media through one reused 64 KiB buffer
  (`Package::hash_entry`) instead of allocating a `Vec` per media part.

## What changed

- **PDF**: one `lopdf::Document` per side supplies text, positioned blocks,
  metadata and page geometry; the page content stream is decoded once and
  reused for both text and blocks; left and right parse with `rayon::join`.
- **Office/OpenDocument**: a shared `Package` keeps one open ZIP per side for
  DOCX/PPTX/ODT/ODP and XLSX metadata. Media hashing streams through a reused
  buffer. Left/right parsing runs in parallel.
- **Text**: common line prefix/suffix trimming, patience mode for large
  middles, early hunk stopping, a word-highlight threshold, and no full-text
  normalization copy when whitespace/case options are off.
- **Structured data**: equal subtrees short-circuit; array edges are trimmed;
  large arrays align on stable 128-bit content fingerprints instead of
  serialising every value.
- **Archives**: ZIP/TAR/CPIO/AR scan only metadata and read payloads on demand;
  changed ZIP pairs are materialised in bounded batches.
- **Intelligence**: directories and archives pair unmatched files by
  `(size, SHA-256)` as `renamed_from`; CSV/TSV infer a unique implicit key when
  no `--key` is given; worksheets detect moved rows/columns and normalise
  formula signatures.

## Known trade-offs

- Directory comparisons pay a small cost (within run-to-run noise, about 0–6%)
  for hashing only unmatched entries. This removes add+remove noise for renames
  and moves.
- The single-pass PDF reader keeps one document alive while deriving text,
  blocks and metadata, which can raise peak RSS slightly on small PDFs while
  cutting parse time by an order of magnitude.
- `docx-1000` and the small real fixtures are dominated by process startup and
  cache effects; the single-part DOCX case is effectively at parity.
- ZIP changed entries are read twice when their sizes collide: once to detect
  inequality and once for the detailed diff. Memory stays bounded by the batch
  budget; archives with very few changes benefit most.
- Text memory is still proportional to input size because decoded strings and
  line indexes are retained for the report. The large-text case now avoids a
  second full normalization copy.
- Uncompressed TAR catalogs still scan the whole archive once to find headers;
  streaming reduces retained payload memory, not the initial header-scan I/O.

## Reproduce

```powershell
python .private/bench/generate_corpus.py
powershell -File .private/bench/run_bench.ps1 `
  -Exe .private/bench/baseline-1.2.0.exe -Label baseline-1.2.0
powershell -File .private/bench/run_bench.ps1 `
  -Exe target/release/diff-everything.exe -Label candidate
```

The runner prints a CSV under `.private/bench/results/` with `ms`, `peak_mb`,
`read_mb`, `write_mb` and `expanded_mb`. Allocation counts are produced by the
local harness described above. On another machine, absolute times will differ;
compare the same-case ratio, read/expanded bytes and memory direction.
