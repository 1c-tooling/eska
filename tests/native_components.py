#!/usr/bin/env python3
"""Accept real local component archives in a new owned sibling playground stand."""

import argparse
import hashlib
import json
import os
import queue
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import threading


def digest(path):
    """Hash actual input bytes rather than trusting installer exit status."""
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def frames(values):
    """Encode independent byte-counted Metadata API frames."""
    output = bytearray()
    for value in values:
        body = json.dumps(value).encode()
        output.extend(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
    return bytes(output)


def metadata_session(binary, locale, stand, environment, evidence):
    """Wait for each protocol acknowledgement before sending shutdown and exit."""
    label = "project-serve-" + locale
    command = list(map(str, [binary, "--lang", locale, "serve", "--stdio"]))
    messages = queue.Queue(maxsize=16)
    with (stand / (label + ".stdout")).open("wb") as output, (stand / (label + ".stderr")).open("wb") as error:
        child = subprocess.Popen(command, cwd=stand, env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=error)
        def read():
            """Bound frame bytes and retain the exact stream while the client awaits each reply."""
            try:
                while header := child.stdout.readline():
                    assert header.startswith(b"Content-Length: ") and len(header) <= 8192
                    size = int(header.split(b":", 1)[1].strip())
                    separator = child.stdout.read(2)
                    assert 0 <= size <= 1024 * 1024 and separator == b"\r\n"
                    body = child.stdout.read(size)
                    assert len(body) == size
                    output.write(header + separator + body)
                    output.flush()
                    messages.put(json.loads(body), timeout=10)
            except BaseException as failure:
                messages.put(failure, timeout=10)
        reader = threading.Thread(target=read, daemon=True)
        reader.start()
        replies = {}
        try:
            requests = [
                {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"apiVersion": {"major": 1, "minor": 12}, "client": {"name": "package-matrix", "version": "1"}}},
                {"jsonrpc": "2.0", "id": 2, "method": "shutdown", "params": {}},
            ]
            for request in requests:
                child.stdin.write(frames([request]))
                child.stdin.flush()
                reply = messages.get(timeout=10)
                if isinstance(reply, BaseException):
                    raise reply
                assert reply.get("id") == request["id"] and "error" not in reply, reply
                replies[reply["id"]] = reply
            child.stdin.write(frames([{"jsonrpc": "2.0", "method": "exit"}]))
            child.stdin.close()
            assert child.wait(timeout=10) == 0
        finally:
            if child.poll() is None:
                child.kill()
            child.wait()
            reader.join(timeout=10)
            child.stdout.close()
            assert not reader.is_alive()
    assert not (stand / (label + ".stderr")).read_bytes()
    evidence.append({"label": label, "exit": child.returncode, "command": command})
    return replies


