"""Generate ZIP fixtures using bzip2, LZMA and zstd entry compression."""
import pathlib
import zipfile

ROOT = pathlib.Path(__file__).resolve().parent.parent / "fixtures" / "zip-methods"
OLD = {
    "a.txt": b"old line\n",
    "dir/config.json": b'{"mode":"light"}\n',
}
NEW = {
    "a.txt": b"new line\n",
    "dir/config.json": b'{"mode":"dark"}\n',
}


def main() -> None:
    ROOT.mkdir(parents=True, exist_ok=True)
    methods = {
        "bz2": zipfile.ZIP_BZIP2,
        "lzma": zipfile.ZIP_LZMA,
        "zstd": zipfile.ZIP_ZSTANDARD,
    }
    for label, method in methods.items():
        for side, files in (("old", OLD), ("new", NEW)):
            path = ROOT / f"{side}-{label}.zip"
            with zipfile.ZipFile(path, "w", compression=method) as archive:
                for name, data in files.items():
                    archive.writestr(name, data)


if __name__ == "__main__":
    main()
