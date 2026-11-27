# Executable fixtures

- `hello-old` / `hello-new`: compiled ELF binaries used by the ELF semantic
  diff tests.
- `diffoscope-test1.macho` / `diffoscope-test2.macho`: real Mach-O binaries
  from the diffoscope test suite
  (https://github.com/diffoscope/diffoscope/tree/master/tests/data).
  `diffoscope-test1.macho` is a fat binary with i386 and x86_64 slices.
- `diffoscope-test1-mod.macho`: `diffoscope-test1.macho` with one byte
  changed inside the i386 slice, used to prove fat Mach-O section digest
  diffing.
- `diffoscope-test1.exe` / `diffoscope-test2.exe`: real PE binaries from the
  same diffoscope data directory.
