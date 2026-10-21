"""Receipt-based component transactions; only selected, byte-verified managed files may change."""

import hashlib
import os
from pathlib import Path
import shutil
import tempfile

from component_package import (Failure, TOOLS, binary_name, contract, digest, installation_lock,
                               json_bytes, keys, native_target, package, parse_json, probe, relative, require, unpack, version)


def destination(root, name):
    """Reject a changed parent symlink before observing or publishing an owned leaf."""
    parts = relative(name)
    current = root
    for part in parts[:-1]:
        current /= part
        require(not current.is_symlink() and (not current.exists() or current.is_dir()), "path", current)
    result = root.joinpath(*parts)
    require(not result.is_symlink(), "path", result)
    return result


def installed_paths(manifest):
    """Executables share bin; notices belong to a private component namespace."""
    return {(item["path"] if item["executable"] else "share/eska/" + manifest["tool"] + "/" + item["path"][len("licenses/"):]): item
            for item in manifest["files"]}


def receipt(root, tool):
    """Accept only a complete component receipt, preserving unknown installation methods."""
    path = destination(root, f"components/{tool}.json")
    if not path.exists():
        return None
    require(path.is_file() and path.stat().st_size <= 4 * 1024 * 1024, "manifest", path)
    value = parse_json(path.read_bytes())
    keys(value, ("schema_version", "method", "archive_sha256", "package"))
    require(type(value["schema_version"]) is int and value["schema_version"] == 1
            and value["method"] == "component-archive", "installation", path)
    from component_package import hex_digest
    hex_digest(value["archive_sha256"])
    package(value["package"])
    require(value["package"]["tool"] == tool, "manifest", path)
    return value


def current_hash(path):
    """Absent and modified managed files are distinct transaction states."""
    return digest(path) if path.exists() or path.is_symlink() else None


def verify_owned(root, manifest):
    """Never replace or remove files whose current bytes differ from the receipt."""
    for name, item in installed_paths(manifest).items():
        path = destination(root, name)
        require(current_hash(path) == item["sha256"], "modified", path)


def pending(root):
    """An interrupted transaction needs explicit recovery before a new installation."""
    found = sorted(root.glob(".eska-component-*/journal.json"))
    require(not found, "recovery-required", found[0].parent if found else "")


def write_json(path, value):
    """Persist a same-directory replacement before publishing its new name."""
    temporary = path.with_name(path.name + ".next")
    with temporary.open("wb") as stream:
        stream.write(json_bytes(value))
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)


def allowed_path(tool, target, name):
    """Recovery journals have the same narrow write boundary as an ordinary install."""
    relative(name)
    return name in ("bin/" + binary_name(tool, target), f"components/{tool}.json") or name.startswith(f"share/eska/{tool}/")


def read_journal(stage):
    """Validate the whole journal before using it as a recovery instruction."""
    require(not stage.is_symlink() and stage.is_dir() and stage.name.startswith(".eska-component-"), "path", stage)
    path = stage / "journal.json"
    require(not path.is_symlink() and path.is_file() and path.stat().st_size <= 8 * 1024 * 1024, "manifest", path)
    value = parse_json(path.read_bytes())
    keys(value, ("schema_version", "tool", "target", "phase", "entries"))
    require(type(value["schema_version"]) is int and value["schema_version"] == 1 and value["tool"] in TOOLS
            and value["phase"] in ("prepared", "committed", "rolled-back"), "manifest", path)
    from component_package import TARGETS, hex_digest
    require(value["target"] in TARGETS and isinstance(value["entries"], list) and len(value["entries"]) <= 16385, "manifest")
    names = set()
    for index, entry in enumerate(value["entries"]):
        keys(entry, ("path", "old", "new", "backup"))
        require(allowed_path(value["tool"], value["target"], entry["path"]) and entry["path"] not in names, "path", entry["path"])
        names.add(entry["path"])
        for kind in ("old", "new"):
            if entry[kind] is not None:
                hex_digest(entry[kind])
        require(entry["old"] is not None or entry["new"] is not None, "manifest")
        require(entry["backup"] == (f"backup/{index}" if entry["old"] is not None else None), "manifest")
    return value


