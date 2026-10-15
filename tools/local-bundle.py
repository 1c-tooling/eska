#!/usr/bin/env python3
"""Build and verify a local tool set without changing an installed channel or project."""

import argparse
import ctypes
import errno
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import queue
import re
import shutil
import signal
import stat
import subprocess
import sys
import tempfile
import threading
import time
import tomllib

REPOSITORY = Path(__file__).resolve().parents[1]
COMPONENTS = ("eska", "eska-project", "eska-build", "eska-check", "eska-analyzer")
RECEIPT = ".eska-local-bundle.json"
PROTOCOLS = {"eska": None, "eska-project": 1, "eska-build": 1, "eska-check": 1, "eska-analyzer": "0.1"}
PROBE_LIMIT = 64 * 1024
CONTROL_LIMIT = 1024 * 1024
BUILD_LIMIT = 32 * 1024 * 1024
SOURCE_EXCLUSIONS = {"target", ".git", ".cache", "cache", "__pycache__", "tests", "fixtures", "benches", "examples"}
CONTROL_KEYS = {"root_manifest_sha256", "analysis_manifest_sha256", "root_lock_sha256", "analysis_lock_sha256"}
LANGUAGES = {"ru": "ru-RU", "ru-RU": "ru-RU", "en": "en-US", "en-US": "en-US"}
HELP = {
    "ru-RU": """Локальный набор инструментов ESKA для разработки.

Использование:
  local-bundle.py [--lang ru-RU|en-US] build DESTINATION [OPTIONS]
  local-bundle.py [--lang ru-RU|en-US] verify BUNDLE [OPTIONS]

build собирает выбранные инструменты в новый каталог; verify проверяет существующий набор.
Без --components собираются eska, eska-project, eska-build, eska-check, eska-analyzer.
При --from PREVIOUS без --components сохраняется состав предыдущего набора.

Параметры build:
  --repository PATH       Корень исходников (по умолчанию — репозиторий этого скрипта).
  --components NAME ...   Явный состав нового набора.
  --from PREVIOUS         Проверенный предыдущий набор, задающий состав по умолчанию.
  --profile release|debug Профиль Cargo (по умолчанию release).
  --cargo PATH            Команда Cargo (по умолчанию cargo).
  --build-timeout SECONDS Предел каждой сборки: 0 < SECONDS <= 7200; по умолчанию 1800.

Общие параметры:
  --probe-timeout SECONDS Предел каждого probe: 0 < SECONDS <= 30; по умолчанию 5.
  --lang ru-RU|en-US      Язык help; затем ESKA_LANG, иначе ru-RU.
  -h, --help              Эта справка.

Утилита не устанавливает канал поставки и не меняет проекты, платформу, Git или PATH.
Destination должен отсутствовать. Атомарная публикация реализована для Linux/Windows.
Результаты и ошибки — JSON, независимый от языка. Cargo stderr передаётся без перевода.
""",
    "en-US": """Local ESKA tools bundle for development.

Usage:
  local-bundle.py [--lang ru-RU|en-US] build DESTINATION [OPTIONS]
  local-bundle.py [--lang ru-RU|en-US] verify BUNDLE [OPTIONS]

build compiles selected tools into a new directory; verify checks an existing bundle.
Without --components, builds eska, eska-project, eska-build, eska-check, eska-analyzer.
With --from PREVIOUS and no --components, preserves the previous bundle's components.

build options:
  --repository PATH       Source root (default: this script's repository).
  --components NAME ...   Explicit components for the new bundle.
  --from PREVIOUS         Verified previous bundle providing the default components.
  --profile release|debug Cargo profile (default: release).
  --cargo PATH            Cargo command (default: cargo).
  --build-timeout SECONDS Per-build limit: 0 < SECONDS <= 7200; default 1800.

Shared options:
  --probe-timeout SECONDS Per-probe limit: 0 < SECONDS <= 30; default 5.
  --lang ru-RU|en-US      Help language; then ESKA_LANG, otherwise ru-RU.
  -h, --help              Show this help.

The utility installs no distribution channel and changes no projects, platform, Git or PATH.
Destination must be absent. Atomic publication is implemented for Linux/Windows.
Results and errors use locale-independent JSON. Cargo stderr is passed through untranslated.
""",
}


