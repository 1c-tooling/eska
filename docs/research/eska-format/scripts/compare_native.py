#!/usr/bin/env python3
"""Compare complete private native trees, with a narrowly verified dump-version exception."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import stat
import xml.etree.ElementTree as ET


def inventory(root):
    """Include every regular file and directory without following filesystem aliases."""
    files = {}
    directories = set()
    pending = [root]
    while pending:
        for path in sorted(pending.pop().iterdir()):
            mode = path.lstat().st_mode
            relative = path.relative_to(root).as_posix()
            if stat.S_ISDIR(mode):
                directories.add(relative)
                pending.append(path)
            elif stat.S_ISREG(mode):
                digest = hashlib.sha256()
                size = 0
                with path.open("rb") as stream:
                    for block in iter(lambda: stream.read(1024 * 1024), b""):
                        size += len(block)
                        digest.update(block)
                files[relative] = {"bytes": size, "sha256": digest.hexdigest()}
            else:
                raise ValueError(f"unsupported filesystem entry: {relative}")
    return files, directories


def dump_versions(left, right):
    """Mask only observed version attributes; every other byte must still be identical."""
    pattern = rb'(configVersion=")([0-9a-f]{40})(")'
    snapshots = []
    versions = []
    for path in (left, right):
        if path.stat().st_size > 64 * 1024 * 1024:
            return None
        content = path.read_bytes()
        if b"<!DOCTYPE" in content or b"<!ENTITY" in content:
            return None
        root = ET.fromstring(content)
        if root.tag != "{http://v8.1c.ru/8.3/xcf/dumpinfo}ConfigDumpInfo":
            return None
        if root.attrib != {"format": "Hierarchical", "version": "2.20"}:
            return None
        values = [item.attrib["configVersion"] for item in root.iter()
                  if "configVersion" in item.attrib]
        matched = re.findall(pattern, content)
        if [item[1].decode("ascii") for item in matched] != values:
            return None
        snapshots.append(re.sub(pattern, rb'\1<version>\3', content))
        versions.append(values)
    if snapshots[0] != snapshots[1]:
        return None
    return {"versioned_entries": len(versions[0]),
            "changed_versions": sum(a != b for a, b in zip(*versions)),
            "all_other_bytes_equal": True}


def main():
    """Write evidence only in the sibling playground and fail on unapproved differences."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("left", type=Path)
    parser.add_argument("right", type=Path)
    parser.add_argument("--allow-dump-versions", action="store_true")
    parser.add_argument("--expect-changed", action="append", default=[])
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    playground = (Path(__file__).resolve().parents[4].parent / "eska-playground").resolve()
    roots = [path.resolve(strict=True) for path in (args.left, args.right)]
    destination = args.output.parent.resolve(strict=True) / args.output.name
    if not all(path.is_relative_to(playground) for path in [*roots, destination]):
        parser.error("input and output must be inside the sibling eska-playground")
    left, left_dirs = inventory(roots[0])
    right, right_dirs = inventory(roots[1])
    changed = sorted(path for path in left.keys() & right.keys() if left[path] != right[path])
    exception = None
    if args.allow_dump_versions and "ConfigDumpInfo.xml" in changed:
        exception = dump_versions(roots[0] / "ConfigDumpInfo.xml", roots[1] / "ConfigDumpInfo.xml")
        if exception is not None:
            changed.remove("ConfigDumpInfo.xml")
    report = {"schema_version": 1, "left": str(roots[0]), "right": str(roots[1]),
              "left_files": len(left), "right_files": len(right),
              "left_directories": len(left_dirs), "right_directories": len(right_dirs),
              "missing": sorted(left.keys() - right.keys()), "extra": sorted(right.keys() - left.keys()),
              "missing_directories": sorted(left_dirs - right_dirs),
              "extra_directories": sorted(right_dirs - left_dirs),
              "changed": changed, "dump_versions": exception}
    valid = (not any(report[key] for key in ["missing", "extra", "missing_directories", "extra_directories"])
             and changed == sorted(set(args.expect_changed)))
    report["accepted"] = valid
    with destination.open("x", encoding="utf-8") as stream:
        json.dump(report, stream, ensure_ascii=False, indent=2)
        stream.write("\n")
    print(json.dumps(report, ensure_ascii=False, indent=2))
    return 0 if valid else 1


if __name__ == "__main__":
    raise SystemExit(main())
