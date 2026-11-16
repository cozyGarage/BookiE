import argparse
from pathlib import Path
import re
import subprocess
import sys

VERSION_NAME = re.compile(r"\bName:\s+GLIBC_(\d+(?:\.\d+)+)\b")


def version_tuple(value):
    return tuple(int(part) for part in value.split("."))


def required_glibc_versions(report):
    versions = []
    needs = False
    for line in report.splitlines():
        if line.startswith("Version needs section"):
            needs = True
            continue
        if line.startswith("Version "):
            needs = False
        if needs:
            match = VERSION_NAME.search(line)
            if match:
                versions.append(version_tuple(match.group(1)))
    return versions


def elf_files(appdir):
    for path in sorted(appdir.rglob("*")):
        if not path.is_file():
            continue
        with path.open("rb") as stream:
            if stream.read(4) == b"\x7fELF":
                yield path


def check_appdir(appdir, floor):
    files = list(elf_files(appdir))
    if not files:
        raise ValueError(f"no ELF files found under {appdir}")
    offenders = []
    for path in files:
        result = subprocess.run(
            ["readelf", "--version-info", "--wide", str(path)],
            capture_output=True,
            check=False,
            text=True,
        )
        if result.returncode != 0:
            raise ValueError(f"readelf failed for {path}: {result.stderr.strip()}")
        versions = required_glibc_versions(result.stdout)
        if versions and max(versions) > floor:
            required = ".".join(str(part) for part in max(versions))
            allowed = ".".join(str(part) for part in floor)
            offenders.append(f"{path}: requires GLIBC_{required}, above floor {allowed}")
    if offenders:
        raise ValueError("\n".join(offenders))


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("appdir", type=Path)
    parser.add_argument("--max", required=True, dest="maximum")
    return parser.parse_args()


def main():
    args = parse_args()
    try:
        floor = version_tuple(args.maximum)
        if len(floor) < 2:
            raise ValueError("glibc floor must contain at least major and minor versions")
        check_appdir(args.appdir, floor)
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1
    print(f"AppImage ELF requirements are within the glibc {args.maximum} floor")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
