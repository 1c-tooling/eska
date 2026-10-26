"""Strict component archive format shared by the local packager and installer (Python 3.11+)."""

from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import platform
import re
import secrets
import stat
import subprocess
import tempfile
import time
import urllib.parse
import urllib.request
import zipfile

TOOLS = ("eska", "eska-project", "eska-check", "eska-build", "eska-vcs")
TARGETS = ("aarch64-apple-darwin", "aarch64-unknown-linux-gnu", "aarch64-unknown-linux-musl",
           "x86_64-apple-darwin", "x86_64-unknown-linux-gnu", "x86_64-unknown-linux-musl",
           "x86_64-pc-windows-msvc")
MAX_ARCHIVE = 512 * 1024 * 1024
MAX_UNPACKED = 2 * 1024 * 1024 * 1024
MAX_MANIFEST = 4 * 1024 * 1024


class Failure(Exception):
    """Carry stable machine codes separately from localized CLI presentation."""
    def __init__(self, code, detail=""):
        """Retain a path or technical identifier without interpreting it as shell input."""
        self.code, self.detail = code, str(detail)
        super().__init__(code, detail)


def require(condition, code, detail=""):
    """Turn failed invariants into explicit installation failures."""
    if not condition:
        raise Failure(code, detail)


def json_bytes(value):
    """Canonical JSON bytes make manifests and receipts reproducible."""
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n").encode("utf-8")


def parse_json(data):
    """Duplicate object keys and non-finite numbers are never valid installation metadata."""
    def unique(pairs):
        """Keep all declared identities unambiguous before any filesystem operation."""
        result = {}
        for key, value in pairs:
            require(key not in result, "manifest", key)
            result[key] = value
        return result
    def invalid(value):
        """Reject JSON extensions accepted by Python's default decoder."""
        raise Failure("manifest", value)
    try:
        return json.loads(data, object_pairs_hook=unique, parse_constant=invalid)
    except (UnicodeError, ValueError) as error:
        raise Failure("manifest", error) from error


def keys(value, expected):
    """Validate exact schema fields instead of accepting partial success documents."""
    require(isinstance(value, dict) and set(value) == set(expected), "manifest")


