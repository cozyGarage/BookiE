#!/usr/bin/env python3
"""Point the community AUR recipe at one release tag.

Rewrites pkgver, pkgrel and the pinned commit of apps/linux/packaging/aur/PKGBUILD so a
moved tag fails the build. With --check it changes nothing and exits 1 when the recipe is stale.
"""

import argparse
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
RECIPE = ROOT / "apps/linux/packaging/aur/PKGBUILD"
TAG = re.compile(r"linux-v([0-9]+\.[0-9]+\.[0-9]+)")
FIELDS = (
    ("pkgver", re.compile(r"^pkgver=.*$", re.MULTILINE)),
    ("pkgrel", re.compile(r"^pkgrel=.*$", re.MULTILINE)),
    ("_commit", re.compile(r"^_commit=.*$", re.MULTILINE)),
)


def tag_commit(repo, tag):
    result = subprocess.run(
        ["git", "-C", str(repo), "rev-parse", "--verify", "--quiet", f"refs/tags/{tag}^{{commit}}"],
        capture_output=True,
        text=True,
        check=False,
    )
    commit = result.stdout.strip()
    if result.returncode != 0 or not re.fullmatch(r"[0-9a-f]{40}", commit):
        raise ValueError(f"tag {tag} does not resolve to a commit in {repo}")
    return commit


def updated(recipe, version, commit):
    values = {"pkgver": version, "pkgrel": "1", "_commit": commit}
    for name, pattern in FIELDS:
        if len(pattern.findall(recipe)) != 1:
            raise ValueError(f"the recipe must define {name} exactly once")
        recipe = pattern.sub(lambda _match, name=name: f"{name}={values[name]}", recipe)
    return recipe


def main(argv):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("tag", help="release tag such as linux-v0.2.7")
    parser.add_argument("--repo", type=Path, default=ROOT, help="repository that holds the tag")
    parser.add_argument("--recipe", type=Path, default=RECIPE)
    parser.add_argument("--output", type=Path, help="write the result here instead of replacing the recipe")
    parser.add_argument("--check", action="store_true", help="exit 1 when the recipe does not match the tag")
    args = parser.parse_args(argv)
    match = TAG.fullmatch(args.tag)
    if not match:
        print(f"invalid release tag: {args.tag}", file=sys.stderr)
        return 2
    try:
        current = args.recipe.read_text()
        result = updated(current, match.group(1), tag_commit(args.repo, args.tag))
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 2
    if args.check:
        if result != current:
            print(f"{args.recipe} does not match {args.tag}; run scripts/update-aur-recipe.py {args.tag}", file=sys.stderr)
            return 1
        return 0
    (args.output or args.recipe).write_text(result)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
