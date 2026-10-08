#!/usr/bin/env bash
set -euo pipefail

branch="${1:-$(git rev-parse --abbrev-ref HEAD)}"
remote="${FORGEJO_REMOTE:-forgejo}"
token_file="${FORGEJO_TOKEN_FILE:-$HOME/.config/forgejo/token}"
api="${FORGEJO_API:-http://192.168.1.246:3000/api/v1/repos/trung/bookie}"
timeout_minutes="${FORGEJO_GATE_TIMEOUT_MINUTES:-60}"

sha="$(git rev-parse "$branch")"
git push --quiet "$remote" "$branch:$branch"
token="$(cat "$token_file")"
deadline=$((SECONDS + timeout_minutes * 60))

summarize() {
  curl -fsS -H "Authorization: token $token" "$api/actions/runs?limit=50" | python3 -c '
import json, sys
sha = sys.argv[1]
runs = [run for run in json.load(sys.stdin)["workflow_runs"] if run["commit_sha"] == sha]
if not runs:
    print("pending")
else:
    run = max(runs, key=lambda item: item["id"])
    print(run["status"], run["id"])
' "$sha"
}

while [ "$SECONDS" -lt "$deadline" ]; do
  read -r status run_id <<<"$(summarize)"
  case "$status" in
    success) echo "forgejo run $run_id passed for $sha"; exit 0 ;;
    failure|cancelled)
      echo "forgejo run $run_id $status for $sha"
      curl -fsS -H "Authorization: token $token" "$api/actions/tasks?limit=100" | python3 -c '
import json, sys
run = int(sys.argv[1])
for task in json.load(sys.stdin).get("workflow_runs", []):
    if task.get("run_number") == run and task.get("status") != "success":
        print("  ", task["status"], task["name"])
' "$run_id"
      exit 1 ;;
  esac
  sleep 30
done
echo "forgejo run for $sha did not finish within $timeout_minutes minutes" >&2
exit 2
