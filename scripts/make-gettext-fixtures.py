#!/usr/bin/env python3
"""Generate small GNU gettext .mo catalogs with plural forms and context."""

from __future__ import annotations

import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "fixtures" / "gettext"

HEADER = (
    b"Project-Id-Version: DiffEverything test\n"
    b"MIME-Version: 1.0\n"
    b"Content-Type: text/plain; charset=UTF-8\n"
    b"Content-Transfer-Encoding: 8bit\n"
    b"Plural-Forms: nplurals=2; plural=(n > 1);\n"
)


def write_mo(path: Path, entries: list[tuple[bytes, bytes]]) -> None:
    count = len(entries)
    original_table = 28
    translation_table = original_table + count * 8
    data_offset = translation_table + count * 8
    originals = []
    translations = []
    data = bytearray()
    for original, translation in entries:
        originals.append((len(original), data_offset + len(data)))
        data.extend(original)
        translations.append((len(translation), data_offset + len(data)))
        data.extend(translation)

    out = bytearray()
    out.extend(
        struct.pack(
            "<IIIIIII",
            0x950412DE,
            0,
            count,
            original_table,
            translation_table,
            0,
            0,
        )
    )
    out.extend(b"\0" * (count * 8 * 2))
    out.extend(data)
    for index, (length, offset) in enumerate(originals):
        struct.pack_into("<II", out, original_table + index * 8, length, offset)
    for index, (length, offset) in enumerate(translations):
        struct.pack_into("<II", out, translation_table + index * 8, length, offset)
    path.write_bytes(out)


def main() -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    write_mo(
        OUT / "plural-old.mo",
        [
            (b"", HEADER),
            (b"file\0files", b"fichier\0fichiers"),
            (b"menu\x04Open", b"Ouvrir"),
        ],
    )
    write_mo(
        OUT / "plural-new.mo",
        [
            (b"", HEADER),
            (b"file\0files", b"document\0documents"),
            (b"menu\x04Open", b"Ouvrir"),
        ],
    )
    for name in ("plural-old.mo", "plural-new.mo"):
        path = OUT / name
        print(name, path.stat().st_size)


if __name__ == "__main__":
    main()
