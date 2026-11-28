#!/usr/bin/env python3
"""Copy package copyright texts for native libraries in a built AppDir."""

from __future__ import annotations

import argparse
import gzip
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import sys
import tempfile


OUTPUT = PurePosixPath("usr/share/doc/bookie/appimage-native-notices")


def run(args: list[str], *, required: bool = True) -> str:
    result = subprocess.run(args, capture_output=True, text=True, check=False)
    if result.returncode and required:
        detail = result.stderr.strip() or f"exit status {result.returncode}"
        raise RuntimeError(f"{' '.join(args[:2])} failed: {detail}")
    return result.stdout


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def build_id(path: Path) -> str | None:
    output = run(["readelf", "-n", str(path)], required=False)
    match = re.search(r"Build ID:\s*([0-9a-fA-F]+)", output)
    return match.group(1).lower() if match else None


def bundled_elfs(appdir: Path) -> list[Path]:
    files = []
    for relative in (PurePosixPath("usr/lib"), PurePosixPath("usr/lib64")):
        root = appdir / relative
        if not root.is_dir():
            continue
        for path in sorted(root.rglob("*")):
            if not path.is_file() or path.is_symlink():
                continue
            with path.open("rb") as source:
                if source.read(4) == b"\x7fELF":
                    files.append(path)
    if not files:
        raise RuntimeError("AppDir contains no ELF libraries under usr/lib or usr/lib64")
    return files


def package_owners(path_pattern: str, root: Path) -> list[tuple[str, Path]]:
    output = run(["dpkg-query", "-S", path_pattern], required=False)
    owners = []
    for line in output.splitlines():
        package, separator, raw_path = line.partition(": ")
        if separator and package and raw_path.startswith("/"):
            owners.append((package, root / raw_path.lstrip("/")))
    return owners


def source_for(artifact: Path, appdir: Path, root: Path, cache: dict) -> dict:
    artifact_id = build_id(artifact)
    artifact_hash = file_sha256(artifact)
    pattern = f"*/{artifact.name}"
    if pattern not in cache:
        cache[pattern] = package_owners(pattern, root)
    candidates = cache[pattern]
    matches = []
    for package, source in candidates:
        if not source.is_file():
            continue
        id_key, hash_key = ("build-id", source), ("sha256", source)
        if id_key not in cache:
            cache[id_key] = build_id(source)
        if hash_key not in cache:
            cache[hash_key] = file_sha256(source)
        source_id, source_hash = cache[id_key], cache[hash_key]
        match_method = "build-id" if artifact_id and source_id == artifact_id else None
        if match_method is None and artifact_hash == source_hash:
            match_method = "sha256"
        if match_method:
            matches.append((package, source, source_id, source_hash, match_method))
    packages = {match[0] for match in matches}
    if len(packages) != 1:
        relative = artifact.relative_to(appdir)
        reason = "no matching Debian package file" if not packages else "ambiguous Debian package owners"
        raise RuntimeError(f"cannot establish provenance for {relative}: {reason}")
    package = next(iter(packages))
    package_matches = [match for match in matches if match[0] == package]
    package_matches.sort(key=lambda match: str(match[1]))
    _, source, source_id, source_hash, method = package_matches[0]
    return {
        "path": artifact.relative_to(appdir).as_posix(),
        "build_id": artifact_id,
        "sha256": artifact_hash,
        "binary_package": package,
        "source_path": "/" + source.relative_to(root).as_posix(),
        "source_build_id": source_id,
        "source_sha256": source_hash,
        "match": method,
    }


