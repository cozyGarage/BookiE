from pathlib import Path
import os
import shlex
import sys


def main():
    if len(sys.argv) != 3:
        raise SystemExit("usage: resolve-arch-rc-pkgbuild.py PKGBUILD OUTPUT")
    source_path = Path(sys.argv[1])
    output_path = Path(sys.argv[2])
    source = source_path.read_text()
    assignments = {
        "pkgver": os.environ["TABLEPRO_RC_VERSION"],
        "pkgrel": os.environ["TABLEPRO_RC_PKGREL"],
        "_commit": os.environ["TABLEPRO_RC_COMMIT"],
        "_archive_sha256": os.environ["TABLEPRO_RC_SHA256"],
        "_archive": os.environ["TABLEPRO_RC_ARCHIVE"],
        "_vendor_sha256": os.environ["TABLEPRO_RC_VENDOR_SHA256"],
        "_vendor_archive": os.environ["TABLEPRO_RC_VENDOR_ARCHIVE"],
        "_metadata": os.environ["TABLEPRO_RC_METADATA"],
        "_metadata_sha256": os.environ["TABLEPRO_RC_METADATA_SHA256"],
    }
    lines = source.splitlines()
    for name, value in assignments.items():
        matches = [index for index, line in enumerate(lines) if line.startswith(f"{name}=")]
        if len(matches) != 1:
            raise SystemExit(f"expected one {name} assignment in PKGBUILD")
        lines[matches[0]] = f"{name}={shlex.quote(value)}"
    source_line = 'source=("$(basename "$_archive")" "$(basename "$_vendor_archive")")'
    if lines.count(source_line) != 1:
        raise SystemExit("expected the candidate source declaration in PKGBUILD")
    source_names = (Path(assignments["_archive"]).name, Path(assignments["_vendor_archive"]).name)
    resolved_sources = "source=(" + " ".join(map(shlex.quote, source_names)) + ")"
    lines[lines.index(source_line)] = resolved_sources
    output_path.write_text("\n".join(lines) + "\n")


if __name__ == "__main__":
    main()
