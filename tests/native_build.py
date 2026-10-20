#!/usr/bin/env python3
"""Run the real 1C build/export matrix only in a new sibling playground directory."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time
import uuid
import xml.etree.ElementTree as ET


def digest(path):
    """Hash actual bytes without depending on the native artifact's internal format."""
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def snapshot(source):
    """Record every source file, including opaque files and unchanged line endings."""
    return {str(path.relative_to(source)): digest(path) for path in source.rglob("*") if path.is_file()}


def source_identity(source):
    """Compare native export identity with the exact incoming Designer root descriptor."""
    for path in source.glob("*.xml"):
        document = ET.parse(path).getroot()
        for element in document:
            properties = next((node for node in element if node.tag.rsplit("}", 1)[-1] == "Properties"), None)
            if properties is None or "uuid" not in element.attrib:
                continue
            name = next((node.text for node in properties if node.tag.rsplit("}", 1)[-1] == "Name"), None)
            if name:
                return name, element.attrib["uuid"]
    raise AssertionError("native source identity is missing")


def external_descriptor(header, name, tag, class_id):
    """Create fresh identities; vendor build and re-export validate the external container format."""
    return (
        '\ufeff<?xml version="1.0" encoding="UTF-8"?>\r\n' + header + "\r\n"
        f'<{tag} uuid="{uuid.uuid4()}"><InternalInfo><xr:ContainedObject>'
        f"<xr:ClassId>{class_id}</xr:ClassId><xr:ObjectId>{uuid.uuid4()}</xr:ObjectId>"
        f'</xr:ContainedObject><xr:GeneratedType name="{tag}Object.{name}" category="Object">'
        f"<xr:TypeId>{uuid.uuid4()}</xr:TypeId><xr:ValueId>{uuid.uuid4()}</xr:ValueId>"
        f"</xr:GeneratedType></InternalInfo><Properties><Name>{name}</Name>"
        f"<Synonym/><Comment/></Properties><ChildObjects/></{tag}></MetaDataObject>\r\n"
    ).encode("utf-8")