def restore(root, stage):
    """Restore exact prior files only if every live target still has a known transaction hash."""
    value = read_journal(stage)
    if value["phase"] in ("committed", "rolled-back"):
        return
    for entry in value["entries"]:
        path = destination(root, entry["path"])
        require(current_hash(path) in (entry["old"], entry["new"]), "rollback-conflict", path)
        if entry["old"] is not None:
            require(current_hash(destination(stage, entry["backup"])) == entry["old"], "rollback-conflict", stage)
    for index, entry in reversed(list(enumerate(value["entries"]))):
        path = destination(root, entry["path"])
        if entry["old"] is None:
            if path.exists():
                path.unlink()
        else:
            temporary = stage / f"restore-{index}"
            shutil.copy2(stage / entry["backup"], temporary)
            path.parent.mkdir(parents=True, exist_ok=True)
            os.replace(temporary, path)
    value["phase"] = "rolled-back"
    write_json(stage / "journal.json", value)


def publish(root, stage, tool, target, old_files, new_files):
    """Keep backups and a recoverable journal before the first managed file is replaced."""
    entries = []
    names = sorted(set(old_files) | set(new_files), key=lambda name: (name.startswith("components/"), name))
    (stage / "backup").mkdir()
    for index, name in enumerate(names):
        path = destination(root, name)
        old = old_files.get(name)
        require(current_hash(path) == old, "modified", path)
        backup = f"backup/{index}" if old is not None else None
        if backup is not None:
            shutil.copy2(path, stage / backup)
            require(digest(stage / backup) == old, "modified", path)
        entries.append({"path": name, "old": old, "new": digest(new_files[name]) if name in new_files else None, "backup": backup})
    journal = {"schema_version": 1, "tool": tool, "target": target, "phase": "prepared", "entries": entries}
    write_json(stage / "journal.json", journal)
    for entry in entries:
        path = destination(root, entry["path"])
        require(current_hash(path) == entry["old"], "modified", path)
        if entry["new"] is None:
            path.unlink()
            continue
        path.parent.mkdir(parents=True, exist_ok=True)
        incoming = new_files[entry["path"]]
        if entry["old"] is None:
            os.link(incoming, path)
        else:
            os.replace(incoming, path)


def finish(stage):
    """The commit marker makes cleanup after a crash distinguishable from rollback."""
    value = read_journal(stage)
    value["phase"] = "committed"
    write_json(stage / "journal.json", value)


def inspect(root, tool):
    """Read and verify one managed component without downloading or executing it."""
    pending(root)
    value = receipt(root, tool)
    if value is None:
        return {"schema_version": 1, "status": "not-installed", "tool": tool}
    verify_owned(root, value["package"])
    return {"schema_version": 1, "status": "installed", "tool": tool, "contract": value["package"]["contract"],
            "archive_sha256": value["archive_sha256"], "method": value["method"]}


def check_install(root, manifest, archive_sha256, force=False):
    """Preflight is read-only and checks every declared installed schema before replacement."""
    package(manifest)
    native_target(manifest["target"])
    pending(root)
    for name in TOOLS:
        installed = receipt(root, name)
        if installed is not None:
            contract(installed["package"]["contract"], name, manifest["target"])
    old = receipt(root, manifest["tool"])
    previous = old["package"] if old else None
    if previous is not None:
        verify_owned(root, previous)
        require(force or version(manifest["version"]) >= version(previous["version"]), "downgrade", manifest["tool"])
        if old["archive_sha256"] == archive_sha256:
            return old, "up-to-date"
        require(force or version(manifest["version"]) != version(previous["version"]), "same-version", manifest["tool"])
    old_names = installed_paths(previous) if previous else {}
    for name in installed_paths(manifest):
        if name not in old_names:
            path = destination(root, name)
            require(not path.exists() and not path.is_symlink(), "unmanaged", path)
    return old, "update-available" if old else "install-available"


