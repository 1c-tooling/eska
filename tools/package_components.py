#!/usr/bin/env python3
"""Create selective local archives without changing release automation (Python 3.11+)."""

import hashlib
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
import zipfile

from components import Parser, option
from component_notices import metadata, notices
from component_package import (Failure, TOOLS, binary_name, contract, digest, json_bytes, native_target,
                               package, probe, require)


def archive_file(archive, name, payload, executable=False):
    """Stable timestamps and explicit regular-file modes make identical inputs reproduce one ZIP."""
    item = zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0))
    item.create_system = 3
    item.external_attr = (stat.S_IFREG | (0o755 if executable else 0o644)) << 16
    item.compress_type = zipfile.ZIP_DEFLATED
    archive.writestr(item, payload, compresslevel=6)


def build(root, binaries, output, selected):
    """Probe real executables, bundle target notices, and publish only a new output directory."""
    require(not output.exists() and not output.is_symlink(), "unmanaged", output)
    require(output.parent.is_dir(), "path", output.parent)
    resolved = metadata(root)
    with tempfile.TemporaryDirectory(prefix=".eska-packaging-", dir=output.parent) as temporary:
        stage = Path(temporary)
        releases = {}
        common_version = target = None
        for tool in selected:
            candidates = [binaries / tool, binaries / (tool + ".exe")]
            found = [path for path in candidates if path.is_file() and not path.is_symlink()]
            require(len(found) == 1, "source", tool)
            executable = found[0]
            value = contract(probe(executable, stage), tool)
            native_target(value["target"])
            common_version = common_version or value["version"]
            target = target or value["target"]
            require(value["version"] == common_version and value["target"] == target, "incompatible", tool)
            payloads = {"licenses/" + name: data for name, data in notices(root, tool, target, resolved).items()}
            name = "bin/" + binary_name(tool, target)
            original = digest(executable)
            payloads[name] = executable.read_bytes()
            require(hashlib.sha256(payloads[name]).hexdigest() == original and digest(executable) == original, "modified", executable)
            files = [{"path": key, "sha256": hashlib.sha256(data).hexdigest(), "size": len(data), "executable": key == name}
                     for key, data in sorted(payloads.items())]
            manifest = package({"schema_version": 1, "tool": tool, "version": common_version, "target": target,
                                "contract": value, "files": files})
            name = f"{tool}-{common_version}-{target}.zip"
            with zipfile.ZipFile(stage / name, "x") as archive:
                archive_file(archive, "package.json", json_bytes(manifest))
                for item in files:
                    archive_file(archive, item["path"], payloads[item["path"]], item["executable"])
            releases[tool] = {"archive": name, "sha256": digest(stage / name), "size": (stage / name).stat().st_size, "contract": value}
        index = {"schema_version": 1, "version": common_version, "target": target, "components": releases}
        (stage / "release.json").write_bytes(json_bytes(index))
        for name in ("components.py", "component_installation.py", "component_package.py"):
            shutil.copyfile(root / "tools" / name, stage / name)
        checksums = "".join(f"{digest(path)}  {path.name}\n" for path in sorted(stage.iterdir()) if path.is_file())
        (stage / "SHA256SUMS").write_text(checksums, encoding="utf-8", newline="\n")
        output.mkdir()
        published = []
        try:
            for path in sorted(stage.iterdir()):
                if path.is_file():
                    destination = output / path.name
                    os.link(path, destination)
                    published.append((destination, digest(path)))
        except BaseException:
            for path, checksum in reversed(published):
                if path.is_file() and not path.is_symlink() and digest(path) == checksum:
                    path.unlink()
            try:
                output.rmdir()
            except OSError:
                pass
            raise
    return index


def main():
    """Packaging is an explicit local developer action, independent of Actions or publication."""
    language = option(sys.argv[1:], "--lang", "en-US")
    help_text = {
        "en-US": "Usage: python3 package_components.py --bin-dir DIRECTORY --output NEW_DIRECTORY --component TOOL [--component TOOL ...] [--lang en-US|ru-RU]\nBuild binaries first with cargo build --workspace --bins --locked. No release workflow is changed.\n",
        "ru-RU": "Использование: python3 package_components.py --bin-dir КАТАЛОГ --output НОВЫЙ_КАТАЛОГ --component ИНСТРУМЕНТ [--component ИНСТРУМЕНТ ...] [--lang en-US|ru-RU]\nСначала соберите бинарники: cargo build --workspace --bins --locked. Release workflow не изменяется.\n",
    }
    try:
        parser = Parser(add_help=False)
        parser.add_argument("--bin-dir", type=Path)
        parser.add_argument("--output", type=Path)
        parser.add_argument("--component", action="append", choices=TOOLS)
        parser.add_argument("--lang", choices=tuple(help_text))
        parser.add_argument("--help", action="store_true")
        args = parser.parse_args()
        if args.help:
            print(help_text[language], end="")
            return 0
        require(args.bin_dir is not None and args.output is not None and args.component, "arguments")
        result = build(Path(__file__).resolve().parent.parent, args.bin_dir.resolve(), args.output.absolute(), list(dict.fromkeys(args.component)))
        print(json_bytes(result).decode("utf-8"), end="")
    except (Failure, OSError, ValueError, zipfile.BadZipFile, subprocess.SubprocessError) as error:
        print(json_bytes({"schema_version": 1, "status": "error", "code": getattr(error, "code", "io"),
                          "detail": getattr(error, "detail", str(error))}).decode("utf-8"), end="")
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
