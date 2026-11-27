"""Create tar.bz2 / tar.xz fixtures from fixtures/dir for archive tests."""
from pathlib import Path
import io
import tarfile

root = Path(__file__).resolve().parent.parent
out = root / "fixtures" / "archives"
out.mkdir(parents=True, exist_ok=True)

for mode, suffix in [("bz2", "tar.bz2"), ("xz", "tar.xz")]:
    for side in ("left", "right"):
        source = root / "fixtures" / "dir" / side
        target = out / f"{'old' if side == 'left' else 'new'}.{suffix}"
        with tarfile.open(target, f"w:{mode}") as archive:
            archive.add(source, arcname=".")
        print(target)


def add_file(archive: tarfile.TarFile, name: str, data: bytes) -> None:
    info = tarfile.TarInfo(name)
    info.size = len(data)
    archive.addfile(info, io.BytesIO(data))


def add_link(archive: tarfile.TarFile, name: str, target: str, kind: bytes) -> None:
    info = tarfile.TarInfo(name)
    info.type = kind
    info.linkname = target
    archive.addfile(info)


for side in ("left", "right"):
    path = out / f"links-{'old' if side == 'left' else 'new'}.tar"
    with tarfile.open(path, "w", format=tarfile.GNU_FORMAT) as archive:
        add_file(archive, "hello.txt", b"hello\n")
        add_file(archive, "docs/readme.md", b"# readme\n")
        add_link(archive, "target.txt", "docs/readme.md" if side == "left" else "hello.txt", tarfile.SYMTYPE)
        add_link(archive, "same.txt", "hello.txt", tarfile.SYMTYPE)
        add_link(archive, "hard.txt", "hello.txt", tarfile.LNKTYPE)
    print(path)