def install(root, data, expected_hash, check=False, force=False):
    """Install exactly one selected package and restore its prior bytes after any recoverable failure."""
    require(hashlib.sha256(data).hexdigest() == expected_hash, "checksum")
    manifest = unpack(data, None)
    old, status = check_install(root, manifest, expected_hash, force)
    result = {"schema_version": 1, "status": status, "tool": manifest["tool"], "version": manifest["version"], "target": manifest["target"]}
    if check or status == "up-to-date":
        return result
    require(not root.is_symlink(), "path", root)
    with installation_lock(root):
        old, status = check_install(root, manifest, expected_hash, force)
        if status == "up-to-date":
            result["status"] = status
            return result
        required = sum(item["size"] for item in manifest["files"]) + (sum(item["size"] for item in old["package"]["files"]) if old else 0)
        require(shutil.disk_usage(root).free >= required + 16 * 1024 * 1024, "space", root)
        stage = Path(tempfile.mkdtemp(prefix=".eska-component-", dir=root))
        keep = False
        try:
            payload = stage / "payload"
            payload.mkdir()
            unpack(data, payload)
            executable = payload / "bin" / binary_name(manifest["tool"], manifest["target"])
            actual = probe(executable, stage)
            contract(actual, manifest["tool"], manifest["target"])
            require(actual == manifest["contract"], "incompatible", manifest["tool"])
            old, _ = check_install(root, manifest, expected_hash, force)
            new_receipt = {"schema_version": 1, "method": "component-archive", "archive_sha256": expected_hash, "package": manifest}
            (stage / "receipt.json").write_bytes(json_bytes(new_receipt))
            new_files = {name: payload / item["path"] for name, item in installed_paths(manifest).items()}
            name = f"components/{manifest['tool']}.json"
            new_files[name] = stage / "receipt.json"
            old_files = {name: item["sha256"] for name, item in installed_paths(old["package"]).items()} if old else {}
            if old:
                old_files[name] = digest(destination(root, name))
            publish(root, stage, manifest["tool"], manifest["target"], old_files, new_files)
            verify_owned(root, manifest)
            actual = probe(root / "bin" / binary_name(manifest["tool"], manifest["target"]), stage)
            require(actual == manifest["contract"], "incompatible", manifest["tool"])
            finish(stage)
            result["status"] = "updated" if old else "installed"
            return result
        except BaseException:
            if (stage / "journal.json").exists():
                try:
                    restore(root, stage)
                except BaseException as error:
                    keep = True
                    raise Failure("rollback", stage) from error
            raise
        finally:
            if not keep:
                shutil.rmtree(stage)


def remove(root, tool, check=False):
    """Remove only unchanged receipt-owned files, leaving all other tools and user content intact."""
    pending(root)
    old = receipt(root, tool)
    require(old is not None, "installation", tool)
    verify_owned(root, old["package"])
    result = {"schema_version": 1, "status": "remove-available", "tool": tool, "version": old["package"]["version"]}
    if check:
        return result
    with installation_lock(root):
        pending(root)
        current = receipt(root, tool)
        require(current == old, "modified", tool)
        verify_owned(root, old["package"])
        stage = Path(tempfile.mkdtemp(prefix=".eska-component-", dir=root))
        keep = False
        try:
            old_files = {name: item["sha256"] for name, item in installed_paths(old["package"]).items()}
            name = f"components/{tool}.json"
            old_files[name] = digest(destination(root, name))
            publish(root, stage, tool, old["package"]["target"], old_files, {})
            finish(stage)
            result["status"] = "removed"
            return result
        except BaseException:
            if (stage / "journal.json").exists():
                try:
                    restore(root, stage)
                except BaseException as error:
                    keep = True
                    raise Failure("rollback", stage) from error
            raise
        finally:
            if not keep:
                shutil.rmtree(stage)


def recover(root, stage):
    """Explicitly restore an interrupted owned transaction after validating all live hashes."""
    require(stage.parent == root and not root.is_symlink(), "path", stage)
    with installation_lock(root):
        value = read_journal(stage)
        restore(root, stage)
        shutil.rmtree(stage)
        return {"schema_version": 1, "status": "recovered", "tool": value["tool"]}
