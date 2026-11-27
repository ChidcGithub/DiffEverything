"""Generate deterministic pickle fixtures for DiffEverything tests."""
import json
import pathlib
import pickle

ROOT = pathlib.Path(__file__).resolve().parent.parent / "fixtures" / "pickle"
OLD = {
    "name": "widget",
    "version": 1,
    "ratio": 0.5,
    "enabled": True,
    "items": [
        {"id": 1, "tags": ["alpha", "beta"]},
        {"id": 2, "tags": ["gamma"]},
    ],
}
NEW = {
    "name": "widget",
    "version": 2,
    "ratio": 0.75,
    "enabled": True,
    "items": [
        {"id": 1, "tags": ["alpha", "delta"]},
        {"id": 3, "tags": ["gamma"]},
    ],
}


class Widget:
    pass


class Gadget:
    pass


def main() -> None:
    ROOT.mkdir(parents=True, exist_ok=True)
    for name, value in (("old", OLD), ("new", NEW)):
        with open(ROOT / f"{name}.pkl", "wb") as handle:
            pickle.dump(value, handle, protocol=4)
        with open(ROOT / f"{name}-protocol5.pkl", "wb") as handle:
            pickle.dump(value, handle, protocol=5)
    with open(ROOT / "old-global.pkl", "wb") as handle:
        pickle.dump({"thing": Widget(), "version": 1}, handle, protocol=4)
    with open(ROOT / "new-global.pkl", "wb") as handle:
        pickle.dump({"thing": Gadget(), "version": 2}, handle, protocol=4)
    with open(ROOT / "old.json", "w", encoding="utf-8", newline="\n") as handle:
        json.dump(OLD, handle, indent=2, sort_keys=True)
        handle.write("\n")


if __name__ == "__main__":
    main()
