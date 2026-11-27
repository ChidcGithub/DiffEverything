"""Generate single-file bzip2/xz fixtures for DiffEverything tests."""
import bz2
import lzma
import pathlib

ROOT = pathlib.Path(__file__).resolve().parent.parent / "fixtures" / "compression"
OLD = b"line one\nline two\nline three\n"
NEW = b"line one\nline THREE\nline four\n"


def main() -> None:
    ROOT.mkdir(parents=True, exist_ok=True)
    (ROOT / "old.txt.bz2").write_bytes(bz2.compress(OLD, compresslevel=9))
    (ROOT / "new.txt.bz2").write_bytes(bz2.compress(NEW, compresslevel=9))
    (ROOT / "old.txt.xz").write_bytes(lzma.compress(OLD, preset=9))
    (ROOT / "new.txt.xz").write_bytes(lzma.compress(NEW, preset=9))


if __name__ == "__main__":
    main()
