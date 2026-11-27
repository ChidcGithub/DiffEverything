# Image fixtures

- `old.png` / `new.png`: PNG fixtures for pixel/region diff tests.
- `diffoscope-test1.ico` / `diffoscope-test2.ico`: ICO files from the
  diffoscope test suite:
  https://github.com/diffoscope/diffoscope/tree/master/tests/data

Both icons are 16x16 BMP/DIB entries. `test1` uses 4bpp/16 colors, `test2` uses
32bpp, which the ICO comparator reports as container/bit-depth changes.
