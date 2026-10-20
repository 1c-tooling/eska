#!/usr/bin/env python3
"""Accept project/build composition against real artifacts from an owned native-build stand."""

import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from native_build import digest, interrupt_native, snapshot, source_identity


def run_matrix(args, repo, stand, fixtures):
    """Keep all writes in the new stand; the previous stand supplies read-only native inputs."""
    sources = {name: repo / "target/debug" / name for name in ["eska", "eska-project", "eska-build", "eska-vcs"]}
    if not all(binary.is_file() for binary in sources.values()):
        raise ValueError("build all workspace binaries first")
    (stand / "bin").mkdir()
    binaries = {}
    for name, source in sources.items():
        binaries[name] = stand / "bin" / name
        shutil.copy2(source, binaries[name])
    shutil.copy2(Path(__file__), stand / "native_onboarding.py")
    shutil.copy2(Path(__file__).with_name("native_build.py"), stand / "native_build.py")
    env = os.environ.copy()
    for key in ["ESKA_IBCMD", "ESKA_DISTROBOX", "ESKA_PLATFORM_ARCH", "ESKA_LANG"]:
        env.pop(key, None)
    env.update({"ESKA_CONFIG_DIR": str(stand / "machine-config"),
                "ESKA_PROJECT_BINARY": str(binaries["eska-project"]),
                "ESKA_BUILD_BINARY": str(binaries["eska-build"]),
                "ESKA_VCS_BINARY": str(binaries["eska-vcs"])})
    tools = ["--platform-version", args.platform_version, "--distrobox", args.distrobox]

    def call(label, command, cwd):
        """Retain raw streams and the exact failed stage instead of accepting partial success."""
        result = subprocess.run(command, cwd=cwd, env=env, capture_output=True, timeout=180)
        (stand / f"{label}.stdout").write_bytes(result.stdout)
        (stand / f"{label}.stderr").write_bytes(result.stderr)
        if result.returncode:
            raise RuntimeError(f"{label}: exit {result.returncode}; inspect saved streams")
        return result

    cases = []
    originals = snapshot(fixtures)
    for kind, extension in [("configuration", "cf"), ("extension", "cfe"), ("processing", "epf"), ("report", "erf")]:
        artifact = fixtures / kind / "build" / f"{kind}.{extension}"
        checksum = digest(artifact)
        expected_identity = source_identity(fixtures / kind / "src")
        manifests = []
        for entry, binary, new, init in [
            ("wrapper", binaries["eska"], "new", "init"),
            ("direct", binaries["eska-project"], "__shell-new", "__shell-init"),
        ]:
            parent = stand / entry
            parent.mkdir(exist_ok=True)
            destination = parent / kind
            prefix = [str(binary), "--lang", "en"]
            call(f"{kind}-{entry}-new", prefix + [new, kind, "--from", str(artifact), "--workflow", "trunk", "--no-vcs"] + tools, parent)
            manifest = (destination / "eska.toml").read_bytes()
            manifests.append(manifest)
            if source_identity(destination / "src") != expected_identity or (destination / ".git").exists():
                raise AssertionError("new published another identity or created unrequested Git metadata")
            imported = call(f"{kind}-{entry}-import", [str(binaries["eska-build"]), "--lang", "en", "import", str(artifact), "--force", "--format", "json"] + tools, destination)
            document = json.loads(imported.stdout)
            if not document["applied"] or document["incoming"]["type"] != kind or source_identity(destination / "src") != expected_identity:
                raise AssertionError("direct import did not preserve the native identity")
            if (destination / "eska.toml").read_bytes() != manifest:
                raise AssertionError("import changed the project manifest")
            if sorted(path.name for path in (destination / ".eska/import").iterdir()) != [".gitignore"]:
                raise AssertionError("owned transfer or native workspace remains")
            attached = parent / f"attached-{kind}"
            shutil.copytree(destination / "src", attached / "src")
            before = snapshot(attached / "src")
            call(f"{kind}-{entry}-init", prefix + [init, "--workflow", "trunk", "--no-vcs"] + tools, attached)
            if snapshot(attached / "src") != before or (attached / ".git").exists():
                raise AssertionError("init modified source bytes or created Git metadata")
            if digest(artifact) != checksum:
                raise AssertionError("an operation changed its input artifact")
            cases.append({"entry": entry, "project_type": kind, "new": "passed", "import": "passed", "init": "passed", "identity_matches": True, "input_unchanged": True, "init_source_unchanged": True, "staging_absent": True})
            print(f"{kind} {entry}: new/import/init passed", flush=True)
        if manifests[0] != manifests[1]:
            raise AssertionError("wrapper and direct onboarding produced different project manifests")
    if snapshot(fixtures) != originals:
        raise AssertionError("the read-only fixture stand changed")
    destination = stand / "cancelled-project"
    artifact = fixtures / "configuration/build/configuration.cf"
    checksum = digest(artifact)
    command = [str(binaries["eska"]), "--lang", "en", "new", str(destination), "--from", str(artifact), "--workflow", "trunk", "--no-vcs"] + tools
    code, _ = interrupt_native(command, destination / ".eska/import", ".eska-import-*/ibcmd.pid", stand, args, env, "cancel-new")
    if code != 1 or destination.exists() or digest(artifact) != checksum:
        raise AssertionError("native cancellation left a project or changed its input")
    return {"complete": True, "platform_version": args.platform_version, "runner": args.distrobox,
            "binary_sha256": {name: digest(binary) for name, binary in binaries.items()},
            "fixture_stand": str(fixtures.relative_to(repo.parent / "eska-playground")),
            "fixture_unchanged": True, "wrapper_direct_manifests_equal": True, "cases": cases,
            "cancellation": {"status": "passed", "exit": code, "native_pid_absent": True, "owned_project_absent": True, "input_unchanged": True}}


def main():
    """Create one isolated acceptance stand and never reuse a user's demo or native database."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", required=True)
    parser.add_argument("--platform-version", required=True)
    parser.add_argument("--distrobox", required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    playground = (repo.parent / "eska-playground").resolve()
    fixtures = Path(args.fixtures).resolve()
    owner = json.loads((fixtures / "owner.json").read_text())
    if fixtures.parent != playground or owner != {"kind": "eska-native-build-acceptance", "repository": str(repo)} or not json.loads((fixtures / "result.json").read_text())["complete"]:
        raise ValueError("fixtures must be a completed, owned native-build stand in sibling playground")
    if Path(os.environ.get("ESKA_TEST_ROOT", playground)).resolve() != playground:
        raise ValueError("ESKA_TEST_ROOT must identify the sibling playground")
    stand = Path(tempfile.mkdtemp(prefix="t96-onboarding-", dir=playground))
    (stand / "owner.json").write_text(json.dumps({"kind": "eska-native-onboarding-acceptance", "repository": str(repo)}))
    print(stand, flush=True)
    try:
        result = run_matrix(args, repo, stand, fixtures)
    except Exception as error:
        (stand / "result.json").write_text(json.dumps({"complete": False, "error": str(error)}, indent=2))
        raise
    (stand / "result.json").write_text(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