def matrix(repo, bundle, stand, analyzer):
    """Exercise installed owners and direct/wrapper results with no user PATH substitutions."""
    if analyzer is None or not analyzer.is_file():
        raise ValueError("complete package acceptance requires the real analyzer executable")
    release = json.loads((bundle / "release.json").read_bytes())
    selected = ("eska-project", "eska-check", "eska-build", "eska-vcs", "eska")
    assert set(release["components"]) == set(selected)
    checksums = {}
    for line in (bundle / "SHA256SUMS").read_text().splitlines():
        checksum, name = line.split("  ", 1)
        assert Path(name).name == name and len(checksum) == 64
        assert digest(bundle / name) == checksum
        checksums[name] = checksum
    frozen = stand / "bundle"
    frozen.mkdir()
    for name in (*checksums, "SHA256SUMS"):
        shutil.copy2(bundle / name, frozen / name)
    shutil.copy2(__file__, stand / "native_components.py")
    base = {key: value for key, value in os.environ.items() if not key.startswith("ESKA_") and key != "PYTHONPATH"}
    base.update({"PATH": "", "ESKA_TEST_ROOT": str(stand.parent), "ESKA_CONFIG_DIR": str(stand / "machine-config"),
                 "XDG_CACHE_HOME": str(stand / "xdg-cache"), "NO_COLOR": "1"})
    evidence = []

    def call(label, command, *, codes=(0,), data=None, env=None):
        """Retain raw streams and enforce a deadline for each actual process."""
        result = subprocess.run(list(map(str, command)), cwd=stand, env=env or base, input=data, capture_output=True, timeout=120)
        (stand / (label + ".stdout")).write_bytes(result.stdout)
        (stand / (label + ".stderr")).write_bytes(result.stderr)
        evidence.append({"label": label, "exit": result.returncode, "command": list(map(str, command))})
        if result.returncode not in codes:
            raise AssertionError(f"{label}: exit {result.returncode}; inspect saved streams in {stand}")
        return result

    installer = [sys.executable, "-B", frozen / "components.py"]
    profiles = {}
    for tool in selected:
        prefix = stand / "profiles" / tool
        command = installer + ["install", "--manifest", frozen / "release.json", "--component", tool, "--root", prefix, "--format", "json"]
        before = call(tool + "-preview", command + ["--check"])
        assert json.loads(before.stdout)["status"] == "install-available" and not prefix.exists()
        result = call(tool + "-install", command)
        assert json.loads(result.stdout)["status"] == "installed"
        suffix = ".exe" if "-windows-" in release["target"] else ""
        binary = prefix / "bin" / (tool + suffix)
        assert {path.name for path in binary.parent.iterdir()} == {binary.name}
        actual = call(tool + "-contract", [binary, "__tool-contract"])
        assert json.loads(actual.stdout) == release["components"][tool]["contract"] and not actual.stderr
        for locale in ("ru", "en"):
            help_result = call(tool + "-help-" + locale, [binary, "--lang", locale, "--help"])
            assert help_result.stdout and not help_result.stderr
        repeat = call(tool + "-offline-repeat", command)
        assert json.loads(repeat.stdout)["status"] == "up-to-date" and not list(prefix.glob(".eska-component-*"))
        status = call(tool + "-status", installer + ["status", "--component", tool, "--root", prefix, "--format", "json"])
        assert json.loads(status.stdout)["contract"] == release["components"][tool]["contract"]
        profiles[tool] = binary

    project = stand / "Проект XML"
    call("project-new", [profiles["eska-project"], "new", project, "--type", "configuration", "--source", "Исходники", "--format", "json"])
    source = project / "Исходники"
    (source / "Ext").mkdir(parents=True, exist_ok=True)
    (source / "Configuration.xml").write_bytes(('\ufeff<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses"><Configuration uuid="11111111-1111-1111-1111-111111111111"><Properties><Name>Приёмка</Name><Version>1.2.3.4</Version><CompatibilityMode>Version8_3_27</CompatibilityMode></Properties><ChildObjects/></Configuration></MetaDataObject>\r\n').encode())
    (source / "Ext/ObjectModule.bsl").write_bytes("\ufeffПроцедура Пример()\r\n    Значение = 1;\r\nКонецПроцедуры\r\n".encode())
    with (project / "eska.toml").open("a", encoding="utf-8") as stream:
        stream.write("\n[vcs.workflow]\npreset=\"trunk\"\n\n[tools.eska-check]\nmax_errors=1000\nmax_warnings=1000\n")
    original = {str(path.relative_to(project)): digest(path) for path in [project / "eska.toml", *source.rglob("*")] if path.is_file()}
    for locale in ("ru", "en"):
        call("project-validate-" + locale, [profiles["eska-project"], "--lang", locale, "--project-dir", project, "validate", "--format", "json"])
        replies = metadata_session(profiles["eska-project"], locale, stand, base, evidence)
        assert replies[1]["result"]["server"]["name"] == "eska-project" and 2 in replies
        assert replies[1]["result"]["apiVersion"] == release["components"]["eska-project"]["contract"]["metadata_api"]
        call("build-clean-" + locale, [profiles["eska-build"], "--lang", locale, "--project-dir", project, "clean"])

    missing = call("shell-missing-project", [profiles["eska"], "--project-dir", project, "version", "--format", "json"], codes=(1,),
                   env={**base, "ESKA_PROJECT_BINARY": str(stand / "missing-project")})
    assert not missing.stdout and missing.stderr
    full = stand / "full"
    for tool in selected:
        call("full-install-" + tool, installer + ["install", "--manifest", frozen / "release.json", "--component", tool, "--root", full, "--format", "json"])
    binaries = {tool: full / "bin" / profiles[tool].name for tool in selected}
    for locale in ("ru", "en"):
        direct = call("version-direct-" + locale, [binaries["eska-project"], "--lang", locale, "--project-dir", project, "version", "--format", "json"])
        wrapper = call("version-wrapper-" + locale, [binaries["eska"], "--lang", locale, "--project-dir", project, "version", "--format", "json"])
        assert (direct.stdout, direct.stderr) == (wrapper.stdout, wrapper.stderr)
    if analyzer is not None:
        installed_analyzer = stand / "external" / analyzer.name
        installed_analyzer.parent.mkdir()
        shutil.copy2(analyzer, installed_analyzer)
        call("analyzer-version", [installed_analyzer, "--version"])
        reports = []
        for locale in ("ru", "en"):
            for label, binary, commands in [("minimal", profiles["eska-check"], []), ("direct", binaries["eska-check"], []), ("wrapper", binaries["eska"], ["check"])]:
                result = call("check-" + label + "-" + locale, [binary, "--lang", locale, "--project-dir", project, *commands,
                    "--format", "json", "--analyzer", installed_analyzer])
                value = json.loads(result.stdout)
                assert value["complete"] is True and value["scope"] == "full" and len(value["files"]) == 1
                reports.append(result.stdout)
        assert all(value == reports[0] for value in reports)
        assert json.loads(reports[0])["counts"]["warnings"] >= 1
        config = project / "eska.toml"
        saved_config = config.read_bytes()
        threshold_reports = []
        try:
            config.write_bytes(saved_config.replace(b"max_warnings=1000", b"max_warnings=0"))
            for locale in ("ru", "en"):
                for label, binary, commands in [("direct", binaries["eska-check"], []), ("wrapper", binaries["eska"], ["check"])]:
                    result = call("threshold-" + label + "-" + locale, [binary, "--lang", locale, "--project-dir", project, *commands,
                        "--format", "json", "--analyzer", installed_analyzer], codes=(1,))
                    value = json.loads(result.stdout)
                    assert value["complete"] is True and value["status"] == "threshold_exceeded" and value["policy"]["max_warnings"] == 0
                    threshold_reports.append(result.stdout)
            assert all(value == threshold_reports[0] for value in threshold_reports)
        finally:
            config.write_bytes(saved_config)

    git = shutil.which("git")
    if git is None:
        raise ValueError("system Git is required only to prepare the tiny acceptance repository")
    (project / ".gitignore").write_text(".eska/\nbuild/\n", encoding="utf-8")
    git_env = {key: value for key, value in base.items() if not key.startswith("GIT_")}
    git_env.update({"GIT_CONFIG_NOSYSTEM": "1", "GIT_CONFIG_GLOBAL": str(stand / "missing-git-config"), "GIT_AUTHOR_NAME": "Eska Acceptance",
                    "GIT_AUTHOR_EMAIL": "acceptance@example.invalid", "GIT_COMMITTER_NAME": "Eska Acceptance", "GIT_COMMITTER_EMAIL": "acceptance@example.invalid"})
    for label, args in [("git-init", ["init", "--initial-branch=main"]), ("git-add", ["add", "."]), ("git-commit", ["-c", "commit.gpgsign=false", "-c", "core.hooksPath=", "commit", "-m", "acceptance fixture"])]:
        call(label, [git, "-C", project, *args], env=git_env)
    for locale in ("ru", "en"):
        values = []
        for label, binary in [("minimal", profiles["eska-vcs"]), ("direct", binaries["eska-vcs"]), ("wrapper", binaries["eska"])]:
            values.append(call("vcs-" + label + "-" + locale, [binary, "--lang", locale, "--project-dir", project, "status", "--format", "json"]).stdout)
        assert len(set(values)) == 1
    for name, checksum in original.items():
        assert digest(project / name) == checksum
    retained = digest(binaries["eska-vcs"])
    call("remove-project", installer + ["remove", "--component", "eska-project", "--root", full, "--format", "json"])
    assert not binaries["eska-project"].exists() and digest(binaries["eska-vcs"]) == retained
    for name, checksum in original.items():
        assert digest(project / name) == checksum
    return {"schema_version": 1, "status": "passed", "source_revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
            "source_tree_dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=repo)), "script_sha256": digest(Path(__file__)),
            "target": release["target"], "version": release["version"], "stand": str(stand), "bundle_sha256": checksums,
            "minimal_profiles": list(selected), "metadata_api": "1.12", "analyzer": str(analyzer) if analyzer else None,
            "check_reports_identical": analyzer is not None, "source_preserved": True, "selective_remove": "passed", "commands": evidence,
            "limitations": ["Linux native run only; Windows/macOS/Remote not verified", "1C build/unpack is a separate native matrix", "No release assets published"]}


def main():
    """Create a persistent evidence stand only below the derived mandatory playground."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", required=True, type=Path)
    parser.add_argument("--analyzer", required=True, type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    playground = (repo.parent / "eska-playground").resolve()
    if Path(os.environ["ESKA_TEST_ROOT"]).resolve() != playground:
        raise ValueError("ESKA_TEST_ROOT must be the sibling eska-playground")
    stand = Path(tempfile.mkdtemp(prefix="t97-installed-", dir=playground))
    print(stand, flush=True)
    try:
        result = matrix(repo, args.bundle.resolve(), stand, args.analyzer.resolve(strict=True))
        (stand / "report.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
        print("passed", flush=True)
    except BaseException as error:
        (stand / "failure.txt").write_text(str(error))
        raise


if __name__ == "__main__":
    main()
