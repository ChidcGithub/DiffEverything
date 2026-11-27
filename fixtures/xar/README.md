# XAR fixtures

Generated deterministically by a maintainer-local fixture generator:

- `left.xar` / `right.xar`: nested directories, modified/added/removed files.
- `mixed-left.xar` / `mixed-right.xar`: same files stored with zlib, bzip2,
  xz, lzma and raw (`application/octet-stream`) payloads.
- `links-left.xar` / `links-right.xar`: symbolic links with unchanged and
  changed targets.

The archives use a 28-byte big-endian XAR header and a zlib-compressed XML
TOC. Data offsets are relative to the heap. Payloads follow libarchive's
interpretation: `application/x-gzip` is a zlib stream, not a gzip wrapper.
`bsdtar -tf` and `bsdtar -xOf` can read the generated fixtures.
