import argparse
from datetime import date
import hashlib
import json
from pathlib import Path
from pathlib import PurePosixPath
import posixpath
import re
import tarfile
import tomllib


COMMIT_PATTERN = re.compile(r"[0-9a-f]{40}")
VERSION_PATTERN = re.compile(r"[0-9]+\.[0-9]+\.[0-9]+(?:[a-z]+[0-9]+)?")
SHA256_PATTERN = re.compile(r"[0-9a-f]{64}")
TARGET = "x86_64-unknown-linux-gnu"


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        while block := stream.read(1024 * 1024):
            digest.update(block)
    return digest.hexdigest()


def require(condition: bool, message: str) -> None:
    if not condition:
        raise ValueError(message)


def archive_contains(path: Path, root: str, required_members: set[str]) -> None:
    found = set()
    try:
        with tarfile.open(path, "r:gz") as archive:
            for member in archive:
                parts = PurePosixPath(member.name).parts
                require(not posixpath.isabs(member.name) and ".." not in parts, "release archive has an unsafe path")
                require(parts and parts[0] == root, "release archive has an unexpected root")
                require(not member.issym() and not member.islnk(), "release archive contains a link")
                require(member.isfile() or member.isdir(), "release archive contains an unsupported file type")
                if member.name.rstrip("/") in required_members:
                    found.add(member.name.rstrip("/"))
    except (OSError, tarfile.TarError) as error:
        raise ValueError("release archive is unreadable") from error
    require(required_members <= found, "release archive is missing required files")


def validate_release_inputs(directory: Path, expected_commit: str, expected_version: str) -> dict[str, str]:
    require(COMMIT_PATTERN.fullmatch(expected_commit) is not None, "expected commit is not an immutable SHA")
    require(VERSION_PATTERN.fullmatch(expected_version) is not None, "expected version is invalid")
    metadata_files = list(directory.glob("bookie-*-*-inputs.json"))
    require(len(metadata_files) == 1, "expected exactly one release input metadata file")
    metadata_path = metadata_files[0]
    require(not metadata_path.is_symlink(), "release input metadata cannot be a symlink")
    require(metadata_path.stat().st_size <= 64 * 1024, "release input metadata is too large")
    try:
        metadata = json.loads(metadata_path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        raise ValueError("release input metadata is unreadable") from error
    require(isinstance(metadata, dict), "release input metadata is invalid")
    commit = metadata.get("commit", "")
    version = metadata.get("package_version", "")
    require(isinstance(commit, str) and isinstance(version, str), "release input identity is invalid")
    require(commit == expected_commit, "release input commit does not match the source revision")
    require(version == expected_version and metadata.get("source_version") == version,
            "release input version does not match the release")
    require(COMMIT_PATTERN.fullmatch(commit) is not None, "release input commit is not an immutable SHA")
    require(VERSION_PATTERN.fullmatch(version) is not None, "release input version is invalid")
    require(metadata.get("target") == TARGET, "release input has an unsupported target")
    for key in ("cargo", "rustc"):
        require(isinstance(metadata.get(key), str) and metadata[key], f"release input is missing {key} provenance")
    require(
        isinstance(metadata.get("workspace_roots"), list)
        and metadata["workspace_roots"]
        and all(isinstance(item, str) and item for item in metadata["workspace_roots"]),
        "release input is missing workspace feature provenance",
    )
    native_inputs = metadata.get("native_build_inputs")
    require(
        isinstance(native_inputs, dict)
        and all(
            isinstance(native_inputs.get(key), list)
            and native_inputs[key]
            and all(isinstance(item, str) and item for item in native_inputs[key])
            for key in ("depends", "makedepends", "checkdepends")
        ),
        "release input is missing native build provenance",
    )
    recipe_checksum = metadata.get("arch_recipe_sha256")
    require(
        isinstance(recipe_checksum, str) and SHA256_PATTERN.fullmatch(recipe_checksum) is not None,
        "release input has an invalid Arch recipe checksum",
    )
    release_date = metadata.get("release_date", "")
    require(isinstance(release_date, str), "release input has an invalid release date")
    try:
        date.fromisoformat(release_date)
    except ValueError as error:
        raise ValueError("release input has an invalid release date") from error
    source = directory / f"bookie-{version}-{commit}.tar.gz"
    vendor = directory / f"bookie-vendor-{version}-{commit}.tar.gz"
    for path in (source, vendor):
        require(path.is_file() and not path.is_symlink(), f"required release input is missing: {path.name}")
    for path, field in ((source, "source_archive_sha256"), (vendor, "vendor_archive_sha256")):
        expected = metadata.get(field, "")
        require(isinstance(expected, str), f"release input has an invalid {field}")
        require(SHA256_PATTERN.fullmatch(expected) is not None, f"release input has an invalid {field}")
        require(file_sha256(path) == expected, f"release input checksum does not match: {path.name}")
    root = f"BookiE-{commit}"
    source_members = {
        f"{root}/Cargo.toml",
        f"{root}/apps/linux/app/Cargo.toml",
        f"{root}/crates/agentd/Cargo.toml",
        f"{root}/crates/ssh/Cargo.toml",
        f"{root}/apps/linux/packaging/arch/PKGBUILD",
    }
    archive_contains(source, root, source_members)
    try:
        with tarfile.open(source, "r:gz") as archive:
            manifest = archive.extractfile(f"{root}/Cargo.toml")
            require(manifest is not None, "source archive has no workspace manifest")
            source_version = tomllib.loads(manifest.read().decode())["workspace"]["package"]["version"]
    except (OSError, KeyError, UnicodeDecodeError, tarfile.TarError, tomllib.TOMLDecodeError) as error:
        raise ValueError("source archive workspace version is unreadable") from error
    require(source_version == version, "source archive version does not match release metadata")
    try:
        with tarfile.open(source, "r:gz") as archive:
            recipe = archive.extractfile(f"{root}/apps/linux/packaging/arch/PKGBUILD")
            require(recipe is not None, "source archive has no Arch recipe")
            recipe_checksum = hashlib.sha256(recipe.read()).hexdigest()
    except (OSError, tarfile.TarError) as error:
        raise ValueError("source archive Arch recipe is unreadable") from error
    require(recipe_checksum == metadata["arch_recipe_sha256"], "Arch recipe checksum does not match source archive")
    archive_contains(vendor, root, {f"{root}/.cargo/config.toml", f"{root}/cargo-vendor"})
    return {"metadata": str(metadata_path), "source_archive": str(source), "vendor_archive": str(vendor)}


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("directory", type=Path)
    parser.add_argument("commit")
    parser.add_argument("version")
    arguments = parser.parse_args()
    try:
        result = validate_release_inputs(arguments.directory, arguments.commit, arguments.version)
    except ValueError as error:
        parser.error(str(error))
    print(json.dumps(result))


if __name__ == "__main__":
    main()
