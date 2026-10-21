"""Component installation conformance uses only unique owned playground directories."""

from contextlib import contextmanager
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock
import zipfile

REPOSITORY = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPOSITORY / "tools"))
import component_installation as installation
import component_package as package
import components
from package_components import archive_file

TARGET = ("aarch64" if platform.machine().lower() in ("arm64", "aarch64") else "x86_64") + "-unknown-linux-gnu"


def capabilities(tool="eska-project", release="1.0.0"):
    """Process fixtures declare the same public installation schemas as actual binaries."""
    return {"schema_version": 2, "tool": tool, "version": release, "target": TARGET,
            "config_schema": "designer-xml-1", "bridge_schema": 1,
            "metadata_api": {"major": 1, "minor": 12} if tool == "eska-project" else None}


def fixture(tool="eska-project", release="1.0.0", after_publish=False, probe_code="", notice=b"notice"):
    """Package an executable process fixture; native product acceptance is performed separately."""
    value = capabilities(tool, release)
    program = f"#!{sys.executable}\nimport json,os,sys,time\nfrom pathlib import Path\nvalue=json.loads({json.dumps(value)!r})\n"
    if after_publish:
        program += "if '.eska-component-' not in str(Path(sys.argv[0])): value['version']='99.0.0'\n"
    program += probe_code + "\nprint(json.dumps(value))\n"
    payloads = {f"bin/{tool}": program.encode(), "licenses/LICENSE": b"project license", "licenses/dependency.txt": notice}
    manifest = {"schema_version": 1, "tool": tool, "version": release, "target": TARGET, "contract": value,
                "files": [{"path": name, "sha256": hashlib.sha256(data).hexdigest(), "size": len(data), "executable": name.startswith("bin/")}
                          for name, data in sorted(payloads.items())]}
    return zipped(manifest, payloads)


def zipped(manifest, payloads, extra=None):
    """Recompute an archive checksum independently of the installer implementation."""
    output = io.BytesIO()
    with zipfile.ZipFile(output, "w") as archive:
        archive_file(archive, "package.json", package.json_bytes(manifest))
        for name, data in payloads.items():
            archive_file(archive, name, data, name.startswith("bin/"))
        if extra:
            archive_file(archive, *extra)
    data = output.getvalue()
    return data, hashlib.sha256(data).hexdigest()


def snapshot(root):
    """Record all managed and user file bytes, ignoring only the persistent advisory lock file."""
    return {str(path.relative_to(root)): (path.read_bytes(), path.stat().st_mode & 0o777)
            for path in root.rglob("*") if path.is_file() and path.name != ".eska-components.lock"}


