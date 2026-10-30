#!/usr/bin/env python3
"""Assemble component releases and the complete cargo-dist installation from one Cargo version."""

import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tomllib
import zipfile

from component_package import TOOLS, binary_name, contract, digest, relative, require

ROOT = Path(__file__).resolve().parent.parent


def release_version():
    """Reject version drift before producing or publishing any release artifact."""
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))
    version = manifest["workspace"]["package"]["version"]
    require(re.fullmatch(r"\d+\.\d+\.\d+", version), "version", version)
    for path in [ROOT / "Cargo.toml", *sorted((ROOT / "crates").glob("*/Cargo.toml"))]:
        package = tomllib.loads(path.read_text(encoding="utf-8"))["package"]
        require(package["version"] == {"workspace": True}, "version", path)
        require(package["publish"] is True, "publish", path)
    for dependency in manifest["workspace"]["dependencies"].values():
        require(dependency["version"] == "=" + version, "version", dependency)
    return version


def verify_crates(directory):
    """Inspect normalized manifests because workspace packaging hides registry-only dev dependencies."""
    version = release_version()
    manifests = [ROOT / "Cargo.toml", *sorted((ROOT / "crates").glob("*/Cargo.toml"))]
    packages = {tomllib.loads(path.read_text(encoding="utf-8"))["package"]["name"] for path in manifests}
    for name in sorted(packages):
        archive = directory / f"{name}-{version}.crate"
        require(archive.is_file(), "missing-crate", archive)
        with tarfile.open(archive, "r:gz") as source:
            with source.extractfile(f"{name}-{version}/Cargo.toml") as manifest_file:
                manifest = tomllib.loads(manifest_file.read().decode("utf-8"))
        require(manifest["package"]["name"] == name and manifest["package"]["version"] == version,
                "version", archive)
        for table in [manifest, *manifest.get("target", {}).values()]:
            for alias, dependency in table.get("dev-dependencies", {}).items():
                package = dependency.get("package", alias) if isinstance(dependency, dict) else alias
                require(package not in packages, "registry-dev-dependency", f"{name} -> {package}")


def resume_config(version, output):
    """Permit an explicit retry of the current version without changing normal release-PR gating."""
    require(version == release_version(), "version", version)
    content = (ROOT / "release-plz.toml").read_text(encoding="utf-8")
    expected = tomllib.loads(content)
    require(expected["workspace"]["release_always"] is False, "release-config")
    content, count = re.subn(r"^release_always = false$", "release_always = true", content, flags=re.M)
    expected["workspace"]["release_always"] = True
    require(count == 1 and tomllib.loads(content) == expected, "release-config")
    output.write_text(content, encoding="utf-8")


def targets():
    """Keep the release matrix and artifact validation tied to the same distribution settings."""
    return tomllib.loads((ROOT / "dist-workspace.toml").read_text(encoding="utf-8"))["dist"]["targets"]


def prepare(binary_directory, components):
    """Generate ignored dist metadata so release automation remains the sole version owner."""
    version = release_version()
    directory = ROOT / "target/release-bundle"
    directory.mkdir(parents=True, exist_ok=True)
    fields = {
        "name": "eska", "version": version, "repository": "https://github.com/1c-tooling/eska",
        "description": "Инструменты разработки проектов 1С", "license": "Apache-2.0",
        "readme": str(ROOT / "README.md"), "changelog": str(ROOT / "CHANGELOG.md"),
        "license-files": [str(ROOT / "LICENSE")], "binaries": list(TOOLS),
        "build-command": [sys.executable, "-B", str(Path(__file__).resolve()), "copy-binaries", "--bin-dir", str(binary_directory.resolve())],
    }
    content = "[package]\n" + "".join(f"{key} = {json.dumps(value, ensure_ascii=False)}\n" for key, value in fields.items())
    content += "\n[dist]\ninclude = " + json.dumps([str(directory / "licenses")]) + "\n"
    (directory / "dist.toml").write_text(content, encoding="utf-8")
    (directory / "licenses").mkdir(exist_ok=True)
    if components:
        index = json.loads((components / "release.json").read_bytes())
        require(index["version"] == version and set(index["components"]) == set(TOOLS), "components")
        for tool, item in index["components"].items():
            archive = components / item["archive"]
            require(digest(archive) == item["sha256"], "checksum", archive)
            with zipfile.ZipFile(archive) as source:
                for name in source.namelist():
                    if name.startswith("licenses/"):
                        destination = directory.joinpath("licenses", tool, *relative(name))
                        destination.parent.mkdir(parents=True, exist_ok=True)
                        destination.write_bytes(source.read(name))


