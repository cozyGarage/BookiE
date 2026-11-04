import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import tomllib
from unittest.mock import patch


def _write_executable(path: Path, body: str) -> None:
    path.write_text(body)
    path.chmod(path.stat().st_mode | stat.S_IEXEC)


def _stage_package(
    source: Path,
    root: Path,
    *,
    include_schema: bool = True,
    include_askpass: bool = True,
    include_notices: bool = True,
    package_name: str = "bookie",
    include_relations: bool = True,
) -> Path:
    version = tomllib.loads((source / "Cargo.toml").read_text())["workspace"]["package"]["version"] + "-1"
    stage = root / "stage"
    for directory in (
        stage / "usr/bin",
        stage / "usr/share/applications",
        stage / "usr/share/metainfo",
        stage / "usr/share/icons/hicolor/scalable/apps",
        stage / "usr/share/doc/bookie",
        stage / "usr/share/glib-2.0/schemas",
        stage / "DEBIAN",
    ):
        directory.mkdir(parents=True)
    os.chmod(stage / "DEBIAN", 0o755)
    _write_executable(stage / "usr/bin/bookie", "#!/bin/sh\nexit 0\n")
    _write_executable(
        stage / "usr/bin/bookie-agentd",
        "#!/bin/sh\necho 'usage: bookie-agentd --policy PATH'\n",
    )
    if include_askpass:
        _write_executable(stage / "usr/bin/bookie-askpass", "#!/bin/sh\nexit 0\n")
    (stage / "usr/share/applications/com.bookie.linux.desktop").write_text(
        (source / "apps/linux/flatpak/com.bookie.linux.desktop").read_text()
    )
    (stage / "usr/share/metainfo/com.bookie.linux.metainfo.xml").write_text(
        (source / "apps/linux/flatpak/com.bookie.linux.metainfo.xml").read_text()
    )
    (stage / "usr/share/icons/hicolor/scalable/apps/com.bookie.linux.svg").write_text(
        (source / "apps/linux/flatpak/icons/scalable/com.bookie.linux.svg").read_text()
    )
    if include_schema:
        shutil.copyfile(
            source / "apps/linux/data/com.bookie.linux.gschema.xml",
            stage / "usr/share/glib-2.0/schemas/com.bookie.linux.gschema.xml",
        )
    (stage / "usr/share/doc/bookie/LICENSE.md").write_text((source / "LICENSE.md").read_text())
    if include_notices:
        shutil.copyfile(
            source / "apps/linux/packaging/THIRD_PARTY_NOTICES.md",
            stage / "usr/share/doc/bookie/THIRD_PARTY_NOTICES.md",
        )
    (stage / "usr/share/doc/bookie/policy.example.toml").write_text(
        (source / "apps/linux/packaging/policy.example.toml").read_text()
    )
    (stage / "DEBIAN/control").write_text(
        f"Package: {package_name}\n"
        f"Version: {version}\n"
        "Section: database\n"
        "Priority: optional\n"
        "Architecture: amd64\n"
        "Maintainer: BookiE Contributors <noreply@bookie.app>\n"
        "Homepage: https://github.com/cozyGarage/BookiE\n"
        + ("Replaces: tablepro\nConflicts: tablepro\n" if include_relations else "")
        + "Description: Native Linux database client\n"
        " BookiE test fixture\n"
    )
    package = root / f"{package_name}_{version}_amd64.deb"
    result = subprocess.run(
        ["dpkg-deb", "--root-owner-group", "--build", str(stage), str(package)],
        capture_output=True,
        text=True,
    )
    if result.returncode:
        detail = result.stderr.strip() or result.stdout.strip() or "no diagnostic output"
        raise RuntimeError(f"dpkg-deb --build failed with exit code {result.returncode}: {detail}")
    return package


def check_builder_builds_and_installs_the_askpass_helper(source: Path) -> None:
    builder = (source / "scripts/build-deb.sh").read_text()
    assert "--bin bookie-askpass" in builder, "build-deb.sh does not build the SSH askpass helper"
    assert "usr/bin/bookie-askpass" in builder, "build-deb.sh does not install the SSH askpass helper"


