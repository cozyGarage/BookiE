import os
import subprocess
import sys


def git(*args):
    return subprocess.run(["git", *args], check=True, capture_output=True, text=True).stdout.strip()


def main():
    event = os.environ["CI_EVENT_NAME"]
    requested_ref = os.environ.get("CI_REQUESTED_REF", "")
    revision = git("rev-parse", "HEAD")

    if event == "workflow_dispatch" and requested_ref:
        if requested_ref.startswith(("refs/pull/", "pull/")):
            raise SystemExit("workflow_dispatch cannot pin a pull request ref")
        branches = git("branch", "-r", "--contains", revision)
        tags = git("tag", "--contains", revision)
        if not any(line.strip().startswith("origin/") for line in branches.splitlines()) and not tags:
            raise SystemExit("pinned revision must be on an origin branch or tag")

    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.write(f"revision={revision}\n")
    print(f"tree under test: {revision}")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        print(f"git command failed: {error.cmd}", file=sys.stderr)
        raise SystemExit(error.returncode) from error
