# APK / Android manifest fixtures

`old.apk` and `new.apk` are copied from the diffoscope test suite:

- https://github.com/diffoscope/diffoscope/tree/master/tests/data

The only entry is the binary `AndroidManifest.xml`; the two manifests differ
in `android:versionCode` (1 vs 2) and `android:versionName` (1.0 vs 2.0).
`manifest1.axml` / `manifest2.axml` are extracted copies used by parser unit
tests. The decoded tree was cross-checked with androguard's `AXMLPrinter`.

`code-old.apk` / `code-new.apk` are real APKs from
https://github.com/krossovochkin/SmallestAPK; their `classes.dex` files
decompile to different `onCreate(Bundle)` Java source.

`native-old.apk` / `native-new.apk` contain the ELF fixtures as
`lib/x86_64/libhello.so` plus `classes.dex`; full mode recursively
disassembles the embedded `.so` with the embedded pure-Rust decoder.