def run_matrix(args, repo, stand):
    """Keep all native databases, exports, logs and reports under the owned stand."""
    binary = Path(args.binary).resolve() if args.binary else repo / "target/debug/eska-build"
    if not binary.is_file():
        raise ValueError("build eska-build first or select --binary")
    shell = Path(args.shell_binary).resolve() if args.shell_binary else None
    vcs = Path(args.vcs_binary).resolve() if args.vcs_binary else repo / "target/debug/eska-vcs"
    if shell and (not shell.is_file() or not vcs.is_file()):
        raise ValueError("build the selected shell and VCS binaries first")
    builder = shell or binary
    native = (["distrobox", "enter", "--name", args.distrobox, "--"] if args.distrobox else []) + [args.ibcmd]
    tools = ["--distrobox", args.distrobox] if args.distrobox else ["--ibcmd", args.ibcmd]
    env = os.environ.copy()
    for key in ["ESKA_IBCMD", "ESKA_DISTROBOX", "ESKA_PLATFORM_ARCH", "ESKA_LANG"]:
        env.pop(key, None)
    env["ESKA_CONFIG_DIR"] = str(stand / "machine-config")
    if shell:
        env["ESKA_BUILD_BINARY"] = str(binary)
        env["ESKA_VCS_BINARY"] = str(vcs)

    def call(label, command, cwd=stand):
        """Save raw evidence and stop on the actual exit code, preserving the failed stand."""
        result = subprocess.run(command, cwd=cwd, env=env, capture_output=True, timeout=120)
        (stand / f"{label}.stdout").write_bytes(result.stdout)
        (stand / f"{label}.stderr").write_bytes(result.stderr)
        if result.returncode:
            raise RuntimeError(f"{label}: exit {result.returncode}; inspect its saved stdout/stderr")
        return result

    version = call("platform-version", native + ["--version"]).stdout.decode().strip()
    if args.platform_version not in version:
        raise ValueError(f"selected native version {version!r} differs from expected {args.platform_version}")
    data = f"--data={stand / 'seed-infobase'}"
    call("create-seed", native + ["infobase", "create", data])
    call("export-seed", native + ["infobase", "config", "export", data, str(stand / "seed-xml")])
    call("create-extension", native + ["infobase", "config", "extension", "create", data,
         "--name=EskaTestExtension", "--name-prefix=EskaTest", "--purpose=customization"])
    call("export-extension", native + ["infobase", "config", "export", data,
         "--extension=EskaTestExtension", str(stand / "extension-xml")])
    header = (stand / "seed-xml/Configuration.xml").read_text(encoding="utf-8-sig").splitlines()[1]
    for name, source in [("configuration", "seed-xml"), ("extension", "extension-xml")]:
        root = stand / name
        root.mkdir()
        shutil.copytree(stand / source, root / "src")
    for name, tag, class_id in [
        ("processing", "ExternalDataProcessor", "c3831ec8-d8d5-4f93-8a22-f9bfae07327f"),
        ("report", "ExternalReport", "e41aff26-25cf-4bb6-b6c1-3f478a75f374"),
    ]:
        source = stand / name / "src"
        source.mkdir(parents=True)
        object_name = "EskaTest" + name
        (source / f"{object_name}.xml").write_bytes(external_descriptor(header, object_name, tag, class_id))
        (source / object_name / "Ext").mkdir(parents=True)
    exports = stand / "exports"
    exports.mkdir()
    results = []
    for name, extension in [("configuration", "cf"), ("extension", "cfe"), ("processing", "epf"), ("report", "erf")]:
        root = stand / name
        (root / "eska.toml").write_text(f"[project]\ntype='{name}'\n[build]\nplatform_version='{args.platform_version}'\n")
        before = snapshot(root / "src")
        command = [str(builder), "--lang", "en", "build", "--format", "json", "--manifest"] + tools
        built = call(name + "-build", command, root)
        artifact = root / "build" / f"{name}.{extension}"
        manifest = Path(str(artifact) + ".manifest.json")
        first = json.loads(manifest.read_text())
        first_checksum = digest(artifact)
        direct = [str(binary)] + (["--vcs-binary", str(vcs)] if shell else []) + command[1:]
        repeated = call(name + "-repeat", direct, root)
        first_result, repeated_result = json.loads(built.stdout), json.loads(repeated.stdout)
        first_result.pop("duration_ms")
        repeated_result.pop("duration_ms")
        if first_result != repeated_result:
            raise AssertionError("wrapper and direct build returned different JSON contracts")
        if b"Creating infobase" in repeated.stderr:
            raise AssertionError("repeat recreated a compatible managed infobase")
        document = json.loads(manifest.read_text())
        if document["source"]["snapshot_id"] != first["source"]["snapshot_id"]:
            raise AssertionError("unchanged inputs produced a different source snapshot")
        expected_git = "absent" if shell else "unavailable"
        if document["source"]["git"]["status"] != expected_git:
            raise AssertionError("build did not retain the selected VCS provenance policy")
        for field in ["schema_version", "kind", "source", "project", "platform"]:
            if first[field] != document[field]:
                raise AssertionError("wrapper and direct build returned different manifest metadata")
        checksum = digest(artifact)
        if document["artifact"]["checksum"]["value"] != checksum:
            raise AssertionError("manifest checksum does not match the native artifact")
        exported = call(name + "-unpack", [str(binary), "--lang", "en", "unpack", str(artifact),
            str(exports / name), "--format", "json", "--platform-version", args.platform_version] + tools)
        identity = json.loads(exported.stdout)["identity"]
        original_identity = source_identity(root / "src")
        if (identity["project_type"] != name or digest(artifact) != checksum
                or (identity["name"], identity["uuid"]) != original_identity):
            raise AssertionError("export changed the input or reported another project type")
        if list((exports / ".eska/import").glob(".eska-import-*")):
            raise AssertionError("an owned native export staging directory remains")
        call(name + "-clean", [str(builder), "--lang", "en", "clean"], root)
        if digest(artifact) != checksum or not manifest.is_file() or snapshot(root / "src") != before:
            raise AssertionError("build/clean changed source bytes or removed a published result")
        if list((root / "build").glob("build.eska-work-*")):
            raise AssertionError("an owned temporary build workspace remains")
        results.append({"project_type": name, "build": "passed", "repeat": "reused_infobase",
            "unpack": "passed", "clean": "passed", "artifact_sha256": checksum,
            "artifact_bytes": artifact.stat().st_size, "identity": identity, "source_unchanged": True,
            "export_identity_matches_source": True, "git_status": expected_git,
            "direct_wrapper_json_equal_except_duration": True if shell else None,
            "direct_wrapper_manifest_metadata_equal": True if shell else None,
            "first_artifact_sha256": first_checksum,
            "repeated_artifact_bytes_equal": first_checksum == checksum})
        print(name + ": passed", flush=True)
    cancellation = {"unpack": cancel_export(binary, stand, args, tools, env),
                    "build": cancel_build(builder, stand, args, tools, env, call)}
    return {"complete": True, "entry": "eska" if shell else "eska-build", "platform_version": version, "runner": "distrobox" if args.distrobox else "host",
            "cases": results, "cancellation": cancellation}