def package_copyrights(package: str, root: Path) -> tuple[str, list[dict]]:
    version = run(["dpkg-query", "-W", "-f=${Version}", package]).strip()
    if not version:
        raise RuntimeError(f"dpkg-query returned no version for {package}")
    paths = run(["dpkg-query", "-L", package]).splitlines()
    copyright_paths = set()
    for path in paths:
        if not path.startswith("/usr/share/doc/"):
            continue
        if PurePosixPath(path).name in {"copyright", "copyright.gz"}:
            copyright_paths.add(path)
        else:
            doc_dir = root / path.lstrip("/")
            copyright_paths.update(
                f"{path}/{name}" for name in ("copyright", "copyright.gz")
                if (doc_dir / name).is_file()
            )
    copyright_paths = sorted(copyright_paths)
    if not copyright_paths:
        raise RuntimeError(f"{package} has no packaged copyright file under /usr/share/doc")
    files = []
    common_licenses = set()
    for raw_path in copyright_paths:
        source = root / raw_path.lstrip("/")
        if not source.is_file():
            raise RuntimeError(f"{package} lists missing copyright file {raw_path}")
        original = source.read_bytes()
        content = gzip.decompress(original) if raw_path.endswith(".gz") else original
        try:
            text = content.decode("utf-8")
            if not text.strip():
                raise RuntimeError(f"{package} has an empty copyright file: {raw_path}")
        except UnicodeDecodeError as error:
            raise RuntimeError(f"{package} has non-UTF-8 copyright text: {raw_path}") from error
        common_licenses.update(
            name.rstrip(".,;:)")
            for name in re.findall(r"/usr/share/common-licenses/([A-Za-z0-9.+-]+)", text)
        )
        relative_doc_path = PurePosixPath(raw_path).relative_to("/usr/share/doc")
        filename = relative_doc_path.with_name("copyright")
        files.append({
            "kind": "debian-copyright",
            "source_path": raw_path,
            "sha256": hashlib.sha256(original).hexdigest(),
            "included_path": filename.as_posix(),
            "included_sha256": hashlib.sha256(content).hexdigest(),
            "content": content,
        })
    for name in sorted(common_licenses):
        raw_path = f"/usr/share/common-licenses/{name}"
        source = root / raw_path.lstrip("/")
        if not source.is_file():
            raise RuntimeError(f"{package} references missing common license text {raw_path}")
        content = source.read_bytes()
        try:
            if not content.decode("utf-8").strip():
                raise RuntimeError(f"{package} has an empty common license text: {raw_path}")
        except UnicodeDecodeError as error:
            raise RuntimeError(f"{package} has non-UTF-8 common license text: {raw_path}") from error
        files.append({
            "kind": "common-license",
            "source_path": raw_path,
            "sha256": hashlib.sha256(content).hexdigest(),
            "included_path": (PurePosixPath("common-licenses") / name).as_posix(),
            "included_sha256": hashlib.sha256(content).hexdigest(),
            "content": content,
        })
    return version, files


def generate(appdir: Path, root: Path = Path("/")) -> tuple[int, int]:
    appdir = appdir.resolve()
    cache: dict = {}
    libraries = [source_for(path, appdir, root, cache) for path in bundled_elfs(appdir)]
    package_names = sorted({library["binary_package"] for library in libraries})
    packages = []
    copyrights_by_package = {}
    for name in package_names:
        version, copyrights = package_copyrights(name, root)
        copyrights_by_package[name] = copyrights
        package_libraries = [library for library in libraries if library["binary_package"] == name]
        license_files = []
        for copyright_file in copyrights:
            included = PurePosixPath("licenses") / name.replace(":", "_") / copyright_file["included_path"]
            license_files.append({key: value for key, value in copyright_file.items() if key != "content"} | {"path": included.as_posix()})
        packages.append({
            "binary_package": name,
            "version": version,
            "bundled_elfs": package_libraries,
            "notice_texts": license_files,
        })

    output = appdir / OUTPUT
    output.parent.mkdir(parents=True, exist_ok=True)
    temporary = Path(tempfile.mkdtemp(prefix="native-notices-", dir=output.parent))
    try:
        for package in packages:
            name = package["binary_package"]
            for copyright_file in copyrights_by_package[name]:
                relative = PurePosixPath("licenses") / name.replace(":", "_") / copyright_file["included_path"]
                destination = temporary / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(copyright_file["content"])
        manifest = {
            "format_version": 1,
            "generator": {
                "path": "scripts/update-appimage-native-notices.py",
                "sha256": file_sha256(Path(__file__).resolve()),
            },
            "packages": packages,
        }
        (temporary / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
        (temporary / "README.md").write_text(
            "# AppImage native dependency notices\n\n"
            "This directory contains the Debian copyright files shipped by the binary packages "
            "that own native ELF files bundled in this AppImage, plus any referenced common "
            "license texts. The files preserve their source text; licenses are not inferred from "
            "package names.\n\n"
            "`manifest.json` records each bundled file, its Debian package and version, the source "
            "package path, and SHA-256 checksums.\n",
            encoding="utf-8",
        )
        if output.exists():
            if output.is_dir() and not output.is_symlink():
                shutil.rmtree(output)
            else:
                output.unlink()
        temporary.rename(output)
    except BaseException:
        shutil.rmtree(temporary, ignore_errors=True)
        raise
    return len(libraries), len(packages)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--appdir", type=Path, required=True)
    parser.add_argument("--sysroot", type=Path, default=Path("/"))
    args = parser.parse_args()
    try:
        library_count, package_count = generate(args.appdir, args.sysroot.resolve())
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(f"AppImage native notices: {error}", file=sys.stderr)
        return 1
    print(f"Recorded {library_count} bundled ELF files from {package_count} Debian packages")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
