"""Collect immutable input evidence in a new, owned sibling-playground directory."""

import argparse
from collections import Counter
import hashlib
import json
import os
from pathlib import Path
import platform
import tempfile
import time
import xml.etree.ElementTree as ET

MD_NAMESPACE = "http://v8.1c.ru/8.3/MDClasses"


def local_name(tag):
    """Return an XML local name for inventory without rewriting its namespace."""
    return tag.rsplit("}", 1)[-1]


def inspect(alias, root, output):
    """Hash regular files and classify bounded XML descriptors without any source writes."""
    started = time.perf_counter()
    manifest = []
    categories = Counter()
    kinds = Counter()
    versions = Counter()
    issues = []
    aggregate = hashlib.sha256()
    for directory, dirs, names in os.walk(root, followlinks=False):
        dirs.sort()
        for name in dirs:
            path = Path(directory) / name
            if path.is_symlink():
                issues.append({"path": path.relative_to(root).as_posix(), "code": "symlink"})
        dirs[:] = [name for name in dirs if not (Path(directory) / name).is_symlink()]
        for name in sorted(names):
            path = Path(directory) / name
            relative = path.relative_to(root).as_posix()
            if path.is_symlink() or not path.is_file():
                issues.append({"path": relative, "code": "not_regular_file"})
                continue
            with path.open("rb") as stream:
                digest = hashlib.file_digest(stream, "sha256").hexdigest()
            stat = path.stat()
            category = path.suffix.lower() or "other"
            if path.suffix.lower() == ".xml" and stat.st_size <= 64 * 1024 * 1024:
                data = path.read_bytes()
                if b"<!DOCTYPE" in data or b"<!ENTITY" in data:
                    issues.append({"path": relative, "code": "xml_dtd"})
                else:
                    try:
                        document = ET.fromstring(data)
                        versions[document.get("version", "none")] += 1
                        category = local_name(document.tag)
                        if document.tag == f"{{{MD_NAMESPACE}}}MetaDataObject":
                            for element in document.iter():
                                if element.tag.startswith(f"{{{MD_NAMESPACE}}}") and "uuid" in element.attrib:
                                    kinds[local_name(element.tag)] += 1
                    except ET.ParseError:
                        issues.append({"path": relative, "code": "invalid_xml"})
            categories[category] += 1
            manifest.append({"path": relative, "bytes": stat.st_size, "sha256": digest})
            aggregate.update(relative.encode("utf-8") + b"\0" + bytes.fromhex(digest))
    summary = {
        "alias": alias,
        "source": str(root),
        "files": len(manifest),
        "bytes": sum(item["bytes"] for item in manifest),
        "corpus_sha256": aggregate.hexdigest(),
        "categories": dict(sorted(categories.items())),
        "metadata_kinds": dict(sorted(kinds.items())),
        "xml_versions": dict(sorted(versions.items())),
        "issues": issues,
        "seconds": time.perf_counter() - started,
    }
    (output / f"{alias}-files.json").write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n")
    return summary


def main():
    """Resolve explicitly selected corpora and exclusively claim an evidence directory."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", action="append", required=True, metavar="ALIAS=PATH")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[4]
    playground = (repo.parent / "eska-playground").resolve(strict=True)
    sources = []
    aliases = set()
    for value in args.input:
        alias, separator, raw_path = value.partition("=")
        if not separator or not alias.isascii() or not alias.isalnum() or alias in aliases:
            parser.error("inputs require unique ASCII alphanumeric aliases")
        source = Path(raw_path).resolve(strict=True)
        if not source.is_dir() or not source.is_relative_to(playground):
            parser.error("input must be a directory inside the sibling playground")
        aliases.add(alias)
        sources.append((alias, source))
    output = Path(tempfile.mkdtemp(prefix="eska-format-corpus-", dir=playground))
    evidence = {"inventory_version": 1, "python": platform.python_version(), "inputs": []}
    for alias, source in sources:
        summary = inspect(alias, source, output)
        evidence["inputs"].append(summary)
        (output / "inventory.json").write_text(json.dumps(evidence, ensure_ascii=False, indent=2) + "\n")
        print(json.dumps({"alias": alias, "files": summary["files"], "issues": len(summary["issues"])}), flush=True)
    print(output)


if __name__ == "__main__":
    main()
