#!/usr/bin/env python3
"""Measure private source trees and controlled system-Git history; never claim ESKA/gix timings."""

import argparse
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import time


def storage(root):
    """Count logical and allocated bytes, including directories but excluding symlinks."""
    result = {"files": 0, "directories": 0, "logical_bytes": 0, "allocated_bytes": 0}
    pending = [root]
    while pending:
        for path in pending.pop().iterdir():
            info = path.lstat()
            result["allocated_bytes"] += info.st_blocks * 512
            if stat.S_ISDIR(info.st_mode):
                result["directories"] += 1
                pending.append(path)
            elif stat.S_ISREG(info.st_mode):
                result["files"] += 1
                result["logical_bytes"] += info.st_size
            else:
                raise ValueError("filesystem alias or special entry")
    return result


def command(root, args, records, label, env):
    """Measure one subprocess with native argv and capture output without shell interpolation."""
    started = time.monotonic()
    result = subprocess.run(args, cwd=root, env=env, capture_output=True)
    records.append({"operation": label, "seconds": time.monotonic() - started,
                    "exit_code": result.returncode})
    if result.returncode:
        raise RuntimeError(result.stderr.decode("utf-8", errors="replace"))
    return result.stdout


def git(root, args, records, label, env):
    """Isolate fixture Git settings and disable shared attributes and automatic background GC."""
    return command(root, ["git", "-c", "core.attributesFile=/dev/null", *args], records, label, env)


def attributes(base, lfs, template):
    """Keep every byte and classify immutable extensionless blocks using their original paths."""
    lines = ["* -text"]
    extensions = set()
    if lfs:
        for line in template.read_text().splitlines():
            if line.startswith("*.") and "filter=lfs" in line:
                lines.append(line)
                extensions.add(line.split()[0][1:])
        manifest = json.loads((base / "accepted.json").read_text())
        hashes = {entry["sha256"] for entry in manifest["files"]
                  if Path(entry["path"]).suffix.lower() in extensions}
        for digest in sorted(hashes):
            lines.append(f"base/blobs/{digest[:2]}/{digest} filter=lfs diff=lfs merge=lfs -text")
    return "\n".join(lines) + "\n"


def change(root, kind, args, owner):
    """Apply the same allowed Comment and BSL edit, preserving all other source bytes."""
    target = root / "src" / (owner if kind == "typed" else args.property_file)
    if kind == "typed":
        shutil.copyfile(args.edited_source / owner, target)
    else:
        original = target.read_bytes()
        replacement = b"<Comment>ESKA property acceptance</Comment>"
        matches = original.count(b"<Comment/>") + original.count(b"<Comment />")
        if matches != 1:
            raise ValueError("control requires one existing empty Comment")
        target.write_bytes(original.replace(b"<Comment/>", replacement).replace(b"<Comment />", replacement))
    shutil.copyfile(args.edited_source / args.module, root / "src" / args.module)


