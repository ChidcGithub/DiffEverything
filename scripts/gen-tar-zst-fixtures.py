"""Generate deterministic tar.zst fixtures for DiffEverything tests."""
import gzip
import io
import pathlib
import tarfile

import zstandard

ROOT = pathlib.Path(__file__).resolve().parent.parent / "fixtures" / "archive-zst"
OLD = {
    "README.md": b"# Demo\n\nversion 1.0\n",
    "src/main.rs": b'fn main() {\n    println!("old");\n}\n',
    "config/settings.json": b'{"mode":"light","retries":3}\n',
}
NEW = {
    "README.md": b"# Demo\n\nversion 1.1\n",
    "src/main.rs": b'fn main() {\n    println!("new");\n}\n',
    "config/settings.json": b'{"mode":"dark","retries":5}\n',
    "extra.txt": b"added on the right\n",
}


def tar_bytes(files: dict[str, bytes]) -> bytes:
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w", format=tarfile.GNU_FORMAT) as archive:
        for name, data in files.items():
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mtime = 0
            info.uid = 0
            info.gid = 0
            info.uname = "root"
            info.gname = "root"
            info.mode = 0o644
            archive.addfile(info, io.BytesIO(data))
    return buffer.getvalue()


def main() -> None:
    ROOT.mkdir(parents=True, exist_ok=True)
    for name, value in (("old", OLD), ("new", NEW)):
        tar = tar_bytes(value)
        (ROOT / f"{name}.tar.zst").write_bytes(
            zstandard.ZstdCompressor(level=10).compress(tar)
        )
    (ROOT / "old.tar.gz").write_bytes(gzip.compress(tar_bytes(OLD), mtime=0))
    (ROOT / "old.log.zst").write_bytes(
        zstandard.ZstdCompressor(level=10).compress(b"line one\nline two\n")
    )
    (ROOT / "new.log.zst").write_bytes(
        zstandard.ZstdCompressor(level=10).compress(b"line one\nline three\nextra\n")
    )


if __name__ == "__main__":
    main()
