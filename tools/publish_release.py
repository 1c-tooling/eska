#!/usr/bin/env python3
"""Publish a complete checked release, resuming a draft without replacing existing asset bytes."""

import json
import os
from pathlib import Path
import subprocess
import sys

from component_package import digest, require

REPOSITORY = "1c-tooling/eska"


def gh(*args):
    """Propagate authentication, API and transport failures without treating them as absence."""
    return subprocess.check_output(["gh", *args], text=True).strip()


def remote_assets(release_id):
    """Read every page because a component release contains more than thirty assets."""
    pages = json.loads(gh("api", f"repos/{REPOSITORY}/releases/{release_id}/assets", "--paginate", "--slurp"))
    assets = [item for page in pages for item in page]
    require(len({item["name"] for item in assets}) == len(assets), "duplicate-assets")
    return {item["name"]: item for item in assets}


def verified_assets(local, remote):
    """Published and resumed draft assets must match exact size and GitHub-provided SHA-256."""
    require(set(remote) <= set(local), "unexpected-assets", sorted(set(remote) - set(local)))
    for name, item in remote.items():
        require(item["state"] == "uploaded" and item["size"] == local[name].stat().st_size
                and item.get("digest") == "sha256:" + digest(local[name]), "asset-integrity", name)


def publish(directory, tag):
    """Only a tag workflow in the official repository may turn a complete draft into a release."""
    require(os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("GITHUB_REPOSITORY") == REPOSITORY
            and os.environ.get("GITHUB_REF") == "refs/tags/" + tag, "publication-context")
    assets = directory / "distrib"
    notes = directory / "release-notes.md"
    require(notes.is_file() and notes.stat().st_size > 0, "notes")
    local = {path.name: path for path in assets.iterdir()}
    require("SHA256SUMS" in local and all(path.is_file() and not path.is_symlink() for path in local.values()), "assets")
    checksums = {}
    for line in local["SHA256SUMS"].read_text(encoding="utf-8").splitlines():
        checksum, name = line.split("  ", 1)
        require(name in local and name not in checksums and digest(local[name]) == checksum, "checksum", name)
        checksums[name] = checksum
    require(set(checksums) == set(local) - {"SHA256SUMS"}, "checksums")
    pages = json.loads(gh("api", f"repos/{REPOSITORY}/releases", "--paginate", "--slurp"))
    release = next((item for page in pages for item in page if item["tag_name"] == tag), None)
    if release is None:
        gh("release", "create", tag, "--repo", REPOSITORY, "--verify-tag", "--draft", "--title", f"ESKA {tag}", "--notes-file", str(notes))
        release = json.loads(gh("api", f"repos/{REPOSITORY}/releases/tags/{tag}"))
    remote = remote_assets(release["id"])
    verified_assets(local, remote)
    missing = sorted(set(local) - set(remote))
    if not release["draft"]:
        require(not missing, "published-incomplete", missing)
        return
    for name in missing:
        gh("release", "upload", tag, str(local[name]), "--repo", REPOSITORY)
    remote = remote_assets(release["id"])
    verified_assets(local, remote)
    require(set(local) == set(remote), "incomplete-assets")
    gh("release", "edit", tag, "--repo", REPOSITORY, "--notes-file", str(notes), "--draft=false", "--latest")


if __name__ == "__main__":
    publish(Path(sys.argv[1]), sys.argv[2])