class Failure(Exception):
    """Stable machine errors avoid OS-localized text in receipts and command output."""

    def __init__(self, code, **details):
        """Retain only explicit diagnostic coordinates, never child output or project bytes."""
        super().__init__(code)
        self.code = code
        self.details = details


class Parser(argparse.ArgumentParser):
    """Argparse validates typed options while explicit bilingual help and machine failures own presentation."""

    def __init__(self, *args, **kwargs):
        """Disable implicit English help and option abbreviation at every subcommand boundary."""
        super().__init__(*args, add_help=False, allow_abbrev=False, **kwargs)

    def error(self, message):
        """Discard argparse's human prose instead of leaking locale-dependent errors to either stream."""
        raise Failure("arguments_invalid")


def language_arguments(arguments):
    """Language may precede or follow a subcommand; only explicit supported identifiers override the default."""
    locale = LANGUAGES.get(os.environ.get("ESKA_LANG", ""), "ru-RU")
    remaining = []
    explicit = False
    index = 0
    while index < len(arguments):
        argument = arguments[index]
        if argument == "--lang" or argument.startswith("--lang="):
            if explicit:
                raise Failure("arguments_invalid")
            if argument == "--lang":
                index += 1
                if index == len(arguments):
                    raise Failure("arguments_invalid")
                locale = arguments[index]
            else:
                locale = argument.removeprefix("--lang=")
            if locale not in HELP:
                raise Failure("arguments_invalid")
            explicit = True
        else:
            remaining.append(argument)
        index += 1
    return locale, remaining


def unique_object(pairs):
    """Duplicate JSON fields cannot override a verified identity or file hash."""
    result = {}
    for key, value in pairs:
        if key in result:
            raise Failure("json_duplicate_key", field=key)
        result[key] = value
    return result


def decode_json(data):
    """Decode one bounded JSON object without replacement UTF-8 or permissive duplicate keys."""
    try:
        return json.loads(data, object_pairs_hook=unique_object)
    except (UnicodeError, ValueError) as error:
        raise Failure("json_invalid") from error


