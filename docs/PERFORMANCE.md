# Performance baseline and 1.3.0 results

This document records the M0 baseline and the M1–M4 engineering results. It is
meant to be reproducible: the corpus generator and runner live outside the
repository (`.private/bench/`) so the published tree stays small, and the
commands below rebuild the same inputs.

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
| CLI | `--quiet --jobs 4` |

Wall time is the best of three warm-cache runs. Peak working set is sampled
every 5 ms while the process runs. The baseline executable was the published
1.2.0 release binary; the candidate was rebuilt from this tree.

## Synthetic corpus

| Case | Shape |
|---|---|
| `text-prefix` | 200k lines (~4.8 MB), last 10 lines changed |
| `text-prefix-large` | 2M lines (~49 MB), last 10 lines changed |
| `text-insert` | 100k lines, one line inserted in the middle |
| `json-large` | 50k records (~3.3 MB), one field and one array tail changed |
| `directory-10k` | 10k small files, 10 changed, 100 renamed |
| `zip-10k` | 10k stored/deflated entries, 82 MB unpacked, 10 changed |
| `pdf-1000` | 1000 pages, one changed page |
| `docx-1000` | 1000 paragraphs, one changed |
| `pptx-500` | 500 slides, one changed |
| `xlsx-100` | 100 sheets × 30 × 10 cells, one cell changed |

The corpus generator uses only the Python standard library (ZIP/XML for
Office, hand-written PDF objects), so it does not depend on Office or a PDF
toolchain.

## Results

| Case | 1.2.0 ms | 1.3.0 ms | Speedup | 1.2.0 peak MB | 1.3.0 peak MB | Δ memory |
|---|---:|---:|---:|---:|---:|---:|
| `text-prefix` | 95.4 | 28.7 | 3.32× | 65.9 | 12.9 | −80% |
| `text-prefix-large` | 1595.6 | 158.7 | 10.05× | 548.4 | 291.0 | −47% |
| `text-insert` | 53.5 | 27.0 | 1.98× | 32.3 | 11.7 | −64% |
| `json-large` | 199.2 | 120.6 | 1.65× | 123.3 | 108.6 | −12% |
| `directory-10k` | 1882.0 | 1860.3 | 1.01× | 19.0 | 19.4 | +2% |
| `zip-10k` | 67.1 | 56.3 | 1.19× | 79.1 | 21.9 | −72% |
| `pdf-1000` | 268.9 | 145.5 | 1.85× | 14.6 | 20.3 | +39% |
| `docx-1000` | 11.2 | 10.7 | 1.05× | 4.1 | 3.9 | −5% |
| `pptx-500` | 27.4 | 13.1 | 2.09× | 9.1 | 3.8 | −58% |
| `xlsx-100` | 93.1 | 63.7 | 1.46× | 14.6 | 15.2 | +4% |

Representative real fixtures (best of three):

| Fixture | 1.2.0 ms | 1.3.0 ms | 1.2.0 peak MB | 1.3.0 peak MB |
|---|---:|---:|---:|---:|
| APK (`native-old`/`native-new`) | 20.5 | 17.9 | 3.8 | 3.9 |
| PDF (`old`/`new`) | 35.8 | 29.5 | 11.1 | 15.5 |
| DOCX (`old`/`new`) | 11.5 | 11.8 | 3.9 | 4.0 |
| XLSX (`workbook/old`/`new`) | 12.6 | 17.7 | 3.9 | 3.9 |
| ZIP (`zip/old`/`new`) | 12.1 | 11.6 | 4.6 | 3.7 |

## What changed

- **PDF**: one `lopdf::Document` load per side supplies text, positioned
  blocks, metadata and page geometry; left and right are parsed with
  `rayon::join`.
- **Office/OpenDocument**: a shared `Package` keeps one open ZIP per side for
  DOCX/PPTX/ODT/ODP and XLSX metadata. Left/right parsing runs in parallel.
- **Text**: common line prefix/suffix trimming, patience mode for large
  middles, early hunk stopping, a word-highlight threshold, and no full-text
  normalization copy when whitespace/case options are off.
- **Structured data**: equal subtrees short-circuit; array edges are trimmed;
  large arrays align on stable 128-bit content fingerprints instead of
  serialising every value.
- **Archives**: ZIP comparison scans the central directory once, reads entry
  payloads on demand, pre-checks unpacked sizes, and reuses cached hashes for
  rename detection. Changed pairs are materialised in bounded 32 MiB batches.
- **Intelligence**: directories and archives pair unmatched files by
  `(size, SHA-256)` as `renamed_from`; CSV/TSV infer a unique implicit key when
  no `--key` is given; worksheets detect moved rows/columns and keep formula
  signatures whitespace/case-normalised.

## Known trade-offs

- Directory comparisons pay a small cost (within run-to-run noise, about 0–6%
  on the 10k corpus) for hashing only unmatched entries. This removes add+remove noise for renames
  and moves. The trade-off is intentional and can be revisited if rename
  detection gets a CLI switch.
- The single-pass PDF reader keeps one document alive while deriving text,
  blocks and metadata, which can raise peak RSS slightly on small PDFs while
  cutting parse time.
- `docx-1000` and the small real fixtures are dominated by process startup and
  cache effects; the DOCX case is effectively at parity. Differences below
  ~15% are within run-to-run noise.
- ZIP changed entries are read twice when their sizes collide: once to detect
  inequality and once for the detailed diff. Memory stays bounded by the batch
  budget; archives with very few changes benefit most.
- Text memory is still proportional to input size because decoded strings and
  line indexes are retained for the report. The large-text case now avoids a
  second full normalization copy.

## Reproduce

```powershell
python .private/bench/generate_corpus.py
powershell -File .private/bench/run_bench.ps1 `
  -Exe .private/bench/baseline-1.2.0.exe -Label baseline-1.2.0
powershell -File .private/bench/run_bench.ps1 `
  -Exe target/release/diff-everything.exe -Label candidate
```

The runner prints a CSV under `.private/bench/results/`. On another machine,
absolute times will differ; compare the same-case ratio and memory direction.
