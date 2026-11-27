"""Generate deterministic newc cpio fixtures for DiffEverything tests."""
import gzip
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent / "fixtures" / "cpio"
OLD = {
    "usr": (b"", 0o040755),
    "usr/share": (b"", 0o040755),
    "usr/share/demo": (b"", 0o040755),
    "usr/share/demo/readme.txt": (b"Demo\nversion 1.0\n", 0o100644),
    "usr/share/demo/config.json": (b'{"mode":"light","retries":3}\n', 0o100644),
    "usr/bin/demo": (b"#!/bin/sh\necho old\n", 0o100755),
}
NEW = {
    "usr": (b"", 0o040755),
    "usr/share": (b"", 0o040755),
    "usr/share/demo": (b"", 0o040755),
    "usr/share/demo/readme.txt": (b"Demo\nversion 1.1\n", 0o100644),
    "usr/share/demo/config.json": (b'{"mode":"dark","retries":5}\n', 0o100644),
    "usr/bin/demo": (b"#!/bin/sh\necho new\nextra\n", 0o100755),
}


def build(files: dict[str, tuple[bytes, int]]) -> bytes:
    out = bytearray()
    for ino, (name, (data, mode)) in enumerate(files.items(), start=1):
        name_bytes = name.encode() + b"\0"
        fields = [
            ino, mode, 0, 0, 1, 0, len(data),
            0, 0, 0, 0, len(name_bytes), 0,
        ]
        out += b"070701" + b"".join(f"{value:08x}".encode() for value in fields)
        out += name_bytes
        out += b"\0" * (-len(out) % 4)
        out += data
        out += b"\0" * (-len(data) % 4)
    trailer = b"TRAILER!!!\0"
    fields = [0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, len(trailer), 0]
    out += b"070701" + b"".join(f"{value:08x}".encode() for value in fields)
    out += trailer
    out += b"\0" * (-len(out) % 4)
    return bytes(out)


def main() -> None:
    ROOT.mkdir(parents=True, exist_ok=True)
    old = build(OLD)
    new = build(NEW)
    (ROOT / "old.cpio").write_bytes(old)
    (ROOT / "new.cpio").write_bytes(new)
    (ROOT / "old.cpio.gz").write_bytes(gzip.compress(old, mtime=0))
    (ROOT / "new.cpio.gz").write_bytes(gzip.compress(new, mtime=0))


if __name__ == "__main__":
    main()
