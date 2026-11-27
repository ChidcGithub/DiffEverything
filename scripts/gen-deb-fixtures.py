"""Generate deterministic .deb fixtures for DiffEverything tests."""
import io
import json
import pathlib
import tarfile

ROOT = pathlib.Path(__file__).resolve().parent.parent / "fixtures" / "deb"


def tar_gz(files: dict[str, bytes]) -> bytes:
    buffer = io.BytesIO()
    with tarfile.open(fileobj=buffer, mode="w:gz", format=tarfile.GNU_FORMAT) as archive:
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


def ar_member(name: str, data: bytes) -> bytes:
    header = (
        f"{name + '/':<16}{0:<12}{0:<6}{0:<6}{0:<8}{len(data):<10}`\n"
    ).encode("ascii")
    padding = b"\n" if len(data) % 2 else b""
    return header + data + padding


def build(version: str, mode: str, retries: int, description: str) -> bytes:
    control = (
        "Package: demo\n"
        f"Version: {version}\n"
        "Architecture: all\n"
        "Maintainer: Demo <demo@example.com>\n"
        f"Description: {description}\n"
    ).encode()
    config = json.dumps(
        {"mode": mode, "retries": retries}, indent=2, sort_keys=True
    ).encode()
    data = tar_gz(
        {
            "usr/share/demo/version.txt": f"{version}\n".encode(),
            "usr/share/demo/config.json": config + b"\n",
            "usr/bin/demo": f"#!/bin/sh\necho demo {version}\n".encode(),
        }
    )
    return b"".join(
        [
            b"!<arch>\n",
            ar_member("debian-binary", b"2.0\n"),
            ar_member("control.tar.gz", tar_gz({"./control": control})),
            ar_member("data.tar.gz", data),
        ]
    )


def main() -> None:
    ROOT.mkdir(parents=True, exist_ok=True)
    (ROOT / "old.deb").write_bytes(
        build("1.0", "light", 3, "Demo package for diff testing")
    )
    (ROOT / "new.deb").write_bytes(
        build("1.1", "dark", 5, "Demo package with updated settings")
    )


if __name__ == "__main__":
    main()