def version(value):
    """Accept the same stable numeric release triples as the native tools."""
    require(isinstance(value, str) and re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", value), "version")
    parts = tuple(map(int, value.split(".")))
    require(all(part <= 2**64 - 1 for part in parts), "version")
    return parts


def contract(value, tool, target=None):
    """Mixed releases interoperate only when their explicit schemas remain supported."""
    keys(value, ("schema_version", "tool", "target", "version", "config_schema", "bridge_schema", "metadata_api"))
    require(type(value["schema_version"]) is int and value["schema_version"] == 2, "incompatible")
    require(tool in TOOLS and value["tool"] == tool and value["target"] in TARGETS, "incompatible", tool)
    require(target is None or value["target"] == target, "target", value["target"])
    version(value["version"])
    require(value["config_schema"] == "designer-xml-2" and type(value["bridge_schema"]) is int
            and value["bridge_schema"] == 1, "incompatible", tool)
    api = value["metadata_api"]
    if tool == "eska-project":
        keys(api, ("major", "minor"))
        require(type(api["major"]) is int and api["major"] == 1 and type(api["minor"]) is int
                and 12 <= api["minor"] <= 2**64 - 1, "incompatible", tool)
    else:
        require(api is None, "incompatible", tool)
    return value


def native_target(target):
    """Check OS and CPU before executing an archive; the native probe verifies remaining runtime needs."""
    require(target in TARGETS, "target", target)
    architecture = {"amd64": "x86_64", "x86_64": "x86_64", "arm64": "aarch64", "aarch64": "aarch64"}.get(platform.machine().lower())
    require(architecture is not None and target.startswith(architecture + "-"), "target", target)
    system = {"Linux": "-unknown-linux-", "Darwin": "-apple-darwin", "Windows": "-pc-windows-msvc"}.get(platform.system())
    require(system is not None and system in target, "target", target)


def digest(path):
    """Hash a regular owned file without following an unexpected leaf symlink."""
    info = path.lstat()
    require(stat.S_ISREG(info.st_mode), "modified", path)
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def hex_digest(value):
    """Checksums are canonical lowercase SHA-256 values."""
    require(isinstance(value, str) and re.fullmatch("[0-9a-f]{64}", value), "checksum")
    return value


def load(source, limit, expected=None):
    """Read a bounded local file or explicitly pinned HTTPS resource, without implicit network queries."""
    if expected is not None:
        hex_digest(expected)
    parsed = urllib.parse.urlsplit(str(source))
    if parsed.scheme in ("http", "https"):
        require(parsed.scheme == "https" and not parsed.username and not parsed.password, "source", source)
        request = urllib.request.Request(str(source), headers={"User-Agent": "eska-component-installer/1"})
        stream = urllib.request.urlopen(request, timeout=30)
    else:
        require(not parsed.scheme or (os.name == "nt" and len(parsed.scheme) == 1), "source", source)
        stream = Path(source).open("rb")
    with stream:
        data = stream.read(limit + 1)
    require(len(data) <= limit, "size", source)
    if expected is not None:
        require(hashlib.sha256(data).hexdigest() == expected, "checksum", source)
    return data


def relative(value):
    """Use one portable namespace with no absolute, traversal or Windows alias paths."""
    require(isinstance(value, str) and value and "\\" not in value and ":" not in value and "\x00" not in value, "path", value)
    parts = PurePosixPath(value).parts
    require(value == "/".join(parts) and all(part not in ("", ".", "..") and not part.endswith((".", " ")) for part in parts)
            and not value.startswith("/"), "path", value)
    require(all(not re.fullmatch(r"(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\..*)?", part) for part in parts), "path", value)
    return parts


def binary_name(tool, target):
    """The sole executable has a fixed name independent of archive-provided paths."""
    return tool + (".exe" if "-windows-" in target else "")


def package(value):
    """Validate every allowlisted payload and its hash before extraction."""
    keys(value, ("schema_version", "tool", "version", "target", "contract", "files"))
    require(type(value["schema_version"]) is int and value["schema_version"] == 1, "manifest")
    tool, target = value["tool"], value["target"]
    contract(value["contract"], tool, target)
    require(value["version"] == value["contract"]["version"], "version")
    files = value["files"]
    require(isinstance(files, list) and 2 <= len(files) <= 8192, "manifest")
    names, total = set(), 0
    executable = "bin/" + binary_name(tool, target)
    for item in files:
        keys(item, ("path", "sha256", "size", "executable"))
        path = item["path"]
        relative(path)
        require(path.casefold() not in {name.casefold() for name in names} and (path == executable or path.startswith("licenses/")), "path", path)
        require(type(item["size"]) is int and 0 <= item["size"] <= MAX_UNPACKED, "size", path)
        require(type(item["executable"]) is bool and item["executable"] == (path == executable), "manifest", path)
        hex_digest(item["sha256"])
        total += item["size"]
        names.add(path)
    require(executable in names and "licenses/LICENSE" in names and total <= MAX_UNPACKED, "manifest")
    return value


def unpack(data, destination):
    """Keep malformed ZIP failures in one stable category for every installer frontend."""
    try:
        return _unpack(data, destination)
    except (zipfile.BadZipFile, KeyError, RuntimeError, NotImplementedError) as error:
        raise Failure("archive", error) from error


def _unpack(data, destination):
    """Extract only regular declared members, checking exact archive cardinality and bytes."""
    import io
    with zipfile.ZipFile(io.BytesIO(data)) as archive:
        infos = archive.infolist()
        require(len(infos) <= 8193 and len({item.filename for item in infos}) == len(infos), "archive")
        require(all(not item.is_dir() and not item.flag_bits & 1 and item.compress_type in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED)
                    and (item.external_attr >> 16) & 0o170000 in (0, stat.S_IFREG) for item in infos), "archive")
        manifest = archive.getinfo("package.json")
        require(manifest.file_size <= MAX_MANIFEST, "size")
        result = package(parse_json(archive.read(manifest)))
        require({item.filename for item in infos} == {"package.json"} | {item["path"] for item in result["files"]}, "archive")
        for item in result["files"]:
            info = archive.getinfo(item["path"])
            require(info.file_size == item["size"], "size", item["path"])
            payload = archive.read(info)
            require(len(payload) == item["size"] and hashlib.sha256(payload).hexdigest() == item["sha256"], "checksum", item["path"])
            if destination is not None:
                output = destination / item["path"]
                output.parent.mkdir(parents=True, exist_ok=True)
                with output.open("xb") as stream:
                    stream.write(payload)
                output.chmod(0o755 if item["executable"] else 0o644)
    return result


def probe(executable, directory):
    """Bound both output streams and time, killing and reaping only the owned probe on any failure."""
    with tempfile.TemporaryFile(dir=directory) as stdout, tempfile.TemporaryFile(dir=directory) as stderr:
        child = subprocess.Popen([str(executable), "__tool-contract"], cwd=directory, stdin=subprocess.DEVNULL,
                                 stdout=stdout, stderr=stderr, env={**os.environ, "ESKA_CONFIG_DIR": str(directory / "probe-config")})
        try:
            deadline = time.monotonic() + 10
            while child.poll() is None:
                require(time.monotonic() < deadline, "probe-timeout")
                require(max(os.fstat(stdout.fileno()).st_size, os.fstat(stderr.fileno()).st_size) <= 65536, "probe-output")
                time.sleep(0.02)
            require(max(os.fstat(stdout.fileno()).st_size, os.fstat(stderr.fileno()).st_size) <= 65536, "probe-output")
            stdout.seek(0)
            stderr.seek(0)
            diagnostic = stderr.read().decode("utf-8", errors="replace")
            require(child.returncode == 0 and not diagnostic, "probe", json.dumps({
                "executable": str(executable), "returncode": child.returncode, "stderr": diagnostic,
            }, ensure_ascii=False))
            return parse_json(stdout.read())
        finally:
            if child.poll() is None:
                child.kill()
            child.wait()


