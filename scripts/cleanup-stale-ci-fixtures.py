#!/usr/bin/env python3
import datetime
import re
import subprocess


STALE_AFTER = datetime.timedelta(hours=3)
RUN_ID = re.compile(
    r"(?:forgejo|github)-\d+-\d+-[a-z0-9][a-z0-9_-]*|"
    r"nightly-\d+-[a-z0-9][a-z0-9_-]*"
)
COMPOSE_PROJECT = re.compile(r"(?:forgejo|github)-\d+-\d+-driver-tls")
INSPECT_FORMAT = (
    '{{.Id}}{{"\\t"}}{{.Created}}{{"\\t"}}'
    '{{index .Config.Labels "com.bookie.test-run"}}{{"\\t"}}'
    '{{index .Config.Labels "com.docker.compose.project"}}'
)


def docker(*args):
    return subprocess.run(
        ["docker", *args], check=True, capture_output=True, text=True, timeout=120
    ).stdout


def container_ids():
    ids = set()
    for label in ("com.bookie.test-run", "com.docker.compose.project"):
        ids.update(docker("ps", "--all", "--quiet", "--filter", f"label={label}").split())
    return ids


def inspect(container_id):
    output = docker("inspect", "--format", INSPECT_FORMAT, container_id).strip()
    container_id, created, run_id, project = output.split("\t", maxsplit=3)
    return {
        "id": container_id,
        "created": datetime.datetime.fromisoformat(created.replace("Z", "+00:00")),
        "run_id": run_id if RUN_ID.fullmatch(run_id) else "",
        "project": project if COMPOSE_PROJECT.fullmatch(project) else "",
    }


def old_enough(container, now):
    return now - container["created"] >= STALE_AFTER


def reconcile(now=None):
    now = datetime.datetime.now(datetime.timezone.utc) if now is None else now
    containers = [inspect(container_id) for container_id in sorted(container_ids())]
    stale_ids = []
    stale_projects = {}
    for container in containers:
        if container["project"]:
            stale_projects.setdefault(container["project"], []).append(container)
        elif container["run_id"] and old_enough(container, now):
            stale_ids.append(container["id"])
    stale_projects = {
        project: members
        for project, members in stale_projects.items()
        if all(old_enough(member, now) for member in members)
    }
    stale_ids.extend(member["id"] for members in stale_projects.values() for member in members)
    if stale_ids:
        docker("rm", "--force", "--volumes", *stale_ids)
        print(f"removed {len(stale_ids)} abandoned BookiE test container(s)")
    for project in stale_projects:
        for resource in ("network", "volume"):
            resource_ids = docker(
                resource,
                "ls",
                "--quiet",
                "--filter",
                f"label=com.docker.compose.project={project}",
            ).split()
            if resource_ids:
                docker(resource, "rm", *resource_ids)
        print(f"removed abandoned BookiE driver-tls project {project}")


if __name__ == "__main__":
    reconcile()
