# Package fixtures

- `old.squashfs` / `new.squashfs` and `old.snap` / `new.snap` are copies of
  diffoscope's `test1.squashfs` / `test2.squashfs` test data.
- `old.ext4` / `new.ext4` are copies of diffoscope's `test1.ext4` / `test2.ext4`.
- `old.appimage` / `new.appimage` are synthetic AppImage-shaped files: an ELF
  runtime from `fixtures/executable/` with a SquashFS payload appended at a
  4096-byte boundary.
- `old.dmg` / `new.dmg` were generated with `udif` from small raw payloads.
- `old.msix` / `new.msix` are synthetic ZIP packages containing
  `AppxManifest.xml`, `AppxBlockMap.xml`, `[Content_Types].xml` and `app.exe`.
- `old.aab` / `new.aab` are synthetic AAB ZIPs containing `BundleConfig.pb`,
  `base/manifest/AndroidManifest.xml`, `base/dex/classes.dex` and
  `base/resources.pb`.
- `old.flatpak` / `new.flatpak` are synthetic bundle payloads with Flatpak-style
  ref, commit and metadata strings; they exercise the metadata/hash fallback
  used for OSTree bundle files without extracting OSTree objects.
