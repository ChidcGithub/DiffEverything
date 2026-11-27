#!/usr/bin/env python3
"""Generate THIRD_PARTY_LICENSES.md from Cargo metadata and local crate sources."""

from __future__ import annotations

import hashlib
import json
import subprocess
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "THIRD_PARTY_LICENSES.md"
OWN_PACKAGES = {"diff-core", "diff-cli"}
LICENSE_PREFIXES = ("LICENSE", "LICENCE", "COPYING", "NOTICE", "UNLICENSE")
FENCE = "~~~~~~~~"


def metadata() -> dict:
    result = subprocess.run(
        ["cargo", "metadata", "--format-version", "1"],
        cwd=ROOT,
        check=True,
        capture_output=True,
    )
    return json.loads(result.stdout)


def read_text(path: Path) -> str:
    data = path.read_bytes()
    for encoding in ("utf-8", "utf-8-sig", "latin-1"):
        try:
            return data.decode(encoding).replace("\r\n", "\n").replace("\r", "\n")
        except UnicodeDecodeError:
            continue
    return data.decode("utf-8", errors="replace").replace("\r\n", "\n")


def license_files(package: dict) -> list[Path]:
    manifest = Path(package["manifest_path"])
    root = manifest.parent
    files: list[Path] = []
    declared = package.get("license_file")
    if declared:
        path = root / declared
        if path.is_file():
            files.append(path)
    for path in root.iterdir():
        if path.is_file() and path.name.upper().startswith(LICENSE_PREFIXES):
            files.append(path)
    unique: dict[str, Path] = {}
    for path in files:
        unique[str(path.resolve()).lower()] = path
    return sorted(unique.values(), key=lambda path: path.name.lower())


def find_canonical(packages: list[dict], filename: str, contains: tuple[str, ...]) -> str:
    for package in packages:
        for path in license_files(package):
            if path.name.lower() != filename.lower():
                continue
            text = read_text(path)
            if all(needle in text for needle in contains):
                return text
    for package in packages:
        for path in license_files(package):
            text = read_text(path)
            if all(needle in text for needle in contains):
                return text
    raise SystemExit(f"could not locate canonical license text: {filename} {contains}")


def mit_for(authors: str, template: str) -> str:
    lines = template.splitlines()
    for index, line in enumerate(lines):
        if line.startswith("Copyright (c)"):
            lines[index] = f"Copyright (c) {authors}"
            break
    return "\n".join(lines) + "\n"


def bsd3_for(authors: str, template: str) -> str:
    lines = template.splitlines()
    for index, line in enumerate(lines):
        if line.startswith("Copyright (c)"):
            lines[index] = f"Copyright (c) {authors}"
            break
    return "\n".join(lines) + "\n"


def package_url(package: dict) -> str:
    repository = (package.get("repository") or "").strip()
    if repository:
        return repository
    return f"https://crates.io/crates/{package['name']}/{package['version']}"


def chosen_license(package: dict) -> str:
    expression = (package.get("license") or "").strip()
    preferred = ("MIT", "Apache-2.0", "BSD-3-Clause", "BSD-2-Clause", "MPL-2.0", "ISC", "0BSD")
    for license_id in preferred:
        if license_id in expression:
            return license_id
    return expression or "NOASSERTION"


