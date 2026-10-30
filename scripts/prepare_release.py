#!/usr/bin/env python3
"""Resolve a release, create its annotated tag and avoid republishing versions.

The build runs in the SAME workflow: a tag pushed with GITHUB_TOKEN does not
trigger another push workflow. No personal access token is needed.
"""

import json
import os
import re
import subprocess
from pathlib import Path

from bump_version import SEMVER, check


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def release_plan(version, event, ref, sha, tags, releases, retry=False):
    """Pure decision logic; all tag/release mutations happen after validation."""
    tag = "v" + version
    if ref.startswith("refs/tags/") and ref != "refs/tags/" + tag:
        raise ValueError(f"Tag {ref} does not match app version {tag}")
    existing = next((r for r in releases if r["tag_name"] == tag), None)
    if existing and not existing["draft"]:
        return {"tag": tag, "sha": tags.get(tag, sha), "publish": False}
    # A normal commit with an unchanged version must never rebuild that release.
    if event == "push" and ref.startswith("refs/heads/") and tag in tags and not retry:
        return {"tag": tag, "sha": tags[tag], "publish": False}
    if ref.startswith("refs/tags/") and tags.get(tag) != sha:
        raise ValueError(f"{tag} does not point to the checked-out commit")
    if tag in tags and tags[tag] != sha:
        raise ValueError(f"{tag} already belongs to another commit; retry on that tag or bump the version")
    current = tuple(map(int, version.split(".")))
    for release in releases:
        old = release["tag_name"]
        if not release["draft"] and not release.get("prerelease") and re.fullmatch("v" + SEMVER, old):
            if tuple(map(int, old[1:].split("."))) >= current:
                raise ValueError(f"New version {tag} must be newer than published {old}")
    return {"tag": tag, "sha": sha, "publish": True}


def main():
    version = check()
    sha = run("git", "rev-parse", "HEAD")
    tags = {tag: run("git", "rev-list", "-n", "1", tag)
            for tag in run("git", "tag", "--list", "v*").splitlines()}
    pages = json.loads(run("gh", "api", "--paginate", "--slurp",
                           f"repos/{os.environ['GITHUB_REPOSITORY']}/releases?per_page=100"))
    releases = [release for page in pages for release in page]
    plan = release_plan(version, os.environ["GITHUB_EVENT_NAME"],
                        os.environ["GITHUB_REF"], sha, tags, releases,
                        retry=int(os.environ.get("GITHUB_RUN_ATTEMPT", "1")) > 1)
    if plan["publish"]:
        if not os.environ.get("UPDATER_SIGNING_KEY"):
            raise ValueError("Missing repository secret TAURI_SIGNING_PRIVATE_KEY")
        if plan["tag"] not in tags:
            run("git", "-c", "user.name=github-actions[bot]", "-c",
                "user.email=41898282+github-actions[bot]@users.noreply.github.com",
                "tag", "-a", plan["tag"], "-m", "Release " + plan["tag"], sha)
            run("git", "push", "origin", "refs/tags/" + plan["tag"])
    with Path(os.environ["GITHUB_OUTPUT"]).open("a") as output:
        for key, value in plan.items():
            output.write(f"{key}={str(value).lower() if isinstance(value, bool) else value}\n")
    print(f"{plan['tag']}: " + ("build and publish" if plan["publish"] else "already tagged/published; skipped"))


if __name__ == "__main__":
    try:
        main()
    except ValueError as error:
        raise SystemExit(str(error))