def regular_file(path):
    """Open the exact regular file, refusing symlinks before and at the filesystem boundary."""
    metadata = path.lstat()
    if not stat.S_ISREG(metadata.st_mode):
        raise Failure("file_not_regular", file=path.name)
    descriptor = os.open(path, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    opened = os.fstat(descriptor)
    if not stat.S_ISREG(opened.st_mode) or (metadata.st_dev, metadata.st_ino) != (opened.st_dev, opened.st_ino):
        os.close(descriptor)
        raise Failure("file_changed", file=path.name)
    return os.fdopen(descriptor, "rb")


def control_bytes(path):
    """Small manifests and receipts have a fixed read bound independent of their stated length."""
    with regular_file(path) as source:
        data = source.read(CONTROL_LIMIT + 1)
    if len(data) > CONTROL_LIMIT:
        raise Failure("control_oversized", file=path.name)
    return data


def file_hash(path):
    """Hash complete executable bytes with constant memory and no timestamp-based shortcuts."""
    digest = hashlib.sha256()
    size = 0
    with regular_file(path) as source:
        while block := source.read(1024 * 1024):
            digest.update(block)
            size += len(block)
    return digest.hexdigest(), size


def source_contract(repository):
    """Current package versions and lockfile hashes describe this checkout without Git or version edits."""
    paths = {"root_manifest": repository / "Cargo.toml", "analysis_manifest": repository / "tools/analysis-core/Cargo.toml",
             "root_lock": repository / "Cargo.lock", "analysis_lock": repository / "tools/analysis-core/Cargo.lock"}
    contents = {name: control_bytes(path) for name, path in paths.items()}
    try:
        root = tomllib.loads(contents["root_manifest"].decode())["package"]["version"]
        analysis = tomllib.loads(contents["analysis_manifest"].decode())["package"]["version"]
    except (UnicodeError, ValueError, KeyError, TypeError) as error:
        raise Failure("source_contract_invalid") from error
    if not all(isinstance(value, str) and re.fullmatch(r"\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?", value) for value in (root, analysis)):
        raise Failure("source_contract_invalid")
    return {"versions": {"eska": root, "eska-analysis": analysis, "source_core": root},
            "controls": {name + "_sha256": hashlib.sha256(data).hexdigest() for name, data in contents.items()}}


def source_files(directory):
    """Walk actual source inputs deterministically without following aliases into another tree."""
    for path in sorted(directory.iterdir()):
        if path.name in SOURCE_EXCLUSIONS:
            continue
        mode = path.lstat().st_mode
        if stat.S_ISDIR(mode):
            yield from source_files(path)
        elif stat.S_ISREG(mode):
            yield path
        else:
            raise Failure("source_input_not_regular")


def source_fingerprint(repository):
    """Bind build inputs across all linked local packages; timestamps and Git are not source identity."""
    paths = [repository / name for name in ("Cargo.toml", "Cargo.lock")]
    for name in ("build.rs", "rust-toolchain", "rust-toolchain.toml"):
        path = repository / name
        if os.path.lexists(path):
            paths.append(path)
    roots = []
    for name in ("src", "locales", "tools", ".cargo"):
        directory = repository / name
        if os.path.lexists(directory):
            if not stat.S_ISDIR(directory.lstat().st_mode):
                raise Failure("source_input_not_regular")
            roots.append(name)
            paths.extend(source_files(directory))
    digest = hashlib.sha256()
    digest.update(b"eska-local-build-source-v1\0")
    digest.update(json.dumps(roots, separators=(",", ":")).encode())
    for path in sorted(paths):
        hash_value, size = file_hash(path)
        entry = [path.relative_to(repository).as_posix(), hash_value, size, stat.S_IMODE(path.lstat().st_mode)]
        digest.update(json.dumps(entry, ensure_ascii=True, separators=(",", ":")).encode())
        digest.update(b"\n")
    return {"schema_version": 1, "sha256": digest.hexdigest(), "file_count": len(paths)}


def valid_hash(value):
    """Receipt digests use one canonical representation, never truncated or coerced values."""
    return isinstance(value, str) and re.fullmatch("[a-f0-9]{64}", value) is not None


def validate_source(value):
    """A receipt keeps package, linked-core and source-fingerprint identities independently inspectable."""
    if not isinstance(value, dict) or set(value) != {"versions", "controls", "fingerprint"}:
        raise Failure("receipt_invalid")
    versions = value["versions"]
    if not isinstance(versions, dict) or set(versions) != {"eska", "eska-analysis", "source_core"}:
        raise Failure("receipt_invalid")
    if not all(isinstance(item, str) and re.fullmatch(r"\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?", item)
               for item in versions.values()) or versions["source_core"] != versions["eska"]:
        raise Failure("receipt_invalid")
    controls = value["controls"]
    fingerprint = value["fingerprint"]
    if not isinstance(controls, dict) or set(controls) != CONTROL_KEYS or not all(map(valid_hash, controls.values())):
        raise Failure("receipt_invalid")
    if not isinstance(fingerprint, dict) or set(fingerprint) != {"schema_version", "sha256", "file_count"}:
        raise Failure("receipt_invalid")
    if type(fingerprint["schema_version"]) is not int or fingerprint["schema_version"] != 1:
        raise Failure("receipt_invalid")
    if not valid_hash(fingerprint["sha256"]) or type(fingerprint["file_count"]) is not int or fingerprint["file_count"] < 4:
        raise Failure("receipt_invalid")


def selected_components(explicit, previous=None):
    """An inherited selective set stays selective unless the caller explicitly replaces it."""
    values = list(explicit if explicit is not None else previous if previous is not None else COMPONENTS)
    if not values or not all(isinstance(value, str) for value in values) or len(values) != len(set(values)) or any(value not in COMPONENTS for value in values):
        raise Failure("components_invalid")
    return [value for value in COMPONENTS if value in values]


def recipes(repository, components, target, profile, cargo):
    """Separate existing Cargo recipes preserve minimal project/build feature boundaries."""
    groups = []
    if "eska" in components:
        groups.append((repository / "Cargo.toml", [], [], ["eska"], "root"))
    minimal = [name for name in ("eska-project", "eska-build") if name in components]
    if minimal:
        features = ["project-cli" if name == "eska-project" else "build-cli" for name in minimal]
        groups.append((repository / "Cargo.toml", ["--no-default-features"], features, minimal, "root"))
    analysis = [name for name in ("eska-check", "eska-analyzer") if name in components]
    if analysis:
        features = ["check-cli" if name == "eska-check" else "analyzer-cli" for name in analysis]
        groups.append((repository / "tools/analysis-core/Cargo.toml", ["--no-default-features"], features, analysis, "analysis"))
    result = []
    for manifest, defaults, features, names, directory in groups:
        destination = target / directory
        arguments = [str(cargo), "build", "--manifest-path", str(manifest), "--locked", "--profile",
                     "release" if profile == "release" else "dev", "--message-format=json-render-diagnostics",
                     "--target-dir", str(destination), *defaults]
        if features:
            arguments += ["--features", ",".join(features)]
        for name in names:
            arguments += ["--bin", name]
        result.append((arguments, names, destination))
    return result


def stop_process(process):
    """Interruptions kill and reap the owned child group, including Cargo's compiler children on Linux."""
    try:
        if os.name == "posix":
            os.killpg(process.pid, signal.SIGKILL)
        elif process.poll() is None:
            process.kill()
    except ProcessLookupError:
        pass
    process.wait(timeout=5)


def drain(stream, limit, name, results, oversized):
    """Each reader retains at most its fixed quota while stdout and stderr drain concurrently."""
    data = bytearray()
    try:
        while block := stream.read(min(8192, limit + 1 - len(data))):
            data.extend(block)
            if len(data) > limit:
                oversized.set()
                break
    finally:
        stream.close()
        results.put((name, bytes(data[:limit])))


def capture(arguments, cwd, timeout, limit, inherit_stderr=False):
    """Bound output, elapsed time and cleanup; a descendant retaining a pipe cannot block forever."""
    process = subprocess.Popen(arguments, cwd=cwd, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                               stderr=None if inherit_stderr else subprocess.PIPE,
                               start_new_session=os.name == "posix")
    results = queue.Queue(maxsize=2)
    oversized = threading.Event()
    readers = [threading.Thread(target=drain, args=(stream, limit, name, results, oversized), daemon=True)
               for name, stream in (("stdout", process.stdout), ("stderr", process.stderr)) if stream is not None]
    for reader in readers:
        reader.start()
    deadline = time.monotonic() + timeout
    output = {}
    complete = False
    try:
        while True:
            if oversized.is_set():
                raise Failure("process_output_oversized")
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise Failure("process_timeout")
            try:
                name, data = results.get(timeout=min(remaining, 0.05))
                output[name] = data
            except queue.Empty:
                pass
            status = process.poll()
            if status is not None and len(output) == len(readers):
                if oversized.is_set():
                    raise Failure("process_output_oversized")
                if status != 0:
                    raise Failure("process_failed", exit_code=status)
                complete = True
                return output["stdout"]
    finally:
        if not complete:
            stop_process(process)
        for reader in readers:
            reader.join(timeout=1)


def expected_version(component, versions):
    """Standalone analysis packages retain their real experimental version rather than borrowing the shell's."""
    return versions["eska-analysis" if component in ("eska-check", "eska-analyzer") else "eska"]


def has_strings(values, required):
    """Capability arrays contain complete string identifiers, never substring or object lookalikes."""
    return isinstance(values, list) and all(isinstance(value, str) for value in values) and required.issubset(values)


def probe(binary, component, versions, timeout):
    """Project-free read-only reports prove compatibility, not Ready state or complete semantics."""
    if component == "eska":
        output = capture([str(binary), "--version"], binary.parent, timeout, PROBE_LIMIT)
        try:
            version = output.decode("utf-8").strip()
        except UnicodeError as error:
            raise Failure("probe_invalid", component=component) from error
        if version != "eska " + versions["eska"]:
            raise Failure("probe_incompatible", component=component)
        return {"version": versions["eska"], "source_core_version": versions["source_core"],
                "protocol_version": None, "method": "version"}
    arguments = [str(binary), "--lang", "en"]
    arguments += ["--capabilities"] if component in ("eska-check", "eska-analyzer") else ["capabilities"]
    output = capture([*arguments, "--format", "json"], binary.parent, timeout, PROBE_LIMIT)
    value = decode_json(output)
    if not isinstance(value, dict) or type(value.get("schema_version")) is not int or value["schema_version"] != 1 or value.get("tool") != component or value.get("command") != "capabilities" or value.get("status") != "success":
        raise Failure("probe_invalid", component=component)
    data = value.get("data")
    if not isinstance(data, dict):
        raise Failure("probe_invalid", component=component)
    protocol = data.get("protocol_version")
    core = data.get("source_core_version") if component in ("eska-check", "eska-analyzer") else data.get("version")
    if data.get("version") != expected_version(component, versions) or core != versions["source_core"] or type(protocol) is not type(PROTOCOLS[component]) or protocol != PROTOCOLS[component]:
        raise Failure("probe_incompatible", component=component)
    if component in ("eska-check", "eska-analyzer"):
        if not has_strings(data.get("profiles"), {"syntax", "semantic"}) or not has_strings(data.get("source_formats"), {"eska"}):
            raise Failure("probe_incompatible", component=component)
        command = "check" if component == "eska-check" else "serve"
        if not has_strings(data.get("commands"), {command}):
            raise Failure("probe_incompatible", component=component)
        if component == "eska-check" and (type(data.get("report_schema_version")) is not int or data["report_schema_version"] != 1):
            raise Failure("probe_incompatible", component=component)
    else:
        commands = {"new", "init", "import", "prepare"} if component == "eska-project" else {"build", "unpack", "import"}
        if not has_strings(data.get("commands"), commands):
            raise Failure("probe_incompatible", component=component)
    return {"version": data["version"], "source_core_version": core, "protocol_version": protocol,
            "method": "capabilities", "data": data}


def binary_name(component, system=None):
    """Only the host's conventional executable suffix changes the declared file name."""
    return component + (".exe" if (system or platform.system()) == "Windows" else "")


def inspect_binary(root, component, versions, timeout):
    """Hash before and after a bounded probe so a self-changing executable cannot obtain a receipt."""
    path = root / "bin" / binary_name(component)
    before = file_hash(path)
    report = probe(path, component, versions, timeout)
    if file_hash(path) != before:
        raise Failure("file_changed", component=component)
    return {"component": component, "path": "bin/" + path.name, "sha256": before[0], "size": before[1], "probe": report}


def read_receipt(root):
    """An exact small receipt addresses only fixed executable names under a regular bundle directory."""
    if not stat.S_ISDIR(root.lstat().st_mode) or not stat.S_ISDIR((root / "bin").lstat().st_mode):
        raise Failure("bundle_not_directory")
    raw = control_bytes(root / RECEIPT)
    value = decode_json(raw)
    if not isinstance(value, dict) or type(value.get("schema_version")) is not int or value["schema_version"] != 1 or value.get("kind") != "eska-local-bundle":
        raise Failure("receipt_invalid")
    components = value.get("components")
    if not isinstance(components, list) or selected_components(components) != components:
        raise Failure("receipt_invalid")
    if value.get("profile") not in ("release", "debug"):
        raise Failure("receipt_invalid")
    previous = value.get("previous_receipt_sha256")
    if previous is not None and not valid_hash(previous):
        raise Failure("receipt_invalid")
    validate_source(value.get("source"))
    if value.get("platform") != {"system": platform.system(), "machine": platform.machine()}:
        raise Failure("bundle_platform_mismatch")
    files = value.get("files")
    if not isinstance(files, list) or len(files) != len(components):
        raise Failure("receipt_invalid")
    for component, entry in zip(components, files):
        if not isinstance(entry, dict) or entry.get("component") != component or entry.get("path") != "bin/" + binary_name(component) or not valid_hash(entry.get("sha256")) or type(entry.get("size")) is not int or entry["size"] < 0 or not isinstance(entry.get("probe"), dict):
            raise Failure("receipt_invalid")
    return value, hashlib.sha256(raw).hexdigest()


def verify(root, timeout=5):
    """Verification never opens a project, invokes a platform or writes the receipt or executable files."""
    root = Path(os.path.abspath(root))
    value, digest = read_receipt(root)
    for entry in value["files"]:
        path = root / entry["path"]
        if file_hash(path) != (entry["sha256"], entry["size"]):
            raise Failure("hash_mismatch", component=entry["component"])
        inspected = inspect_binary(root, entry["component"], value["source"]["versions"], timeout)
        if inspected != entry:
            raise Failure("probe_changed", component=entry["component"])
    if hashlib.sha256(control_bytes(root / RECEIPT)).hexdigest() != digest:
        raise Failure("receipt_changed")
    return value, digest


def publication_function():
    """Unsupported hosts refuse before compiling; POSIX rename has no overwrite-capable fallback."""
    if os.name == "nt":
        return None
    if sys.platform != "linux":
        raise Failure("atomic_publication_unsupported")
    library = ctypes.CDLL(None, use_errno=True)
    try:
        function = library.renameat2
    except AttributeError as error:
        raise Failure("atomic_publication_unsupported") from error
    function.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_int, ctypes.c_char_p, ctypes.c_uint]
    function.restype = ctypes.c_int
    return function