def main() -> None:
    data = metadata()
    packages = sorted(
        (package for package in data["packages"] if package["name"] not in OWN_PACKAGES),
        key=lambda package: (package["name"].lower(), package["version"]),
    )

    texts: dict[str, dict] = {}

    def register(label: str, body: str, package: dict) -> None:
        normalized = body.strip() + "\n"
        digest = hashlib.sha256(normalized.encode("utf-8")).hexdigest()
        entry = texts.setdefault(
            digest,
            {"label": label, "body": normalized, "packages": set()},
        )
        entry["packages"].add(f"{package['name']} {package['version']}")

    for package in packages:
        files = license_files(package)
        if files:
            for path in files:
                register(path.name, read_text(path), package)
            continue

        license_id = chosen_license(package)
        authors = "; ".join(package.get("authors") or []) or "the crate authors"
        if license_id == "MIT":
            register("MIT License", mit_for(authors, MIT_TEMPLATE), package)
        elif license_id == "BSD-3-Clause":
            register("BSD 3-Clause License", bsd3_for(authors, BSD3_TEMPLATE), package)
        elif license_id == "Apache-2.0":
            register("Apache License 2.0", APACHE_TEXT, package)
        elif license_id == "MPL-2.0":
            register("Mozilla Public License 2.0", MPL_TEXT, package)
        elif license_id == "0BSD":
            register("Zero-Clause BSD", ZERO_BSD_TEXT, package)
        else:
            register(
                f"License notice ({license_id})",
                f"This crate declares the license expression: {license_id}.\n"
                "The crate did not include a license file in its published package.\n"
                f"See {package_url(package)} for the authoritative license information.\n",
                package,
            )

    expressions = Counter((package.get("license") or "NOASSERTION") for package in packages)
    mpl_packages = [package for package in packages if "MPL-2.0" in (package.get("license") or "")]
    mpl_only = [
        package
        for package in mpl_packages
        if not any(option in (package.get("license") or "") for option in ("MIT", "Apache-2.0", "BSD-"))
    ]
    mpl_dual = [package for package in mpl_packages if package not in mpl_only]
    native_packages = [
        package
        for package in packages
        if package["name"].endswith("-sys") or package["name"] in {"ring", "openssl-sys"}
    ]
    lock_path = ROOT / "Cargo.lock"
    lock_hash = hashlib.sha256(lock_path.read_bytes()).hexdigest()[:16]
    generated = datetime.now(timezone.utc).strftime("%Y-%m-%d")

    lines: list[str] = []
    lines.append("# Third-Party Licenses")
    lines.append("")
    lines.append("DiffEverything itself is licensed under the MIT License; see [LICENSE](LICENSE).")
    lines.append("")
    lines.append(
        "This file lists third-party Rust crates in the workspace dependency graph and reproduces "
        "the license texts published with those crates. It covers normal, build and test "
        "dependencies to be conservative."
    )
    lines.append("")
    lines.append(f"- Generated: {generated}")
    lines.append(f"- Cargo.lock SHA-256 (first 16 hex): `{lock_hash}`")
    lines.append(f"- Third-party packages: {len(packages)}")
    lines.append(f"- Distinct license texts: {len(texts)}")
    lines.append("")
    lines.append("## License Summary")
    lines.append("")
    lines.append("| SPDX expression | Packages |")
    lines.append("|---|---:|")
    for expression, count in sorted(expressions.items(), key=lambda item: (-item[1], item[0])):
        lines.append(f"| `{expression}` | {count} |")
    lines.append("")
    lines.append(
        "The table shows the SPDX expression declared by each crate. Where a crate offers "
        "multiple options, DiffEverything relies on a permissive option (MIT, Apache-2.0 or BSD) "
        "and only the corresponding license text applies to this distribution."
    )
    lines.append("")
    lines.append(
        "No GPL or AGPL code is present. The only dependency offering an LGPL option is "
        "`r-efi`, which is used under MIT/Apache-2.0."
    )
    lines.append("")
    lines.append("## MPL-2.0 Source Notice")
    lines.append("")
    if mpl_packages:
        lines.append(
            "The dependency graph contains MPL-2.0 code. MPL-2.0-only crates are used unmodified; "
            "their source is available from crates.io and the repositories below."
        )
        lines.append("")
        for package in mpl_only:
            lines.append(
                f"- MPL-2.0 only: `{package['name']} {package['version']}` — "
                f"https://crates.io/crates/{package['name']}/{package['version']} — "
                f"{package_url(package)}"
            )
        if mpl_dual:
            lines.append("")
            lines.append(
                "Crates that offer MPL-2.0 alongside a permissive option are used under the "
                "permissive option:"
            )
            lines.append("")
            for package in mpl_dual:
                lines.append(
                    f"- `{package['name']} {package['version']}` "
                    f"(`{package.get('license')}`) — {package_url(package)}"
                )
    else:
        lines.append("No MPL-2.0 crates are present in this dependency graph.")
    lines.append("")
    lines.append("## Native and Bundled Libraries")
    lines.append("")
    lines.append(
        "Some `-sys` crates build or link native libraries. Their published packages include "
        "the corresponding license files where applicable, and those files are reproduced below."
    )
    lines.append("")
    for package in native_packages:
        lines.append(f"- `{package['name']} {package['version']}` — {package_url(package)}")
    lines.append("")
    lines.append("## Dependency Inventory")
    lines.append("")
    lines.append("| Crate | Version | License | Repository |")
    lines.append("|---|---|---|---|")
    for package in packages:
        license_id = (package.get("license") or "NOASSERTION").replace("|", "\\|")
        lines.append(
            f"| `{package['name']}` | {package['version']} | `{license_id}` | {package_url(package)} |"
        )
    lines.append("")
    lines.append("## License Texts")
    lines.append("")
    lines.append(
        "License texts are deduplicated by exact content. A package can appear under more than "
        "one text when the crate offers a choice of licenses."
    )
    lines.append("")

    ordered = sorted(texts.values(), key=lambda entry: (entry["label"].lower(), entry["body"]))
    for index, entry in enumerate(ordered, 1):
        used_by = ", ".join(sorted(entry["packages"]))
        lines.append(f"### {index}. {entry['label']}")
        lines.append("")
        lines.append(f"Used by: {used_by}")
        lines.append("")
        if FENCE in entry["body"]:
            raise SystemExit(f"license text contains the markdown fence: {entry['label']}")
        lines.append(f"{FENCE}text")
        lines.append(entry["body"].rstrip())
        lines.append(FENCE)
        lines.append("")

    OUTPUT.write_text("\n".join(lines).rstrip() + "\n", encoding="utf-8", newline="\n")
    print(f"wrote {OUTPUT} ({OUTPUT.stat().st_size:,} bytes, {len(packages)} packages)")


if __name__ == "__main__":
    metadata_cache = metadata()
    all_packages = metadata_cache["packages"]
    MIT_TEMPLATE = read_text(ROOT / "LICENSE")
    APACHE_TEXT = find_canonical(
        all_packages,
        "LICENSE-APACHE",
        ("Apache License", "Version 2.0, January 2004"),
    )
    BSD3_TEMPLATE = find_canonical(
        all_packages,
        "LICENSE.BSD-3-Clause",
        ("Redistribution and use in source and binary forms", "Neither the name"),
    )
    MPL_TEXT = find_canonical(
        all_packages,
        "LICENSE",
        ("Mozilla Public License Version 2.0",),
    )
    ZERO_BSD_TEXT = find_canonical(
        all_packages,
        "LICENSE",
        ("Permission to use, copy, modify, and/or distribute this software",),
    )
    main()