def interrupt_native(command, pid_root, pattern, stand, args, env, label):
    """Interrupt only a PID published by this owned run and save its actual process result."""
    child = subprocess.Popen(command, cwd=stand, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    native_pid = None
    try:
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline and child.poll() is None:
            for path in pid_root.glob(pattern):
                value = path.read_text().strip()
                if value.isdecimal():
                    native_pid = value
                    break
            if native_pid:
                break
            time.sleep(0.01)
        if not native_pid:
            raise RuntimeError("owned native PID was not published before the process ended")
        child.send_signal(signal.SIGINT)
        stdout, stderr = child.communicate(timeout=30)
        (stand / f"{label}.stdout").write_bytes(stdout)
        (stand / f"{label}.stderr").write_bytes(stderr)
        probe = subprocess.run(["distrobox", "enter", "--name", args.distrobox, "--", "sh", "-c",
            'test ! -d /proc/"$1"', "sh", native_pid], cwd=stand, capture_output=True, timeout=30)
        if probe.returncode:
            raise AssertionError("an owned native process survived cancellation")
        return child.returncode, stdout
    finally:
        if child.poll() is None:
            if native_pid:
                subprocess.run(["distrobox", "enter", "--name", args.distrobox, "--", "sh", "-c",
                    'if [ -r /proc/"$1"/cmdline ] && grep -Fq -- "$2" /proc/"$1"/cmdline; then kill -KILL "$1"; fi',
                    "sh", native_pid, str(stand)], cwd=stand, capture_output=True, timeout=30)
            child.kill()
            child.communicate(timeout=30)


def cancel_export(binary, stand, args, tools, env):
    """A cancelled real export must leave no new destination and preserve its binary input."""
    artifact = stand / "configuration/build/configuration.cf"
    checksum = digest(artifact)
    destination = stand / "cancelled-export"
    command = [str(binary), "--lang", "en", "unpack", str(artifact), str(destination),
               "--format", "json", "--platform-version", args.platform_version] + tools
    code, _ = interrupt_native(command, stand / ".eska/import", ".eska-import-*/ibcmd.pid", stand, args, env, "cancel-unpack")
    if code != 130 or destination.exists() or digest(artifact) != checksum:
        raise AssertionError("cancellation did not preserve the output boundary and input artifact")
    if list((stand / ".eska/import").glob(".eska-import-*")):
        raise AssertionError("an owned export staging directory survived cancellation")
    return {"status": "passed", "exit": code, "native_pid_absent": True, "staging_absent": True, "input_unchanged": True}


def cancel_build(binary, stand, args, tools, env, call):
    """Cancel native import into a reused infobase, preserving the previous artifact and manifest."""
    root = stand / "configuration"
    command = [str(binary), "--lang", "en", "build", "--project-dir", str(root), "--format", "json", "--manifest"] + tools
    call("prepare-cancel-build", command)
    artifact = root / "build/configuration.cf"
    manifest = Path(str(artifact) + ".manifest.json")
    old_artifact, old_manifest, old_source = digest(artifact), digest(manifest), snapshot(root / "src")
    owners = list((root / "build/.eska/infobases").glob("*/owner"))
    if len(owners) != 1:
        raise AssertionError("expected exactly one owned managed infobase")
    code, stdout = interrupt_native(command, root / "build", "build.eska-work-*/ibcmd.pid", stand, args, env, "cancel-build")
    document = json.loads(stdout)
    if code != 1 or document["error"]["code"] != "interrupted" or document["error"]["stage"] != "import-sources":
        raise AssertionError("native import did not report the established interrupted-build contract")
    if digest(artifact) != old_artifact or digest(manifest) != old_manifest or snapshot(root / "src") != old_source:
        raise AssertionError("interrupted import changed the previous result or source bytes")
    if not owners[0].is_file() or not (owners[0].parent / "data").is_dir():
        raise AssertionError("interrupted import removed the managed infobase")
    if list((root / "build").glob("build.eska-work-*")):
        raise AssertionError("interrupted import left an owned temporary build workspace")
    call("clean-after-cancel-build", [str(binary), "--lang", "en", "clean", "--project-dir", str(root)])
    return {"status": "passed", "exit": code, "stage": "import-sources", "native_pid_absent": True,
            "previous_result_unchanged": True, "source_unchanged": True, "managed_infobase_preserved": True, "staging_absent": True}


def main():
    """Require an explicit platform and preserve each exclusively owned acceptance run for review."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary")
    parser.add_argument("--shell-binary")
    parser.add_argument("--vcs-binary")
    parser.add_argument("--ibcmd", required=True)
    parser.add_argument("--platform-version", required=True)
    parser.add_argument("--distrobox")
    args = parser.parse_args()
    if os.name != "posix" or not args.distrobox:
        parser.error("the complete native matrix currently requires a POSIX Distrobox runner")
    repo = Path(__file__).resolve().parent.parent
    playground = (repo.parent / "eska-playground").resolve()
    if Path(os.environ.get("ESKA_TEST_ROOT", playground)).resolve() != playground:
        raise ValueError("ESKA_TEST_ROOT must identify the sibling eska-playground")
    stand = Path(tempfile.mkdtemp(prefix="t96-native-" if args.shell_binary else "t94-native-", dir=playground))
    print(str(stand), flush=True)
    (stand / "owner.json").write_text(json.dumps({"kind": "eska-native-build-acceptance", "repository": str(repo)}))
    try:
        result = run_matrix(args, repo, stand)
    except Exception as error:
        (stand / "result.json").write_text(json.dumps({"complete": False, "error": str(error)}, indent=2))
        raise
    (stand / "result.json").write_text(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
