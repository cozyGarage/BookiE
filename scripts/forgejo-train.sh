#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 2 ]; then
  echo "usage: $0 <remote-branch> <remote-branch>..." >&2
  exit 64
fi

root="$(git rev-parse --show-toplevel)"

bookie_remote() {
  if [ -n "${FORGEJO_TRAIN_REMOTE:-}" ]; then
    printf '%s\n' "$FORGEJO_TRAIN_REMOTE"
    return
  fi
  git -C "$root" remote -v | awk '$3 == "(fetch)" && $2 ~ /github\.com[:\/]cozyGarage\/BookiE(\.git)?$/ { print $1; exit }'
}

remote="$(bookie_remote)"
if [ -z "$remote" ]; then
  echo "train: no remote tracks github.com/cozyGarage/BookiE; set FORGEJO_TRAIN_REMOTE to the one that does" >&2
  exit 1
fi

git -C "$root" fetch --quiet "$remote" linux "$@"
work="$(mktemp -d)"
trap 'git -C "$root" worktree remove --force "$work" 2>/dev/null || rm -rf -- "$work"' EXIT
git -C "$root" worktree add --quiet --detach "$work" "${FORGEJO_TRAIN_BASE:-$remote/linux}"

merged=()
for branch in "$@"; do
  if git -C "$work" merge --quiet --no-ff -m "train: $branch" "$remote/$branch" >/dev/null 2>&1; then
    merged+=("$branch")
  else
    git -C "$work" merge --abort
    echo "train: $branch conflicts with the train so far; gate it on its own" >&2
  fi
done
[ "${#merged[@]}" -gt 0 ] || { echo "train: nothing to gate" >&2; exit 1; }

format_union_merges() {
  (cd "$work" && cargo fmt --all)
  local changed outside
  changed="$(git -C "$work" diff --name-only)"
  [ -n "$changed" ] || return 0
  outside="$(printf '%s\n' "$changed" | git -C "$work" check-attr --stdin merge | awk -F': ' '$3 != "union" { print $1 }')"
  if [ -n "$outside" ]; then
    echo "train: a merged branch is not formatted; gate it on its own to find which:" >&2
    printf '%s\n' "$outside" | sed 's/^/  /' >&2
    return 1
  fi
  git -C "$work" commit --quiet -am "train: format union-merged files"
}
format_union_merges

sha="$(git -C "$work" rev-parse HEAD)"
git -C "$root" update-ref "refs/heads/train/gate" "$sha"
[ -z "${FORGEJO_TRAIN_DRY:-}" ] || { echo "train: dry run, would gate ${merged[*]} ($sha)"; exit 0; }
echo "train: gating ${merged[*]} as one run ($sha)"
cd "$root"
scripts/forgejo-gate.sh train/gate "train/$(date +%m%d-%H%M)"
echo "train passed for: ${merged[*]}"
