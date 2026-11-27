#!/usr/bin/env python3
"""Generate deterministic XAR archives used by the Rust test suite.

XAR layout: 28-byte big-endian header, zlib-compressed XML TOC, then the
heap. File data offsets in the TOC are relative to the heap start. Payloads
use the same zlib/bzip2/xz/lzma encodings as macOS packages (XAR's
"application/x-gzip" style is a zlib stream, matching libarchive).
"""

from __future__ import annotations

import bz2
import io
import lzma
import struct
import zlib
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "fixtures" / "xar"


@dataclass
class Symlink:
    target: str

ENCODINGS = {
    "zlib": ("application/x-gzip", lambda data: zlib.compress(data, 9)),
    "bzip2": ("application/x-bzip2", lambda data: bz2.compress(data, 9)),
    "xz": ("application/x-xz", lambda data: lzma.compress(data, format=lzma.FORMAT_XZ)),
    "lzma": ("application/x-lzma", lambda data: lzma.compress(data, format=lzma.FORMAT_ALONE)),
    "none": ("application/octet-stream", lambda data: data),
}


def write_xar(path: Path, files: dict[str, bytes | tuple[bytes, str]]) -> None:
    tree: dict = {}
    for name, value in files.items():
        node = tree
        parts = name.split("/")
        for part in parts[:-1]:
            node = node.setdefault(part, {})
        node[parts[-1]] = value

    ids = iter(range(1, 1000))
    heap = io.BytesIO()

    def emit(name: str, value, indent: str) -> str:
        file_id = next(ids)
        if isinstance(value, dict):
            children = "".join(emit(child, data, indent + "  ") for child, data in value.items())
            return (
                f'{indent}<file id="{file_id}">'
                f"<name>{name}</name><type>directory</type>"
                f"<mode>0755</mode>"
                f"{children}</file>"
            )
        if isinstance(value, Symlink):
            return (
                f'{indent}<file id="{file_id}">'
                f"<name>{name}</name><type>symlink</type>"
                f"<link>{value.target}</link>"
                "</file>"
            )
        data, codec = value if isinstance(value, tuple) else (value, "zlib")
        style, compress = ENCODINGS[codec]
        offset = heap.tell()
        compressed = compress(data)
        heap.write(compressed)
        return (
            f'{indent}<file id="{file_id}">'
            f"<name>{name}</name><type>file</type>"
            f"<mode>0644</mode>"
            "<data>"
            f"<length>{len(compressed)}</length>"
            f"<offset>{offset}</offset>"
            f"<size>{len(data)}</size>"
            f'<encoding style="{style}"/>'
            "</data></file>"
        )

    body = "".join(emit(name, value, "  ") for name, value in tree.items())
    xml = (
        '<?xml version="1.0" encoding="UTF-8"?>\n'
        "<xar><toc>"
        "<creation-time>2026-11-27T02:00:00</creation-time>"
        '<checksum style="none"/>'
        f"{body}"
        "</toc></xar>"
    ).encode()
    compressed = zlib.compress(xml, 9)
    header = struct.pack(
        ">IHHQQI",
        0x78617221,
        28,
        1,
        len(compressed),
        len(xml),
        0,
    )
    path.write_bytes(header + compressed + heap.getvalue())


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    write_xar(
        OUT / "left.xar",
        {
            "hello.txt": b"hello xar\n",
            "docs/readme.md": b"# XAR fixture\n",
            "docs/data.bin": bytes(range(64)),
        },
    )
    write_xar(
        OUT / "right.xar",
        {
            "hello.txt": b"hello xar!\n",
            "docs/readme.md": b"# XAR fixture\n",
            "logs/app.log": b"line one\nline two\n",
        },
    )
    codecs = ("zlib", "bzip2", "xz", "lzma", "none")
    write_xar(
        OUT / "mixed-left.xar",
        {f"codec/{codec}.txt": (b"alpha\n", codec) for codec in codecs},
    )
    write_xar(
        OUT / "mixed-right.xar",
        {f"codec/{codec}.txt": (b"beta\n", codec) for codec in codecs},
    )
    write_xar(
        OUT / "links-left.xar",
        {
            "target.txt": Symlink("hello.txt"),
            "same.txt": Symlink("docs/readme.md"),
        },
    )
    write_xar(
        OUT / "links-right.xar",
        {
            "target.txt": Symlink("docs/readme.md"),
            "same.txt": Symlink("docs/readme.md"),
        },
    )
    for name in (
        "left.xar",
        "right.xar",
        "mixed-left.xar",
        "mixed-right.xar",
        "links-left.xar",
        "links-right.xar",
    ):
        path = OUT / name
        print(name, path.stat().st_size)


if __name__ == "__main__":
    main()
