#!/usr/bin/env python3
"""Generate the Arch application's third-party license notice bundle."""

from __future__ import annotations

import argparse
import difflib
import hashlib
import json
import os
from itertools import islice
from pathlib import Path
import subprocess
import sys
import tomllib


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "apps/linux/packaging/THIRD_PARTY_NOTICES.md"
MANIFEST = ROOT / "apps/linux/app/Cargo.toml"
MANIFESTS = [
    (MANIFEST, ["kerberos"]),
    (ROOT / "crates/agentd/Cargo.toml", ["kerberos"]),
    (ROOT / "crates/ssh/Cargo.toml", []),
]
CONFIG = ROOT / "about.toml"


def generate() -> str:
    accepted = tomllib.loads(CONFIG.read_text())["accepted"]
    deny_allow = tomllib.loads((ROOT / "deny.toml").read_text())["licenses"]["allow"]
    if set(accepted) != set(deny_allow):
        raise RuntimeError("about.toml accepted licenses differ from deny.toml [licenses].allow")
    licenses_by_text: dict[tuple[str, str], set[str]] = {}
    for manifest, features in MANIFESTS:
        command = [
            os.environ.get("CARGO_ABOUT", "cargo-about"),
            "generate",
            "--manifest-path",
            str(manifest),
            "--format",
            "json",
            "--config",
            str(CONFIG),
            "--locked",
            "--fail",
        ]
        if features:
            command.extend(["--features", ",".join(features)])
        result = subprocess.run(command, cwd=ROOT, check=False, capture_output=True, text=True)
        if result.returncode:
            raise RuntimeError(result.stderr.strip() or f"cargo-about failed for {manifest.relative_to(ROOT)}")
        inventory = json.loads(result.stdout)
        licenses = inventory.get("licenses")
        if not isinstance(licenses, list) or not licenses:
            raise RuntimeError(f"cargo-about returned no license inventory for {manifest.relative_to(ROOT)}")
        for license_item in licenses:
            license_id = license_item.get("id")
            text = license_item.get("text")
            if not isinstance(license_id, str) or not isinstance(text, str) or not text.strip():
                raise RuntimeError(f"cargo-about returned incomplete license data: {license_item!r}")
            users = license_item.get("used_by", [])
            normalized_text = text.replace("\r\n", "\n").replace("\r", "\n").strip()
            key = (license_id, normalized_text)
            packages = licenses_by_text.setdefault(key, set())
            for entry in users:
                package = entry.get("crate", {}) if isinstance(entry, dict) else {}
                name, version = package.get("name"), package.get("version")
                if isinstance(name, str) and isinstance(version, str):
                    packages.add(f"{name} {version}")
    if not licenses_by_text:
        raise RuntimeError("cargo-about returned no license inventory")

    sections: list[str] = [
        "# Third-party software notices",
        "",
        "BookiE includes the third-party Rust packages listed below. License texts are generated from the locked GUI, agent daemon and askpass dependency graphs with cargo-about.",
        "",
        "This file records the license expressions declared by package authors and the corresponding license text resolved by cargo-about. Review upstream package metadata when redistributing modified builds.",
    ]
    for (license_id, text), package_set in sorted(licenses_by_text.items()):
        packages = sorted(package_set)
        if not packages:
            raise RuntimeError(f"license {license_id} has no attributed packages")
        sections.extend(["", f"## {license_id}", "", "Packages:", ""])
        sections.extend(f"- `{package}`" for package in packages)
        sections.extend(["", "```text", text.strip(), "```"])
    return "\n".join(sections) + "\n"


def cache_directory() -> Path:
    base = os.environ.get("XDG_CACHE_HOME") or str(Path.home() / ".cache")
    return Path(base) / "bookie-tools"


def input_digest() -> str:
    digest = hashlib.sha256()
    listing = subprocess.run(
        [
            "git", "ls-files", "-z", "--cached", "--others", "--exclude-standard", "--",
            "Cargo.lock", "*Cargo.toml", "*LICENSE*", "*LICENCE*", "*COPYING*", "*NOTICE*",
            "rust-toolchain", "rust-toolchain.toml", ".cargo/config", ".cargo/config.toml",
        ],
        cwd=ROOT,
        check=True,
        capture_output=True,
    ).stdout
    inputs = {path for path in listing.split(b"\0") if path}
    inputs |= {os.fsencode(str(path.relative_to(ROOT))) for path in (CONFIG, ROOT / "deny.toml", OUTPUT, Path(__file__).resolve())}
    inputs = sorted(inputs)
    for raw in inputs:
        path = ROOT / os.fsdecode(raw)
        digest.update(len(raw).to_bytes(8, "big") + raw)
        content = path.read_bytes() if path.exists() else b""
        digest.update(len(content).to_bytes(8, "big") + content)
    version = subprocess.run(
        [os.environ.get("CARGO_ABOUT", "cargo-about"), "--version"], cwd=ROOT, check=False, capture_output=True
    ).stdout
    digest.update(version)
    return digest.hexdigest()


MARKER_TEXT = "ok\n"


def read_marker(marker: Path) -> bool:
    try:
        return marker.read_text() == MARKER_TEXT
    except OSError:
        return False


def write_marker(marker: Path) -> None:
    try:
        marker.parent.mkdir(parents=True, exist_ok=True)
        pending = marker.with_name(f"{marker.name}.{os.getpid()}.tmp")
        pending.write_text(MARKER_TEXT)
        os.replace(pending, marker)
    except OSError:
        pass


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="fail when the checked-in notice bundle is stale")
    parser.add_argument(
        "--cached",
        action="store_true",
        help="with --check, skip the check when its inputs match a run that already passed on this machine",
    )
    args = parser.parse_args()
    marker = None
    if args.check and args.cached:
        try:
            marker = cache_directory() / f"notices-check-{input_digest()}.ok"
        except (OSError, ValueError, subprocess.CalledProcessError):
            marker = None
        if marker is not None and read_marker(marker):
            print("third-party notices: inputs unchanged since a passing check, skipped")
            return 0
    try:
        generated = generate()
    except (OSError, RuntimeError, json.JSONDecodeError) as error:
        print(f"third-party notice generation failed: {error}", file=sys.stderr)
        return 1
    if OUTPUT.exists():
        with OUTPUT.open("r", newline="") as notice_file:
            current = notice_file.read()
    else:
        current = None
    if args.check:
        if current != generated:
            print(f"{OUTPUT.relative_to(ROOT)} is stale; run python3 scripts/update-third-party-notices.py", file=sys.stderr)
            if current is not None:
                diff = difflib.unified_diff(
                    current.splitlines(keepends=True),
                    generated.splitlines(keepends=True),
                    fromfile=str(OUTPUT.relative_to(ROOT)),
                    tofile="generated",
                )
                print("".join(islice(diff, 80)), file=sys.stderr, end="")
            return 1
        if marker is not None:
            write_marker(marker)
        return 0
    OUTPUT.write_text(generated)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