def publish(source, destination, function):
    """One no-replace rename publishes all verified bytes and preserves raced-in destinations."""
    if function is None:
        try:
            os.rename(source, destination)
        except FileExistsError as error:
            raise Failure("destination_exists") from error
        return
    descriptor = os.open(destination.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        relative = source.relative_to(destination.parent)
        if function(descriptor, os.fsencode(relative), descriptor, os.fsencode(destination.name), 1) != 0:
            code = ctypes.get_errno()
            if code in (errno.EEXIST, errno.ENOTEMPTY):
                raise Failure("destination_exists")
            if code in (errno.ENOSYS, errno.EINVAL, errno.EOPNOTSUPP):
                raise Failure("atomic_publication_unsupported")
            raise OSError(code, os.strerror(code))
    finally:
        os.close(descriptor)


def destination_path(path):
    """Canonicalize only the existing parent; the destination itself must not exist, including broken links."""
    if os.path.lexists(path):
        raise Failure("destination_exists")
    if path.name in ("", ".", ".."):
        raise Failure("destination_invalid")
    parent = path.parent.resolve(strict=True)
    if not parent.is_dir():
        raise Failure("destination_invalid")
    return parent / path.name


def copy_artifacts(output, names, target, bundle):
    """Cargo's executable messages must identify complete regular files inside this private build staging."""
    artifacts = {}
    for line in output.splitlines():
        value = decode_json(line)
        if isinstance(value, dict) and value.get("reason") == "compiler-artifact" and value.get("executable") is not None:
            target_info = value.get("target")
            if not isinstance(target_info, dict) or not isinstance(value["executable"], str):
                raise Failure("artifact_invalid")
            name = target_info.get("name")
            if name in names:
                path = Path(value["executable"])
                if not path.is_absolute() or not path.resolve(strict=True).is_relative_to(target.resolve(strict=True)) or path.is_symlink():
                    raise Failure("artifact_outside_staging", component=name)
                if name in artifacts:
                    raise Failure("artifact_duplicate", component=name)
                artifacts[name] = path
    if set(artifacts) != set(names):
        raise Failure("artifact_missing")
    for name, path in artifacts.items():
        with regular_file(path) as source, (bundle / "bin" / binary_name(name)).open("xb") as destination:
            mode = stat.S_IMODE(os.fstat(source.fileno()).st_mode)
            shutil.copyfileobj(source, destination)
        os.chmod(bundle / "bin" / binary_name(name), mode)


def build(repository, destination, explicit=None, previous=None, profile="release", cargo="cargo", build_timeout=1800, probe_timeout=5):
    """Compile selected tools privately; failed or interrupted work never claims a published destination."""
    destination = destination_path(destination)
    function = publication_function()
    old, previous_digest = verify(previous, probe_timeout) if previous is not None else (None, None)
    components = selected_components(explicit, old["components"] if old is not None else None)
    source = source_contract(repository)
    source["fingerprint"] = source_fingerprint(repository)
    with tempfile.TemporaryDirectory(prefix=".eska-local-bundle-", dir=destination.parent) as temporary:
        staging = Path(temporary)
        bundle = staging / "bundle"
        (bundle / "bin").mkdir(parents=True)
        for arguments, names, target in recipes(repository, components, staging / "cargo", profile, cargo):
            output = capture(arguments, repository, build_timeout, BUILD_LIMIT, inherit_stderr=True)
            copy_artifacts(output, names, target, bundle)
        entries = [inspect_binary(bundle, component, source["versions"], probe_timeout) for component in components]
        receipt = {"schema_version": 1, "kind": "eska-local-bundle", "components": components,
                   "platform": {"system": platform.system(), "machine": platform.machine()},
                   "profile": profile, "source": source, "files": entries,
                   "previous_receipt_sha256": previous_digest}
        raw = (json.dumps(receipt, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode()
        if len(raw) > CONTROL_LIMIT:
            raise Failure("control_oversized")
        (bundle / RECEIPT).write_bytes(raw)
        for entry in entries:
            if file_hash(bundle / entry["path"]) != (entry["sha256"], entry["size"]):
                raise Failure("file_changed", component=entry["component"])
        try:
            current = source_contract(repository)
            current["fingerprint"] = source_fingerprint(repository)
        except (OSError, Failure) as error:
            raise Failure("source_changed") from error
        if current != source:
            raise Failure("source_changed")
        publish(bundle, destination, function)
    return receipt, hashlib.sha256(raw).hexdigest()


def main():
    """The utility emits machine JSON and never modifies PATH, Cargo installs or an existing bundle."""
    parser = Parser()
    commands = parser.add_subparsers(dest="command", required=True)
    create = commands.add_parser("build")
    create.add_argument("destination", type=Path)
    create.add_argument("--repository", type=Path, default=REPOSITORY)
    create.add_argument("--components", nargs="+", choices=COMPONENTS)
    create.add_argument("--from", dest="previous", type=Path)
    create.add_argument("--profile", choices=("release", "debug"), default="release")
    create.add_argument("--cargo", default="cargo")
    create.add_argument("--build-timeout", type=float, default=1800)
    create.add_argument("--probe-timeout", type=float, default=5)
    check = commands.add_parser("verify")
    check.add_argument("bundle", type=Path)
    check.add_argument("--probe-timeout", type=float, default=5)
    try:
        locale, raw_arguments = language_arguments(sys.argv[1:])
        if "--help" in raw_arguments or "-h" in raw_arguments:
            print(HELP[locale], end="")
            return 0
        arguments = parser.parse_args(raw_arguments)
        if not math.isfinite(arguments.probe_timeout) or not 0 < arguments.probe_timeout <= 30:
            raise Failure("timeout_invalid")
        if arguments.command == "build":
            if not math.isfinite(arguments.build_timeout) or not 0 < arguments.build_timeout <= 7200:
                raise Failure("timeout_invalid")
            receipt, digest = build(arguments.repository.resolve(strict=True), arguments.destination,
                                    arguments.components, arguments.previous, arguments.profile,
                                    arguments.cargo, arguments.build_timeout, arguments.probe_timeout)
        else:
            receipt, digest = verify(arguments.bundle, arguments.probe_timeout)
        value = {"schema_version": 1, "status": "success", "command": arguments.command,
                 "components": receipt["components"], "receipt_sha256": digest}
        code = 0
    except Failure as error:
        value = {"schema_version": 1, "status": "error", "error": {"code": error.code, **error.details}}
        code = 1
    except KeyboardInterrupt:
        value = {"schema_version": 1, "status": "error", "error": {"code": "interrupted"}}
        code = 130
    except (OSError, subprocess.SubprocessError) as error:
        value = {"schema_version": 1, "status": "error", "error": {"code": "io", "errno": getattr(error, "errno", None)}}
        code = 1
    print(json.dumps(value, ensure_ascii=False, sort_keys=True))
    return code


if __name__ == "__main__":
    sys.exit(main())
