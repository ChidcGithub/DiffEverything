"""Generate tar.lz4 and single-file lz4 fixtures for DiffEverything tests."""
import io
import pathlib
import tarfile

import lz4.frame

ROOT = pathlib.Path(__file__).resolve().parent.parent / "fixtures" / "lz4"
OLD = {
    "README.md": b"# LZ4 demo\nversion 1.0\n",
    "src/main.rs": b'fn main() { println!("old"); }\n',
}
NEW = {
    "README.md": b"# LZ4 demo\nversion 1.1\n",
    "src/main.rs": b'fn main() { println!("new"); }\n',
    "extra.txt": b"added\n",
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
    for name, files in (("old", OLD), ("new", NEW)):
        (ROOT / f"{name}.tar.lz4").write_bytes(
            lz4.frame.compress(tar_bytes(files), compression_level=9)
        )
        text = b"line one\n" + (b"line two\n" if name == "old" else b"line THREE\nextra\n")
        (ROOT / f"{name}.log.lz4").write_bytes(
            lz4.frame.compress(text, compression_level=9)
        )


if __name__ == "__main__":
    main()
