#!/usr/bin/env python3
"""Refresh the GitHub contributor avatar list in CONTRIBUTORS.md and README.md.

Fetches contributors from the GitHub API (via `gh api`, or urllib with
GITHUB_TOKEN / GH_TOKEN when `gh` is unavailable), then rewrites the generated
files. Avatar URLs use https://github.com/<login>.png?size=64 so the list does
not depend on API-hosted avatar paths.

Usage (from the repository root):

    python3 scripts/generate-contributors.py
    python3 scripts/generate-contributors.py --check
    python3 scripts/generate-contributors.py --include-bots
    python3 scripts/generate-contributors.py --repo cozyGarage/BookiE

Authentication: `gh` uses its logged-in account. Without `gh`, set GITHUB_TOKEN
or GH_TOKEN for higher API rate limits; unauthenticated requests are limited.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.request
from pathlib import Path

REPOSITORY = "cozyGarage/BookiE"
REQUEST_TIMEOUT_SECONDS = 60
AVATAR_SIZE = 64
START_MARKER = "<!-- contributors:start -->"
END_MARKER = "<!-- contributors:end -->"
ROOT = Path(__file__).resolve().parents[1]
CONTRIBUTORS_PATH = ROOT / "CONTRIBUTORS.md"
README_PATH = ROOT / "README.md"
USER_AGENT = "bookie-generate-contributors"


def parse_repo(value: str) -> tuple[str, str]:
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", value):
        raise argparse.ArgumentTypeError("--repo must be OWNER/NAME")
    owner, name = value.split("/", 1)
    return owner, name


def is_bot(entry: dict) -> bool:
    login = entry.get("login")
    if not isinstance(login, str) or not login:
        return True
    if entry.get("type") == "Bot":
        return True
    return login.endswith("[bot]")


def normalize_contributors(raw: list[dict], *, include_bots: bool) -> list[dict]:
    seen: set[str] = set()
    people: list[dict] = []
    for entry in raw:
        if not isinstance(entry, dict):
            raise ValueError("contributor record must be an object")
        login = entry.get("login")
        if not isinstance(login, str) or not login:
            raise ValueError("contributor record missing login")
        if not include_bots and is_bot(entry):
            continue
        if login in seen:
            continue
        seen.add(login)
        people.append({"login": login, "type": entry.get("type", "User")})
    return people


def fetch_via_gh(owner: str, name: str, runner=None) -> tuple[list[dict] | None, str | None]:
    run = subprocess.run if runner is None else runner
    endpoint = f"repos/{owner}/{name}/contributors?per_page=100&anon=false"
    try:
        result = run(
            ["gh", "api", "--paginate", "--slurp", endpoint],
            capture_output=True,
            text=True,
            timeout=REQUEST_TIMEOUT_SECONDS,
            check=False,
        )
    except FileNotFoundError:
        return None, "gh CLI is not installed"
    except subprocess.TimeoutExpired:
        return None, f"gh api timed out after {REQUEST_TIMEOUT_SECONDS} seconds"

    if result.returncode != 0:
        detail = result.stderr.strip() or f"exit status {result.returncode}"
        return None, f"gh api failed: {detail}"

    try:
        pages = json.loads(result.stdout)
    except json.JSONDecodeError:
        return None, "gh api returned invalid JSON"

    if not isinstance(pages, list) or any(not isinstance(page, list) for page in pages):
        return None, "gh api returned an unexpected response shape"

    people: list[dict] = []
    for page in pages:
        for entry in page:
            if not isinstance(entry, dict):
                return None, "gh api returned an unexpected contributor record"
            people.append(entry)
    return people, None


def api_headers() -> dict[str, str]:
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": USER_AGENT,
        "X-GitHub-Api-Version": "2022-11-28",
    }
    token = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if token:
        headers["Authorization"] = f"Bearer {token}"
    return headers


def fetch_via_urllib(owner: str, name: str, opener=None) -> tuple[list[dict] | None, str | None]:
    open_url = urllib.request.urlopen if opener is None else opener
    headers = api_headers()
    people: list[dict] = []
    page = 1
    while True:
        url = (
            f"https://api.github.com/repos/{owner}/{name}/contributors"
            f"?per_page=100&anon=false&page={page}"
        )
        request = urllib.request.Request(url, headers=headers)
        try:
            with open_url(request, timeout=REQUEST_TIMEOUT_SECONDS) as response:
                body = response.read().decode("utf-8")
                link = response.headers.get("Link", "")
        except urllib.error.HTTPError as error:
            detail = error.read().decode("utf-8", errors="replace").strip() or error.reason
            return None, f"GitHub API HTTP {error.code}: {detail}"
        except urllib.error.URLError as error:
            return None, f"GitHub API request failed: {error.reason}"

        try:
            batch = json.loads(body)
        except json.JSONDecodeError:
            return None, "GitHub API returned invalid JSON"

        if not isinstance(batch, list):
            return None, "GitHub API returned an unexpected response shape"
        if not batch:
            break
        for entry in batch:
            if not isinstance(entry, dict):
                return None, "GitHub API returned an unexpected contributor record"
            people.append(entry)
        if 'rel="next"' not in link:
            break
        page += 1
    return people, None


def fetch_contributors(owner: str, name: str, *, runner=None, opener=None) -> tuple[list[dict] | None, str | None]:
    people, error = fetch_via_gh(owner, name, runner=runner)
    if people is not None:
        return people, None
    if error and "not installed" not in error:
        urllib_people, urllib_error = fetch_via_urllib(owner, name, opener=opener)
        if urllib_people is not None:
            return urllib_people, None
        return None, f"{error}; urllib fallback failed: {urllib_error}"
    return fetch_via_urllib(owner, name, opener=opener)


def avatar_markdown(login: str, size: int = AVATAR_SIZE) -> str:
    profile = f"https://github.com/{login}"
    image = f"{profile}.png?size={size}"
    return (
        f'<a href="{profile}" title="{login}">'
        f'<img src="{image}" width="{size}" height="{size}" alt="{login}"/>'
        f"</a>"
    )


def avatar_grid(people: list[dict], *, size: int = AVATAR_SIZE) -> str:
    if not people:
        return "_No contributors returned by the GitHub API._"
    cells = "\n  ".join(avatar_markdown(person["login"], size=size) for person in people)
    return f'<p align="center">\n  {cells}\n</p>'


def contributors_document(people: list[dict], repo: str, *, size: int = AVATAR_SIZE) -> str:
    count = len(people)
    grid = avatar_grid(people, size=size)
    lines = [
        "# Contributors",
        "",
        f"Thank you to everyone who has contributed to [`{repo}`](https://github.com/{repo}).",
        "",
        "This file is generated. Refresh it from the repository root with:",
        "",
        "```bash",
        "python3 scripts/generate-contributors.py",
        "```",
        "",
        "Optional flags: `--check` (verify without writing), `--include-bots`,",
        f"`--repo OWNER/NAME` (default `{REPOSITORY}`).",
        "",
        "Authentication uses the `gh` CLI when available. Without `gh`, set",
        "`GITHUB_TOKEN` or `GH_TOKEN` for authenticated API access.",
        "",
        f"Listed below: **{count}** contributor{'s' if count != 1 else ''} (bots omitted by default).",
        "",
        START_MARKER,
        grid,
        END_MARKER,
        "",
    ]
    return "\n".join(lines)


def readme_section(people: list[dict], *, size: int = AVATAR_SIZE) -> str:
    grid = avatar_grid(people, size=size)
    return "\n".join(
        [
            START_MARKER,
            grid,
            "",
            "See [CONTRIBUTORS.md](CONTRIBUTORS.md) for the full generated list and how to refresh it.",
            END_MARKER,
        ]
    )


def replace_marked_section(text: str, replacement: str) -> str:
    start = text.find(START_MARKER)
    end = text.find(END_MARKER)
    if start < 0 or end < 0 or end < start:
        raise ValueError(f"missing {START_MARKER} / {END_MARKER} markers")
    end += len(END_MARKER)
    return text[:start] + replacement.strip() + text[end:]


def ensure_readme_section(text: str, section_body: str) -> str:
    if START_MARKER in text and END_MARKER in text:
        return replace_marked_section(text, section_body)
    block = "\n".join(
        [
            "## Contributors",
            "",
            "People who have contributed commits to this repository:",
            "",
            section_body.strip(),
            "",
        ]
    )
    anchor = "## Contributing\n"
    if anchor in text:
        return text.replace(anchor, block + "\n" + anchor, 1)
    return text.rstrip() + "\n\n" + block


def write_if_changed(path: Path, content: str, *, check: bool) -> bool:
    previous = path.read_text(encoding="utf-8") if path.exists() else None
    if previous == content:
        return False
    if check:
        raise SystemExit(f"{path.relative_to(ROOT)} is out of date; run python3 scripts/generate-contributors.py")
    path.write_text(content, encoding="utf-8")
    return True


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--repo", type=parse_repo, default=parse_repo(REPOSITORY), help="GitHub repository OWNER/NAME")
    parser.add_argument("--include-bots", action="store_true", help="keep GitHub Apps and bot accounts")
    parser.add_argument("--check", action="store_true", help="exit non-zero when generated files would change")
    parser.add_argument("--avatar-size", type=int, default=AVATAR_SIZE, help="avatar pixel size (default 64)")
    args = parser.parse_args(argv)
    owner, name = args.repo
    repo = f"{owner}/{name}"

    raw, error = fetch_contributors(owner, name)
    if error is not None:
        print(f"contributor list unavailable: {error}", file=sys.stderr)
        return 2

    people = normalize_contributors(raw, include_bots=args.include_bots)
    size = args.avatar_size
    if size < 16 or size > 512:
        parser.error("--avatar-size must be between 16 and 512")
    document = contributors_document(people, repo, size=size)
    section = readme_section(people, size=size)
    readme = ensure_readme_section(README_PATH.read_text(encoding="utf-8"), section)
    changed = []
    if write_if_changed(CONTRIBUTORS_PATH, document, check=args.check):
        changed.append("CONTRIBUTORS.md")
    if write_if_changed(README_PATH, readme if readme.endswith("\n") else readme + "\n", check=args.check):
        changed.append("README.md")

    if args.check:
        print(f"contributor list up to date ({len(people)} people)")
        return 0

    if changed:
        print(f"updated {', '.join(changed)} ({len(people)} people)")
    else:
        print(f"already up to date ({len(people)} people)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
