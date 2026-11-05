#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
APP_SOURCE = ROOT / "apps/linux/app/src"


def call_arguments(source, call):
    found = []
    start = 0
    while (position := source.find(call, start)) >= 0:
        opening = position + len(call)
        depth = 1
        end = opening
        for offset, character in enumerate(source[opening:], opening):
            if character == "(":
                depth += 1
            elif character == ")":
                depth -= 1
            if depth == 0:
                end = offset
                break
        found.append(source[opening:end].strip())
        start = opening
    return found


def is_plain_or_protected(argument):
    return (
        argument.startswith('"')
        or argument.startswith(("crate::tr!", "tr!", "&crate::tr!", "&tr!"))
        or any(
            marker in argument
            for marker in ("escape_markup(", "markup_escape_text(", "glib_escaped(")
        )
    )


def statements(source, start):
    found = []
    offset = 0
    while (position := source.find(start, offset)) >= 0:
        remainder = source[position:]
        end = remainder.find(".build()")
        found.append(remainder[: end if end >= 0 else min(len(remainder), 900)])
        offset = position + len(start)
    return found


def source_violations(path, source):
    offenders = []
    kinds = (
        "ActionRow",
        "ExpanderRow",
        "SwitchRow",
        "ComboRow",
        "EntryRow",
        "PasswordEntryRow",
    )
    for kind in kinds:
        for statement in statements(source, f"adw::{kind}::builder()"):
            dynamic = any(
                not is_plain_or_protected(argument)
                for call in (".subtitle(", ".title(")
                for argument in call_arguments(statement, call)
            )
            if dynamic and "use_markup(false)" not in statement:
                offenders.append(f"{path}: row text parses as markup")

    for call in (".description(", ".set_description(Some("):
        for argument in call_arguments(source, call):
            if not is_plain_or_protected(argument):
                offenders.append(f"{path}: {call}{argument}")

    for statement in statements(source, "adw::Banner::builder()"):
        if "use_markup(false)" not in statement:
            offenders.append(f"{path}: banner builder parses its title as markup")
    for constructor, protection in (
        ("adw::Banner::new(", "set_use_markup(false)"),
        ("&adw::Banner {", "set_use_markup: false"),
    ):
        offset = 0
        while (position := source.find(constructor, offset)) >= 0:
            window = source[position : position + (160 if constructor.endswith("new(") else 260)]
            if protection not in window:
                offenders.append(f"{path}: banner declaration parses its title as markup")
            offset = position + 1

    if "Toast::new(" in source or "Toast::builder(" in source:
        offenders.append(f"{path}: toast bypasses the plain-text helper")
    return offenders


def production_sources():
    return sorted(
        path
        for path in APP_SOURCE.rglob("*.rs")
        if "tests" not in path.relative_to(APP_SOURCE).parts
        and path.name != "plain_text.rs"
        and path.name != "tests.rs"
        and not path.name.endswith("_tests.rs")
    )


def check_paths(paths):
    return [
        problem
        for path in paths
        for problem in source_violations(path.relative_to(ROOT), path.read_text(encoding="utf-8"))
    ]


def main():
    problems = check_paths(production_sources())
    if problems:
        print("\n".join(problems))
        return 1
    print("gtk markup: ok")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
