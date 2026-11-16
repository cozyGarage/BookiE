import argparse
from pathlib import Path
import re
import sys
import tomllib
import xml.etree.ElementTree as ET


ROOT = Path(__file__).resolve().parents[1]
CONFIG = "apps/linux/app/src/config.rs"
FLATPAK = "apps/linux/flatpak"
METAINFO = f"{FLATPAK}/com.bookie.linux.metainfo.xml"
SCHEMAS = "apps/linux/data"
PKGBUILD = "apps/linux/packaging/arch/PKGBUILD"
DEBIAN = "scripts/build-deb.sh"
NATIVE_PACKAGES = {
    "gtk4": ("gtk4", "libgtk-4-1"),
    "libadwaita-1": ("libadwaita", "libadwaita-1-0"),
    "gtksourceview-5": ("gtksourceview5", "libgtksourceview-5-0"),
    "glib-2.0": ("glib2", None),
}


def first(pattern, text):
    match = re.search(pattern, text, re.MULTILINE)
    return match.group(1) if match else None


def rust_constant(root, name):
    value = first(rf"pub const {name}: &str =([^;]*);", (root / CONFIG).read_text())
    strings = re.findall(r'"([^"]*)"', value or "")
    return strings[-1] if strings else None


def metainfo_text(root, path):
    element = ET.parse(root / METAINFO).find(path)
    return element.text.strip() if element is not None and element.text else None


def pkgbuild_array(root, name):
    body = first(rf"^{name}=\(([^)]*)\)", (root / PKGBUILD).read_text())
    return re.findall(r"'([^']*)'", body or "")


def pkgbuild_floors(root, name):
    floors = {}
    for entry in pkgbuild_array(root, name):
        match = re.fullmatch(r"([^<>=]+)>=(?:[0-9]+:)?(.+)", entry)
        if match:
            floors[match.group(1)] = match.group(2)
    return floors


def debian_floors(root):
    depends = first(r"^Depends: (.*)$", (root / DEBIAN).read_text())
    return dict(re.findall(r"([^\s,]+) \(>= ([^)]+)\)", depends or ""))


def meson_floors(root):
    return dict(re.findall(r"dependency\('([^']+)',\s*version:\s*'>=\s*([^']+)'", (root / "meson.build").read_text()))


def identifier_problems(root):
    app_id = rust_constant(root, "APP_ID")
    schema_id = rust_constant(root, "SCHEMA_ID")
    errors = []
    if app_id is None or schema_id is None:
        return [f"{CONFIG} must declare APP_ID and SCHEMA_ID"]
    if metainfo_text(root, "id") != app_id:
        errors.append(f"AppStream id differs from APP_ID {app_id}")
    if metainfo_text(root, "launchable[@type='desktop-id']") != f"{app_id}.desktop":
        errors.append(f"AppStream launchable differs from {app_id}.desktop")
    if not (root / FLATPAK / f"{app_id}.desktop").is_file():
        errors.append(f"{FLATPAK}/{app_id}.desktop is missing")
    if first(r"^application_id = '([^']+)'", (root / "meson.build").read_text()) != app_id:
        errors.append(f"Meson application_id differs from APP_ID {app_id}")
    schemas = sorted((root / SCHEMAS).glob("*.gschema.xml"))
    if schema_id not in {schema.get("id") for path in schemas for schema in ET.parse(path).iter("schema")}:
        errors.append(f"no GSettings schema in {SCHEMAS} has the id SCHEMA_ID {schema_id}")
    return errors


def icon_problems(root):
    icons = {path.stem for path in (root / FLATPAK / "icons").glob("*/*") if path.is_file()}
    errors = []
    for desktop in sorted((root / FLATPAK).glob("*.desktop")):
        icon = first(r"^Icon=(.*)$", desktop.read_text())
        if icon not in icons:
            errors.append(f"{desktop.name} Icon={icon} has no icon file under {FLATPAK}/icons")
    return errors


def license_problems(root):
    expected = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["license"]
    declared = {
        "Meson license": first(r"project\([^)]*license:\s*'([^']+)'", (root / "meson.build").read_text()),
        "PKGBUILD license": " ".join(pkgbuild_array(root, "license")),
        "AppStream project_license": metainfo_text(root, "project_license"),
    }
    return [f"{name} {value} differs from the workspace license {expected}" for name, value in declared.items() if value != expected]


def homepage_problems(root):
    expected = metainfo_text(root, "url[@type='homepage']")
    homepage = first(r"^Homepage: (.*)$", (root / DEBIAN).read_text())
    if homepage != expected:
        return [f"Debian Homepage {homepage} differs from the AppStream homepage {expected}"]
    return []


def native_floor_problems(root):
    arch = pkgbuild_floors(root, "depends")
    debian = debian_floors(root)
    errors = []
    for module, floor in meson_floors(root).items():
        arch_name, debian_name = NATIVE_PACKAGES.get(module, (None, None))
        for label, name, floors in (("PKGBUILD", arch_name, arch), ("Debian", debian_name, debian)):
            if name in floors and floors[name] != floor:
                errors.append(f"{label} {name} floor {floors[name]} differs from Meson {module} {floor}")
    return errors


def rust_toolchain_problems(root):
    expected = tomllib.loads((root / "Cargo.toml").read_text())["workspace"]["package"]["rust-version"]
    channel = tomllib.loads((root / "rust-toolchain.toml").read_text())["toolchain"]["channel"]
    errors = []
    if channel != expected:
        errors.append(f"rust-toolchain.toml channel {channel} differs from the workspace rust-version {expected}")
    floor = pkgbuild_floors(root, "makedepends").get("rust")
    if floor is not None and floor != expected:
        errors.append(f"PKGBUILD rust floor {floor} differs from the workspace rust-version {expected}")
    return errors


CHECKS = (
    identifier_problems,
    icon_problems,
    license_problems,
    homepage_problems,
    native_floor_problems,
    rust_toolchain_problems,
)


def problems(root):
    return [error for check in CHECKS for error in check(root)]


def main():
    argparse.ArgumentParser(description="Check BookiE packaging metadata consistency.").parse_args()
    try:
        errors = problems(ROOT)
    except (OSError, ValueError, KeyError, ET.ParseError) as error:
        print(f"packaging metadata check failed: {error}", file=sys.stderr)
        return 1
    if errors:
        print("\n".join(errors), file=sys.stderr)
        return 1
    print(f"packaging metadata: {rust_constant(ROOT, 'APP_ID')}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
