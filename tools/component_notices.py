"""Collect notices for the resolved target graph, retaining vendored native notices and MPL sources."""

import hashlib
from pathlib import Path
import subprocess
import tomllib

from component_package import Failure, digest, json_bytes, parse_json, require


def cargo(root, *args):
    """Resolve only the already locked local Cargo graph, without changing releases or fetching crates."""
    result = subprocess.run(["cargo", *args, "--locked", "--offline"], cwd=root, check=True, capture_output=True)
    return result.stdout


def metadata(root):
    """Index exact package versions and registry checksums from the local lockfile."""
    packages = parse_json(cargo(root, "metadata", "--format-version", "1"))["packages"]
    checksums = {(item["name"], item["version"]): item.get("checksum")
                 for item in tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"]}
    return {(item["name"], item["version"]): item for item in packages}, checksums


def notices(root, tool, target, resolved):
    """Include an overapproximation of normal/build dependencies; never substitute an SPDX label for missing texts."""
    packages, checksums = resolved
    selected = set()
    for line in cargo(root, "tree", "--package", tool, "--target", target, "--edges", "normal,build",
                      "--prefix", "none", "--format", "{p}").decode("utf-8").splitlines():
        parts = line.split()
        require(len(parts) >= 2 and parts[1].startswith("v"), "license", line)
        selected.add((parts[0], parts[1][1:]))
    output = {"LICENSE": (root / "LICENSE").read_bytes()}
    entries = []
    for identity in sorted(selected):
        item = packages[identity]
        if item["source"] is None:
            continue
        crate = Path(item["manifest_path"]).parent
        files = [file for file in crate.rglob("*") if file.is_file() and
                 file.name.upper().startswith(("LICENSE", "LICENCE", "COPYING", "NOTICE", "COPYRIGHT"))]
        base = crate
        if not files:
            base = root / "tools" / "license-overrides" / f"{identity[0]}-{identity[1]}"
            files = sorted(base.rglob("*")) if base.is_dir() else []
            files = [file for file in files if file.is_file()]
            if files:
                provenance = parse_json((base / "provenance.json").read_bytes())
                recorded = parse_json((crate / ".cargo_vcs_info.json").read_bytes())
                require(provenance["upstream_commit"] == recorded["git"]["sha1"], "license", base)
                require(set(provenance["files"]) | {"provenance.json"} == {file.name for file in files}, "license", base)
                for filename, declaration in provenance["files"].items():
                    require(digest(base / filename) == declaration["sha256"] and
                            "/" + provenance["upstream_commit"] + "/" in declaration["source"], "license", filename)

        require(files, "license", f"{identity[0]} {identity[1]}")
        copied = []
        for file in sorted(files):
            require(not file.is_symlink() and file.resolve().is_relative_to(base.resolve()), "license", file)
            name = f"third-party/{identity[0]}-{identity[1]}/{file.relative_to(base).as_posix()}"
            output[name] = file.read_bytes()
            copied.append(name)
        entry = {"name": item["name"], "version": item["version"], "license": item["license"], "authors": item["authors"],
                 "repository": item["repository"], "source": item["source"], "registry_sha256": checksums[identity], "notices": copied}
        if "MPL-2.0" in (item["license"] or ""):
            registry = crate.parent.name
            archive = crate.parent.parent.parent / "cache" / registry / f"{identity[0]}-{identity[1]}.crate"
            require(archive.is_file() and digest(archive) == checksums[identity], "license", archive)
            name = f"sources/{identity[0]}-{identity[1]}.crate"
            output[name] = archive.read_bytes()
            entry["source_archive"] = name
        entries.append(entry)
    output["third-party.json"] = json_bytes({"schema_version": 1, "tool": tool, "target": target,
        "cargo_lock_sha256": hashlib.sha256((root / "Cargo.lock").read_bytes()).hexdigest(),
        "graph": "resolved normal and build dependencies; may include components not linked into the executable", "packages": entries})
    return output