def benchmark(root, kind, lfs, args, owner, env):
    """Use a fresh repository for a two-commit history and repeated clean/dirty operations."""
    roots = {"original": args.original, "normalized": args.normalized, "typed": args.source}
    root.mkdir()
    shutil.copytree(roots[kind], root / "src")
    if kind == "typed":
        shutil.copytree(args.base, root / "base")
    (root / ".gitattributes").write_text(attributes(args.base, lfs, args.lfs_template))
    records = []
    git(root, ["init", "-q", "-b", "benchmark"], records, "init", env)
    for key, value in [("user.name", "ESKA experiment"), ("user.email", "experiment@local.invalid"),
                       ("core.autocrlf", "false"), ("core.eol", "lf"), ("gc.auto", "0"),
                       ("pack.threads", "1"), ("commit.gpgsign", "false")]:
        git(root, ["config", key, value], records, "configure", env)
    if lfs:
        git(root, ["lfs", "install", "--local"], records, "lfs_init", env)
    git(root, ["add", "."], records, "initial_add", env)
    git(root, ["commit", "-q", "-m", "chore: Исходный снимок"], records, "initial_commit", env)
    for _ in range(args.repetitions):
        output = git(root, ["status", "--porcelain=v2", "-z"], records, "status_clean", env)
        if output:
            raise ValueError("fixture must start clean")
    change(root, kind, args, owner)
    changed = git(root, ["diff", "--name-only", "-z"], records, "changed_paths", env)
    if len([path for path in changed.split(b"\0") if path]) != 2:
        raise ValueError("exactly two source files must change")
    for _ in range(args.repetitions):
        git(root, ["status", "--porcelain=v2", "-z"], records, "status_dirty", env)
        git(root, ["diff", "--no-ext-diff"], records, "diff_dirty", env)
    diff = git(root, ["diff", "--no-ext-diff"], records, "diff_evidence", env)
    (root.parent / (root.name + "-diff.txt")).write_bytes(diff)
    git(root, ["add", "src"], records, "save_add", env)
    git(root, ["commit", "-q", "-m", "chore: Изменены комментарии объекта и модуля"], records, "save_commit", env)
    git(root, ["gc", "--aggressive", "--prune=now"], records, "pack_two_commits", env)
    index = root / ".git/index"
    lfs_root = root / ".git/lfs/objects"
    pack_root = root / ".git/objects/pack"
    return {"kind": kind, "lfs": lfs, "working_source": storage(root / "src"),
            "base": storage(root / "base") if kind == "typed" else None,
            "index_bytes": index.stat().st_size, "objects": storage(root / ".git/objects"),
            "pack": storage(pack_root), "lfs_objects": storage(lfs_root) if lfs_root.exists() else None,
            "diff_bytes": len(diff), "history_commits": 2, "records": records}


def main():
    """Create only one new run directory below the sibling playground, with no remote writes."""
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ["original", "normalized", "source", "base", "edited-source", "lfs-template", "output"]:
        parser.add_argument("--" + name, type=Path, required=True)
    parser.add_argument("--property-file", required=True)
    parser.add_argument("--module", required=True)
    parser.add_argument("--repetitions", type=int, default=3)
    args = parser.parse_args()
    if args.repetitions < 1:
        parser.error("repetitions must be positive")
    playground = (Path(__file__).resolve().parents[4].parent / "eska-playground").resolve()
    for name in ["original", "normalized", "source", "base", "edited_source", "lfs_template"]:
        path = getattr(args, name).resolve(strict=True)
        if not path.is_relative_to(playground):
            parser.error("all inputs must be inside the sibling playground")
        setattr(args, name, path)
    args.output = args.output.parent.resolve(strict=True) / args.output.name
    if not args.output.is_relative_to(playground):
        parser.error("output must be inside the sibling playground")
    args.output.mkdir()
    manifest = json.loads((args.source / "source.json").read_text())
    owner = next(entry["role"]["source"] for entry in manifest["files"]
                 if entry["path"] == args.property_file)
    for value in [args.property_file, args.module, owner]:
        path = Path(value)
        if path.is_absolute() or ".." in path.parts:
            parser.error("edit paths must be relative and contained")
    env = {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}
    env.update({"GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": "/dev/null", "LC_ALL": "C"})
    report = {"schema_version": 1, "backend": "system-git", "repetitions": args.repetitions,
              "os_cache": "not flushed", "byte_policy": "-text for all source and base blobs",
              "git_version": subprocess.check_output(["git", "--version"], env=env, text=True).strip(),
              "cases": []}
    for lfs in [False, True]:
        for kind in ["original", "normalized", "typed"]:
            label = kind + ("-lfs" if lfs else "-plain")
            print(label, "started", flush=True)
            result = benchmark(args.output / label, kind, lfs, args, owner, env)
            report["cases"].append(result)
            (args.output / "measurement.json").write_text(json.dumps(report, ensure_ascii=False, indent=2))
            print(label, "finished", result["index_bytes"], result["pack"]["logical_bytes"], flush=True)
    print("system Git measurement complete", flush=True)


if __name__ == "__main__":
    main()