@unittest.skipUnless(sys.platform.startswith("linux"), "Executable fixtures require Linux; other target acceptance is separate")
class InstallationTests(unittest.TestCase):
    """Exercise real process ownership, selective writes, rollback and explicit recovery."""

    def setUp(self):
        """Refuse a generic temp directory outside the project's required playground."""
        root = Path(os.environ["ESKA_TEST_ROOT"]).resolve()
        self.assertEqual(root, (REPOSITORY.parent / "eska-playground").resolve())
        self.directory = tempfile.TemporaryDirectory(prefix="eska-components-test-", dir=root)
        self.addCleanup(self.directory.cleanup)
        self.base = Path(self.directory.name)
        self.root = self.base / "global installation"

    def apply(self, *args, **kwargs):
        """Install a generated package through the actual transaction engine."""
        data, checksum = fixture(*args, **kwargs)
        return installation.install(self.root, data, checksum)

    def failure(self, code, operation):
        """A rejected operation must retain its documented stable category."""
        with self.assertRaises(package.Failure) as raised:
            operation()
        self.assertEqual(raised.exception.code, code)

    def test_independent_update_remove_and_offline_repeat(self):
        """One selected component never replaces sibling tools or unowned files."""
        self.apply()
        self.apply("eska-vcs")
        (self.root / "bin/bsl-analyzer-app").write_bytes(b"independent analyzer")
        (self.root / "share/eska/eska-project/user.txt").write_bytes(b"user note")
        previous = snapshot(self.root)
        result = self.apply(release="1.1.0", notice=b"updated notice")
        self.assertEqual(result["status"], "updated")
        for name, value in previous.items():
            if "eska-vcs" in name or name.endswith(("bsl-analyzer-app", "user.txt")):
                self.assertEqual(snapshot(self.root)[name], value)
        before = snapshot(self.root)
        self.assertEqual(self.apply(release="1.1.0", notice=b"updated notice")["status"], "up-to-date")
        self.assertEqual(snapshot(self.root), before)
        self.assertEqual(installation.remove(self.root, "eska-project")["status"], "removed")
        self.assertFalse((self.root / "bin/eska-project").exists())
        self.assertTrue((self.root / "bin/eska-vcs").is_file())
        self.assertEqual((self.root / "share/eska/eska-project/user.txt").read_bytes(), b"user note")
        self.assertEqual(list(self.root.glob(".eska-component-*")), [])

    def test_check_and_unknown_installation_do_not_write(self):
        """A preview stays read-only; --force never adopts an unmanaged executable."""
        data, checksum = fixture()
        self.assertEqual(installation.install(self.root, data, checksum, check=True)["status"], "install-available")
        self.assertFalse(self.root.exists())
        (self.root / "bin").mkdir(parents=True)
        (self.root / "bin/eska-project").write_bytes(b"custom binary")
        previous = snapshot(self.root)
        self.failure("unmanaged", lambda: installation.install(self.root, data, checksum, force=True))
        self.assertEqual(snapshot(self.root), previous)

    def test_modified_files_block_replacement_and_removal(self):
        """Byte ownership is rechecked before both mutations."""
        self.apply()
        (self.root / "bin/eska-project").write_bytes(b"user modification")
        previous = snapshot(self.root)
        self.failure("modified", lambda: self.apply(release="1.1.0"))
        self.failure("modified", lambda: installation.remove(self.root, "eska-project"))
        self.assertEqual(snapshot(self.root), previous)

    def test_checksum_schema_and_archive_paths_fail_before_installation(self):
        """A valid ZIP is insufficient when its declared contract or namespace is invalid."""
        data, checksum = fixture()
        self.failure("checksum", lambda: installation.install(self.root, data, "0" * 64))
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            manifest = json.loads(archive.read("package.json"))
            payloads = {name: archive.read(name) for name in archive.namelist() if name != "package.json"}
        for key, value in [("config_schema", "designer-xml-2"), ("bridge_schema", 2), ("metadata_api", {"major": 2, "minor": 12})]:
            changed = json.loads(json.dumps(manifest))
            changed["contract"][key] = value
            bad, sha = zipped(changed, payloads)
            self.failure("incompatible", lambda: installation.install(self.root, bad, sha))
        bad, sha = zipped(manifest, payloads, ("../outside", b"escape"))
        self.failure("archive", lambda: installation.install(self.root, bad, sha))
        manifest["files"][0]["path"] = "../outside"
        bad, sha = zipped(manifest, payloads)
        self.failure("path", lambda: installation.install(self.root, bad, sha))
        self.assertFalse(self.root.exists())
        self.assertFalse((self.base / "outside").exists())

    def test_probe_failure_after_publication_restores_exact_previous_files(self):
        """A lying executable cannot leave an update merely because file copying succeeded."""
        self.apply()
        previous = snapshot(self.root)
        self.failure("incompatible", lambda: self.apply(release="1.1.0", after_publish=True))
        self.assertEqual(snapshot(self.root), previous)
        self.assertEqual(list(self.root.glob(".eska-component-*")), [])

    def test_publication_failure_and_cancellation_roll_back(self):
        """A filesystem error or cancellation after writes preserves the former installation."""
        self.apply()
        previous = snapshot(self.root)
        replace = os.replace
        failed = False
        def reject_receipt(source, destination):
            """Fail once after the binary and notices have already been replaced."""
            nonlocal failed
            if Path(destination) == self.root / "components/eska-project.json" and not failed:
                failed = True
                raise OSError("injected write failure")
            return replace(source, destination)
        with mock.patch.object(installation.os, "replace", reject_receipt):
            with self.assertRaises(OSError):
                self.apply(release="1.1.0")
        self.assertTrue(failed)
        self.assertEqual(snapshot(self.root), previous)
        publish = installation.publish
        def cancel_after_writes(*args):
            """Exercise the same unwind used by terminal cancellation."""
            publish(*args)
            raise package.Failure("cancelled")
        with mock.patch.object(installation, "publish", cancel_after_writes):
            self.failure("cancelled", lambda: self.apply(release="1.1.0"))
        self.assertEqual(snapshot(self.root), previous)

    def test_explicit_recovery_preserves_backups_on_external_conflict(self):
        """Incomplete rollback is recoverable, and recovery never overwrites unknown current bytes."""
        self.apply()
        previous = snapshot(self.root)
        with mock.patch.object(installation, "restore", side_effect=OSError("injected rollback failure")):
            self.failure("rollback", lambda: self.apply(release="1.1.0", after_publish=True))
        stage, = self.root.glob(".eska-component-*")
        self.failure("recovery-required", lambda: self.apply(release="1.2.0"))
        binary = self.root / "bin/eska-project"
        candidate = binary.read_bytes()
        binary.write_bytes(b"external change")
        current = snapshot(self.root)
        self.failure("rollback-conflict", lambda: installation.recover(self.root, stage))
        self.assertEqual(snapshot(self.root), current)
        binary.write_bytes(candidate)
        self.assertEqual(installation.recover(self.root, stage)["status"], "recovered")
        self.assertEqual(snapshot(self.root), previous)

    def test_two_languages_have_identical_json_and_real_custom_probe(self):
        """The standalone installer works without eska and never claims a user executable."""
        data, checksum = fixture()
        archive = self.base / "package.zip"
        archive.write_bytes(data)
        outputs = []
        for language in ("ru-RU", "en-US"):
            root = self.base / language
            result = subprocess.run([sys.executable, "-B", str(REPOSITORY / "tools/components.py"), "install", "--archive", str(archive),
                "--sha256", checksum, "--root", str(root), "--format", "json", "--lang", language], capture_output=True, cwd=self.base)
            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            self.assertFalse(result.stderr)
            outputs.append(result.stdout)
            result = subprocess.run([sys.executable, "-B", str(REPOSITORY / "tools/components.py"), "inspect", "--executable", str(root / "bin/eska-project"),
                "--root", str(self.base / "unused"), "--format", "json", "--lang", language], capture_output=True, cwd=self.base)
            self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
            self.assertEqual(json.loads(result.stdout)["method"], "custom")
        self.assertEqual(*outputs)
        self.assertFalse((self.base / "unused").exists())
        self.assertEqual(set(components.MESSAGES["ru-RU"]), set(components.MESSAGES["en-US"]))

    def test_force_changes_only_an_owned_release_and_lock_is_exclusive(self):
        """Downgrades and rebuilt versions need explicit intent; another owner retains its lock."""
        self.apply(release="2.0.0")
        previous = snapshot(self.root)
        self.failure("downgrade", lambda: self.apply(release="1.0.0"))
        self.failure("same-version", lambda: self.apply(release="2.0.0", notice=b"rebuilt"))
        self.assertEqual(snapshot(self.root), previous)
        data, checksum = fixture(release="1.0.0")
        with package.installation_lock(self.root):
            self.failure("busy", lambda: installation.install(self.root, data, checksum, force=True))
        self.assertEqual(snapshot(self.root), previous)
        self.assertEqual(installation.install(self.root, data, checksum, force=True)["status"], "updated")

    def test_other_component_incompatibility_and_symlink_parent_are_preserved(self):
        """Preflight neither repairs another tool nor follows an unexpected install parent."""
        self.apply("eska-vcs")
        path = self.root / "components/eska-vcs.json"
        value = json.loads(path.read_bytes())
        value["package"]["contract"]["bridge_schema"] = 99
        path.write_bytes(package.json_bytes(value))
        previous = snapshot(self.root)
        self.failure("incompatible", lambda: self.apply())
        self.assertEqual(snapshot(self.root), previous)
        isolated = self.base / "symlink installation"
        isolated.mkdir()
        outside = self.base / "user directory"
        outside.mkdir()
        (outside / "keep").write_bytes(b"user data")
        (isolated / "bin").symlink_to(outside, target_is_directory=True)
        data, checksum = fixture()
        self.failure("path", lambda: installation.install(isolated, data, checksum))
        self.assertEqual({path.name for path in outside.iterdir()}, {"keep"})
        self.assertEqual((outside / "keep").read_bytes(), b"user data")

    def test_shared_claim_recovery_never_evicts_a_live_or_unknown_owner(self):
        """Explorer and CLI share one nonce claim; only explicit recovery may remove a dead owner."""
        self.root.mkdir()
        claim = self.root / ".eska-components.claim"
        active = {"schema_version": 1, "pid": os.getpid(), "nonce": "a" * 32}
        claim.write_bytes(package.json_bytes(active))
        self.failure("busy", lambda: self.apply())
        with self.assertRaises(package.Failure):
            with package.installation_lock(self.root, recover_claim=True):
                self.fail("live owner evicted")
        self.assertEqual(json.loads(claim.read_bytes()), active)
        child = subprocess.Popen([sys.executable, "-c", "pass"])
        child.wait(timeout=5)
        claim.write_bytes(package.json_bytes({**active, "pid": child.pid}))
        with package.installation_lock(self.root, recover_claim=True):
            self.assertEqual(json.loads(claim.read_bytes())["pid"], os.getpid())
        self.assertFalse(claim.exists())
        self.assertFalse((self.root / ".eska-components.recovering").exists())

    def test_cli_failure_json_is_locale_independent(self):
        """The same rejected archive has one machine result while help remains localized."""
        data, checksum = fixture()
        path = self.base / "package.zip"
        path.write_bytes(data)
        values, helps = [], []
        for language in ("ru-RU", "en-US"):
            command = [sys.executable, "-B", str(REPOSITORY / "tools/components.py")]
            result = subprocess.run(command + ["install", "--archive", str(path), "--sha256", "0" * 64,
                "--root", str(self.root), "--format", "json", "--lang", language], capture_output=True, cwd=self.base)
            self.assertEqual(result.returncode, 1)
            self.assertFalse(result.stderr)
            self.assertEqual(json.loads(result.stdout)["code"], "checksum")
            values.append(result.stdout)
            help_result = subprocess.run(command + ["--help", "--lang", language], capture_output=True, cwd=self.base)
            self.assertEqual(help_result.returncode, 0)
            self.assertIn(b"--manifest-sha256", help_result.stdout)
            helps.append(help_result.stdout)
        self.assertEqual(*values)
        self.assertNotEqual(*helps)
        self.assertFalse(self.root.exists())

    def test_real_cancellation_reaps_the_probe(self):
        """SIGTERM during verification leaves no owned executable process or partial installed binary."""
        marker = self.base / "native-probe-pid"
        data, checksum = fixture(probe_code=f"Path({str(marker)!r}).write_text(str(os.getpid()))\nwhile True: time.sleep(.05)")
        path = self.base / "slow.zip"
        path.write_bytes(data)
        child = subprocess.Popen([sys.executable, "-B", str(REPOSITORY / "tools/components.py"), "install", "--archive", str(path),
            "--sha256", checksum, "--root", str(self.root), "--format", "json"], stdout=subprocess.PIPE, stderr=subprocess.PIPE, cwd=self.base)
        try:
            deadline = time.monotonic() + 10
            while not marker.exists():
                self.assertIsNone(child.poll())
                self.assertLess(time.monotonic(), deadline)
                time.sleep(.02)
            pid = int(marker.read_text())
            child.send_signal(signal.SIGTERM)
            stdout, stderr = child.communicate(timeout=10)
            self.assertEqual(child.returncode, 1)
            self.assertEqual(json.loads(stdout)["code"], "cancelled")
            self.assertFalse(stderr)
            with self.assertRaises(ProcessLookupError):
                os.kill(pid, 0)
            self.assertFalse((self.root / "bin/eska-project").exists())
            self.assertEqual(list(self.root.glob(".eska-component-*")), [])
        finally:
            if child.poll() is None:
                child.kill()
            child.wait()


if __name__ == "__main__":
    unittest.main()
