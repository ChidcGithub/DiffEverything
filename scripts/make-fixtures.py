"""Regenerate DiffEverything's generated fixtures.

The SQLite fixtures are synthesized locally. The ELF fixtures are extracted
from the real Debian hello packages downloaded by scripts/fetch-testdata.ps1.
"""

import io
import lzma
import os
import sqlite3
import tarfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SQLITE_DIR = os.path.join(ROOT, "fixtures", "sqlite")
EXECUTABLE_DIR = os.path.join(ROOT, "fixtures", "executable")
EXTERNAL_DIR = os.path.join(ROOT, "testdata", "external")


def make_sqlite(name, page_size, rows):
    path = os.path.join(SQLITE_DIR, name)
    if os.path.exists(path):
        os.remove(path)
    connection = sqlite3.connect(path)
    connection.execute(f"PRAGMA page_size={page_size}")
    connection.execute(
        "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, age INTEGER, photo BLOB)"
    )
    connection.execute("CREATE INDEX idx_users_name ON users(name)")
    connection.executemany("INSERT INTO users VALUES (?, ?, ?, ?)", rows)
    connection.commit()
    connection.close()


def ar_members(path):
    with open(path, "rb") as archive:
        if archive.read(8) != b"!<arch>\n":
            raise SystemExit(f"{path} is not an ar archive")
        while True:
            header = archive.read(60)
            if len(header) < 60:
                break
            name = header[:16].decode("ascii", "replace").strip().rstrip("/")
            size = int(header[48:58].decode("ascii").strip())
            data = archive.read(size)
            if size % 2:
                archive.read(1)
            yield name, data


def extract_hello(deb, destination):
    for name, data in ar_members(deb):
        if not name.startswith("data.tar"):
            continue
        raw = lzma.decompress(data) if name.endswith(".xz") else data
        with tarfile.open(fileobj=io.BytesIO(raw)) as tar:
            for member in tar.getmembers():
                if member.isfile() and member.name.endswith("usr/bin/hello"):
                    with open(destination, "wb") as output:
                        output.write(tar.extractfile(member).read())
                    return
    raise SystemExit(f"usr/bin/hello not found in {deb}")


def main():
    os.makedirs(SQLITE_DIR, exist_ok=True)
    os.makedirs(EXECUTABLE_DIR, exist_ok=True)

    make_sqlite(
        "old.db",
        4096,
        [(1, "alice", 30, b"\x00\xff"), (2, "bob", 25, None)],
    )
    make_sqlite(
        "old-copy.db",
        8192,
        [(2, "bob", 25, None), (1, "alice", 30, b"\x00\xff")],
    )
    make_sqlite(
        "new.db",
        4096,
        [
            (1, "alice", 31, b"\x00\xff"),
            (2, "bob", 25, None),
            (3, "carol", 41, b"\x01"),
        ],
    )

    extract_hello(
        os.path.join(EXTERNAL_DIR, "hello_2.10-2_amd64.deb"),
        os.path.join(EXECUTABLE_DIR, "hello-old"),
    )
    extract_hello(
        os.path.join(EXTERNAL_DIR, "hello_2.10-3_amd64.deb"),
        os.path.join(EXECUTABLE_DIR, "hello-new"),
    )
    print("generated SQLite and ELF fixtures")


if __name__ == "__main__":
    main()
