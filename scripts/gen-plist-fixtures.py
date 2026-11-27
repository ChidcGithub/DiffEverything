"""Generate deterministic plist fixtures for DiffEverything tests."""
import json
import pathlib
import plistlib

ROOT = pathlib.Path(__file__).resolve().parent.parent / "fixtures" / "plist"
OLD = {
    "CFBundleName": "DemoApp",
    "CFBundleVersion": "1.0",
    "Monitored": ["alpha", "beta"],
    "Settings": {"Theme": "light", "AutoSave": True},
}
NEW = {
    "CFBundleName": "DemoApp",
    "CFBundleVersion": "1.1",
    "Monitored": ["alpha", "gamma"],
    "Settings": {"Theme": "dark", "AutoSave": True, "Interval": 30},
}


def main() -> None:
    ROOT.mkdir(parents=True, exist_ok=True)
    for name, value in (("old", OLD), ("new", NEW)):
        with open(ROOT / f"{name}.plist", "wb") as handle:
            plistlib.dump(value, handle, fmt=plistlib.FMT_XML, sort_keys=True)
        with open(ROOT / f"{name}-binary.plist", "wb") as handle:
            plistlib.dump(value, handle, fmt=plistlib.FMT_BINARY, sort_keys=True)
    with open(ROOT / "old.json", "w", encoding="utf-8", newline="\n") as handle:
        json.dump(OLD, handle, indent=2, sort_keys=True)
        handle.write("\n")


if __name__ == "__main__":
    main()