def copy_binaries(source):
    """The generic dist bundle uses the exact native binaries already checked and packaged."""
    target = os.environ["CARGO_DIST_TARGET"]
    for tool in TOOLS:
        path = source / binary_name(tool, target)
        value = json.loads(subprocess.check_output([path, "__tool-contract"], timeout=15))
        contract(value, tool, target)
        require(value["version"] == release_version(), "version", tool)
        shutil.copy2(path, Path.cwd() / path.name)


def copy_identical(source, destination):
    """Matrix jobs may share bootstrap files but must never silently replace differing bytes."""
    require(source.is_file() and not source.is_symlink(), "source", source)
    if destination.exists():
        require(digest(source) == digest(destination), "duplicate", destination.name)
    else:
        shutil.copy2(source, destination)


def collect(parts, output):
    """Require every target and component before merging native job outputs for publication."""
    version = release_version()
    indexes = list(parts.glob("*/release.json"))
    observed = set()
    output.mkdir(parents=True, exist_ok=True)
    for path in indexes:
        index = json.loads(path.read_bytes())
        target = index["target"]
        require(target in targets() and target not in observed, "target", target)
        observed.add(target)
        require(index["schema_version"] == 1 and index["version"] == version, "version", path)
        require(set(index["components"]) == set(TOOLS), "components", path)
        for tool, item in index["components"].items():
            contract(item["contract"], tool, target)
            require(item["contract"]["version"] == version, "version", tool)
            name = f"{tool}-{version}-{target}.zip"
            require(item["archive"] == name, "archive", name)
            archive = path.parent / name
            require(archive.stat().st_size == item["size"] and digest(archive) == item["sha256"], "checksum", name)
            copy_identical(archive, output / name)
        copy_identical(path, output / f"release-{target}.json")
        for name in ("LICENSE", "components.py", "component_installation.py", "component_package.py"):
            copy_identical(path.parent / name, output / name)
    require(observed == set(targets()), "missing-targets", sorted(set(targets()) - observed))
    for target in targets():
        extension = ".zip" if target.endswith("windows-msvc") else ".tar.xz"
        name = f"eska-{target}{extension}"
        matches = list(parts.glob("*/" + name))
        require(len(matches) == 1, "archive", name)
        copy_identical(matches[0], output / name)
        copy_identical(matches[0].with_name(name + ".sha256"), output / (name + ".sha256"))
        checksum = (output / (name + ".sha256")).read_text(encoding="utf-8").split()[0]
        require(checksum == digest(output / name), "checksum", name)
        manifest_name = f"{target}-dist-manifest.json"
        manifest = matches[0].parent / manifest_name
        value = json.loads(manifest.read_bytes())
        require(value["announcement_tag"] == "v" + version and name in value["artifacts"], "dist-manifest", target)
        require(value["artifacts"][name]["checksums"]["sha256"] == checksum, "dist-checksum", name)
        copy_identical(manifest, output / manifest_name)


def notes(version):
    """Use the translated changelog verbatim, including original commit scopes and messages."""
    changelog = (ROOT / "CHANGELOG.md").read_text(encoding="utf-8")
    match = re.search(r"^## \[" + re.escape(version) + r"\][^\n]*\n(.*?)(?=^## |\Z)", changelog, re.M | re.S)
    require(match is not None, "changelog", version)
    body = match[1].strip()
    return body + f"""

## Установка

Полный комплект содержит `eska`, `eska-project`, `eska-check`, `eska-build`, `eska-vcs` и `eska-fmt`.

Linux и macOS:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/odineska/eska/releases/download/v{version}/eska-installer.sh | sh
```

Windows PowerShell:

```powershell
irm https://github.com/odineska/eska/releases/download/v{version}/eska-installer.ps1 | iex
```

Из исходников через Cargo:

```sh
cargo install eska-project eska-check eska-build eska-vcs eska-fmt eska --version {version} --locked
```

## Отдельные компоненты

Архивы `<инструмент>-{version}-<целевая-платформа>.zip` содержат бинарник,
манифест и лицензии. Для выборочной установки предназначен `components.py`
(Python 3.11+); описание каждой платформы — `release-<целевая-платформа>.json`.
Контрольные суммы всех файлов находятся в `SHA256SUMS`.

Explorer устанавливает `eska-project` и внешний bsl-analyzer независимо.
Платформа 1С и bsl-analyzer в полный комплект ESKA не входят.
"""