def check_builder_installs_third_party_notices(source: Path) -> None:
    builder = (source / "scripts/build-deb.sh").read_text()
    assert "apps/linux/packaging/THIRD_PARTY_NOTICES.md" in builder
    assert "usr/share/doc/bookie/THIRD_PARTY_NOTICES.md" in builder


def check_package_test_reports_the_failing_command(source: Path) -> None:
    helper = source / "scripts/command-failure-context.sh"
    result = subprocess.run(
        ["bash", "-c", 'source "$1"; bookie_enable_command_failure_context; fail() { false; }; fail', "test", str(helper)],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 1
    assert "command failed at" in result.stderr, result.stderr
    assert "false" in result.stderr, result.stderr


def check_deb_build_failure_reports_stderr(source: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="bookie-deb-error-test-") as temporary:
        def run_failed_build(*args, **kwargs):
            if kwargs.get("check"):
                raise subprocess.CalledProcessError(2, args[0], output="", stderr="dpkg-deb: fixture failure")
            return subprocess.CompletedProcess(args[0], 2, "", "dpkg-deb: fixture failure")

        with patch("subprocess.run", side_effect=run_failed_build):
            try:
                _stage_package(source, Path(temporary))
            except RuntimeError as error:
                assert "dpkg-deb: fixture failure" in str(error), str(error)
            else:
                raise AssertionError("Debian package build failure was not reported")


def main() -> None:
    source = Path(__file__).resolve().parents[2]
    check_builder_builds_and_installs_the_askpass_helper(source)
    check_builder_installs_third_party_notices(source)
    check_package_test_reports_the_failing_command(source)
    check_deb_build_failure_reports_stderr(source)
    if shutil.which("dpkg-deb") is None:
        print("Debian package validation skipped: dpkg-deb is unavailable")
        return
    validator = source / "scripts/validate-deb-package.sh"
    with tempfile.TemporaryDirectory(prefix="bookie-deb-test-") as temporary:
        root = Path(temporary)
        old_umask = os.umask(0o077)
        try:
            valid = _stage_package(source, root / "valid")
        finally:
            os.umask(old_umask)
        subprocess.run(["bash", str(validator), str(valid)], check=True)
        missing_askpass = _stage_package(source, root / "missing-askpass", include_askpass=False)
        result = subprocess.run(["bash", str(validator), str(missing_askpass)], capture_output=True, text=True)
        assert result.returncode != 0
        assert "package is missing /usr/bin/bookie-askpass" in result.stderr, result.stderr
        missing_notices = _stage_package(source, root / "missing-notices", include_notices=False)
        result = subprocess.run(["bash", str(validator), str(missing_notices)], capture_output=True, text=True)
        assert result.returncode != 0
        assert "package is missing /usr/share/doc/bookie/THIRD_PARTY_NOTICES.md" in result.stderr, result.stderr
        missing_schema = _stage_package(source, root / "missing-schema", include_schema=False)
        result = subprocess.run(["bash", str(validator), str(missing_schema)], capture_output=True, text=True)
        assert result.returncode != 0
        assert "package is missing /usr/share/glib-2.0/schemas/com.bookie.linux.gschema.xml" in result.stderr, result.stderr
        old_name = _stage_package(source, root / "old-name", package_name="tablepro")
        result = subprocess.run(["bash", str(validator), str(old_name)], capture_output=True, text=True)
        assert result.returncode != 0
        assert "Debian package name must be bookie" in result.stderr, result.stderr
        no_relations = _stage_package(source, root / "no-relations", include_relations=False)
        result = subprocess.run(["bash", str(validator), str(no_relations)], capture_output=True, text=True)
        assert result.returncode != 0
        assert "must declare Replaces: tablepro" in result.stderr, result.stderr
    print("Debian package validation passed")


if __name__ == "__main__":
    main()
