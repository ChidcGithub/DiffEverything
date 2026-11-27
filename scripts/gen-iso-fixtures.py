"""Generate deterministic ISO9660 fixtures for DiffEverything tests."""
import io
import pathlib

import pycdlib

ROOT = pathlib.Path(__file__).resolve().parent.parent / "fixtures" / "iso"
OLD = {
    "/README.TXT": b"Demo ISO\nversion 1.0\n",
    "/SRC": None,
    "/SRC/MAIN.RS": b'fn main() {\n    println!("old");\n}\n',
    "/CONFIG.JSN": b'{"mode":"light","retries":3}\n',
}
NEW = {
    "/README.TXT": b"Demo ISO\nversion 1.1\n",
    "/SRC": None,
    "/SRC/MAIN.RS": b'fn main() {\n    println!("new");\n}\n',
    "/CONFIG.JSN": b'{"mode":"dark","retries":5}\n',
    "/EXTRA.TXT": b"added on the right\n",
}


def build(files: dict[str, bytes | None]) -> bytes:
    iso = pycdlib.PyCdlib()
    iso.new(interchange_level=1, vol_ident="DIFFDEMO")
    for path, data in files.items():
        if data is None:
            iso.add_directory(path)
            continue
        iso.add_fp(io.BytesIO(data), len(data), path + ";1")
    buffer = io.BytesIO()
    iso.write_fp(buffer)
    iso.close()
    return buffer.getvalue()


def main() -> None:
    ROOT.mkdir(parents=True, exist_ok=True)
    (ROOT / "old.iso").write_bytes(build(OLD))
    (ROOT / "new.iso").write_bytes(build(NEW))


if __name__ == "__main__":
    main()