def guard_installers(output):
    """Insert ownership preflights at pinned dist 0.32 anchors, before destination writes."""
    anchors = {
        "sh": "    # Identical to the sh version, just with a .fish file extension",
        "ps1": "  # The replace call here ensures proper escaping is inlined into the receipt",
    }
    for extension, anchor in anchors.items():
        path = output / ("eska-installer." + extension)
        content = path.read_text(encoding="utf-8")
        require(content.count(anchor) == 1 and "component-installation-conflict" not in content, "installer-template", path)
        guard = (ROOT / f"tools/release-install-guard.{extension}").read_text(encoding="utf-8")
        path.write_text(content.replace(anchor, guard + "\n" + anchor), encoding="utf-8", newline="\n")


def finalize(output, notes_path):
    """Refuse an incomplete release and produce checksums after all installers have been built."""
    version = release_version()
    required = ["eska-installer.sh", "eska-installer.ps1", "dist-manifest.json"]
    for target in targets():
        required.extend(f"{tool}-{version}-{target}.zip" for tool in TOOLS)
        required.append(f"release-{target}.json")
        required.append(f"eska-{target}" + (".zip" if target.endswith("windows-msvc") else ".tar.xz"))
    for name in required:
        require((output / name).is_file() and (output / name).stat().st_size > 0, "missing-artifact", name)
    guard_installers(output)
    description = notes(version)
    manifest_path = output / "dist-manifest.json"
    manifest = json.loads(manifest_path.read_bytes())
    for target in targets():
        name = f"eska-{target}" + (".zip" if target.endswith("windows-msvc") else ".tar.xz")
        require(manifest["artifacts"][name]["checksums"]["sha256"] == digest(output / name), "dist-checksum", name)
    manifest["announcement_github_body"] = description
    manifest["announcement_title"] = "ESKA v" + version
    for name in ("eska-installer.sh", "eska-installer.ps1"):
        if name in manifest["artifacts"]:
            manifest["artifacts"][name]["checksums"] = {"sha256": digest(output / name)}
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    for target in targets():
        (output / f"{target}-dist-manifest.json").unlink()
    checksums = "".join(f"{digest(path)}  {path.name}\n" for path in sorted(output.iterdir()) if path.is_file() and path.name != "SHA256SUMS")
    (output / "SHA256SUMS").write_text(checksums, encoding="utf-8", newline="\n")
    notes_path.write_text(description, encoding="utf-8", newline="\n")


def main():
    """Release helpers only prepare local files; GitHub publication belongs to the gated workflow."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("version", "verify-crates", "resume-config", "prepare",
                                            "copy-binaries", "collect", "finalize"))
    parser.add_argument("--version")
    parser.add_argument("--bin-dir", type=Path)
    parser.add_argument("--components", type=Path)
    parser.add_argument("--parts", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--notes", type=Path)
    args = parser.parse_args()
    if args.command == "version":
        print(release_version())
    elif args.command == "verify-crates":
        verify_crates(args.parts or ROOT / "target/package")
    elif args.command == "resume-config":
        require(args.version and args.output, "arguments", "--version, --output")
        resume_config(args.version, args.output)
    elif args.command == "prepare":
        prepare(args.bin_dir or ROOT / "target/release", args.components)
    elif args.command == "copy-binaries":
        copy_binaries(args.bin_dir)
    elif args.command == "collect":
        collect(args.parts, args.output)
    else:
        finalize(args.output, args.notes)


if __name__ == "__main__":
    main()