@contextmanager
def installation_lock(root, recover_claim=False):
    """Coordinate native CLI writers and the Node-based Explorer through one exclusive claim."""
    root.mkdir(parents=True, exist_ok=True)
    path = root / ".eska-components.lock"
    require(not path.is_symlink(), "path", path)
    with path.open("a+b") as stream:
        try:
            if os.name == "nt":
                import msvcrt
                if path.stat().st_size == 0:
                    stream.write(b"\0")
                    stream.flush()
                stream.seek(0)
                msvcrt.locking(stream.fileno(), msvcrt.LK_NBLCK, 1)
            else:
                import fcntl
                fcntl.flock(stream.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as error:
            raise Failure("busy", root) from error
        claim = root / ".eska-components.claim"
        token = json_bytes({"schema_version": 1, "pid": os.getpid(), "nonce": secrets.token_hex(16)})
        claimed = False
        identity = None
        try:
            if recover_claim:
                recover_dead_claim(root)
            require(not os.path.lexists(root / ".eska-components.recovering"), "busy", root)
            try:
                with claim.open("xb") as owner:
                    claimed = True
                    info = os.fstat(owner.fileno())
                    identity = (info.st_dev, info.st_ino)
                    owner.write(token)
                    owner.flush()
                    os.fsync(owner.fileno())
            except FileExistsError as error:
                raise Failure("busy", claim) from error
            require(not os.path.lexists(root / ".eska-components.recovering"), "busy", root)
            yield
        finally:
            try:
                if claimed and not claim.is_symlink() and claim.exists():
                    info = claim.stat()
                    if (info.st_dev, info.st_ino) == identity:
                        claim.unlink()
            finally:
                if os.name == "nt":
                    stream.seek(0)
                    msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
                else:
                    fcntl.flock(stream.fileno(), fcntl.LOCK_UN)


def process_alive(pid):
    """Inspect process existence without sending termination signals, including on Windows."""
    if os.name != "nt":
        try:
            os.kill(pid, 0)
            return True
        except ProcessLookupError:
            return False
        except PermissionError:
            return True
    import ctypes
    from ctypes import wintypes
    kernel = ctypes.WinDLL("kernel32", use_last_error=True)
    kernel.OpenProcess.argtypes = (wintypes.DWORD, wintypes.BOOL, wintypes.DWORD)
    kernel.OpenProcess.restype = wintypes.HANDLE
    kernel.GetExitCodeProcess.argtypes = (wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD))
    kernel.CloseHandle.argtypes = (wintypes.HANDLE,)
    handle = kernel.OpenProcess(0x1000, False, pid)
    if not handle:
        return ctypes.get_last_error() != 87
    try:
        status = wintypes.DWORD()
        return not kernel.GetExitCodeProcess(handle, ctypes.byref(status)) or status.value == 259
    finally:
        kernel.CloseHandle(handle)


def recover_dead_claim(root):
    """Explicit recovery serializes claim removal; live or unidentifiable owners remain protected."""
    marker, claim = root / ".eska-components.recovering", root / ".eska-components.claim"
    token = json_bytes({"schema_version": 1, "pid": os.getpid(), "nonce": secrets.token_hex(16)})
    try:
        owner = marker.open("xb")
    except FileExistsError as error:
        raise Failure("busy", marker) from error
    info = os.fstat(owner.fileno())
    identity = (info.st_dev, info.st_ino)
    try:
        with owner:
            owner.write(token)
            owner.flush()
            os.fsync(owner.fileno())
        if not os.path.lexists(claim):
            return
        require(not claim.is_symlink() and claim.is_file() and claim.stat().st_size <= 1024, "busy", claim)
        value = parse_json(claim.read_bytes())
        keys(value, ("schema_version", "pid", "nonce"))
        require(value["schema_version"] == 1 and type(value["pid"]) is int and 0 < value["pid"] <= 2**31 - 1
                and isinstance(value["nonce"], str) and re.fullmatch("[0-9a-f]{32}", value["nonce"]), "busy", claim)
        require(not process_alive(value["pid"]), "busy", claim)
        claim.unlink()
    finally:
        if not marker.is_symlink() and marker.exists():
            info = marker.stat()
            if (info.st_dev, info.st_ino) == identity:
                marker.unlink()
