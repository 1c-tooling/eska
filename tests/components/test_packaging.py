"""Archive packaging preserves selected scope, reproducible bytes and installer licensing."""

import hashlib
import io
import os
from pathlib import Path
import shutil
import sys
import subprocess
import tempfile
import unittest
from unittest import mock
import zipfile

from test_installation import REPOSITORY, fixture
import component_package
import component_notices
import package_components


class NoticeErrorsTests(unittest.TestCase):
    """Retain the actionable Cargo diagnostic instead of reporting only subprocess exit status."""

    def test_offline_metadata_failure_preserves_cargo_stderr(self):
        """A missing foreign-target dependency must be identifiable in the packaging JSON detail."""
        diagnostic = b'error: failed to download `anstyle-wincon v3.0.11`\n\nattempting HTTP with --offline\n'
        error = subprocess.CalledProcessError(101, ["cargo", "metadata"], stderr=diagnostic)
        with mock.patch.object(component_notices.subprocess, "run", side_effect=error), \
                self.assertRaises(component_package.Failure) as raised:
            component_notices.metadata(REPOSITORY)
        self.assertEqual(raised.exception.code, "cargo")
        self.assertEqual(raised.exception.detail, diagnostic.decode().strip())


class NoticeCheckoutTests(unittest.TestCase):
    """Exercise Windows-style Git checkout without changing upstream notice bytes."""

    def test_autocrlf_preserves_upstream_checksums(self):
        """A fresh CRLF checkout must retain every exact checksum recorded in provenance."""
        playground = Path(os.environ["ESKA_TEST_ROOT"]).resolve()
        self.assertEqual(playground, (REPOSITORY.parent / "eska-playground").resolve())
        with tempfile.TemporaryDirectory(prefix="eska-notice-checkout-", dir=playground) as temporary:
            checkout = Path(temporary)
            shutil.copy2(REPOSITORY / ".gitattributes", checkout)
            notices = checkout / "tools/license-overrides"
            shutil.copytree(REPOSITORY / "tools/license-overrides", notices)
            control = checkout / "control.txt"
            control.write_bytes(b"ordinary text\n")
            subprocess.run(["git", "init", "--quiet"], cwd=checkout, check=True, capture_output=True)
            subprocess.run(["git", "-c", "core.autocrlf=false", "add", "."],
                           cwd=checkout, check=True, capture_output=True)
            shutil.rmtree(notices)
            control.unlink()
            subprocess.run(["git", "-c", "core.autocrlf=true", "checkout-index", "--all"],
                           cwd=checkout, check=True, capture_output=True)
            self.assertEqual(control.read_bytes(), b"ordinary text\r\n")
            provenances = list(notices.glob("*/provenance.json"))
            self.assertTrue(provenances)
            for provenance in provenances:
                for filename, declaration in component_package.parse_json(provenance.read_bytes())["files"].items():
                    with self.subTest(crate=provenance.parent.name, filename=filename):
                        self.assertEqual(component_package.digest(provenance.parent / filename), declaration["sha256"])


@unittest.skipUnless(sys.platform.startswith("linux"), "Executable fixture requires Linux")
class PackagingTests(unittest.TestCase):
    """The graph fixture avoids Cargo/network while exercising the real producer and process probe."""

    def test_failed_probe_preserves_exit_status_and_stderr(self):
        """A runtime crash must expose its process result instead of only the executable path."""
        playground = Path(os.environ["ESKA_TEST_ROOT"]).resolve()
        self.assertEqual(playground, (REPOSITORY.parent / "eska-playground").resolve())
        with tempfile.TemporaryDirectory(prefix="eska-probe-test-", dir=playground) as temporary:
            base = Path(temporary)
            for returncode in (0, 7):
                with self.subTest(returncode=returncode):
                    data, _checksum = fixture(probe_code=f"sys.stderr.buffer.write(b'runtime failure\\xff'); sys.exit({returncode})")
                    executable = base / "eska-project"
                    with zipfile.ZipFile(io.BytesIO(data)) as archive:
                        executable.write_bytes(archive.read("bin/eska-project"))
                    executable.chmod(0o755)
                    with self.assertRaises(component_package.Failure) as raised:
                        component_package.probe(executable, base)
                    self.assertEqual(raised.exception.code, "probe")
                    self.assertEqual(component_package.parse_json(raised.exception.detail), {
                        "executable": str(executable), "returncode": returncode, "stderr": "runtime failure\ufffd",
                    })

    def test_selected_package_reproduces_bytes_and_carries_the_bootstrap_license(self):
        """A local bundle includes only requested tools and never replaces an existing output directory."""
        playground = Path(os.environ["ESKA_TEST_ROOT"]).resolve()
        self.assertEqual(playground, (REPOSITORY.parent / "eska-playground").resolve())
        with tempfile.TemporaryDirectory(prefix="eska-package-test-", dir=playground) as temporary:
            base = Path(temporary)
            binaries = base / "binaries"
            binaries.mkdir()
            data, _checksum = fixture()
            with zipfile.ZipFile(io.BytesIO(data)) as archive:
                executable = binaries / "eska-project"
                executable.write_bytes(archive.read("bin/eska-project"))
                executable.chmod(0o755)
            with mock.patch.object(package_components, "metadata", return_value=({}, {})), \
                    mock.patch.object(package_components, "notices", return_value={"LICENSE": b"fixture notice"}):
                first = package_components.build(REPOSITORY, binaries, base / "one", ["eska-project"])
                second = package_components.build(REPOSITORY, binaries, base / "two", ["eska-project"])
            self.assertEqual(first, second)
            self.assertEqual(set(first["components"]), {"eska-project"})
            name = first["components"]["eska-project"]["archive"]
            self.assertEqual((base / "one" / name).read_bytes(), (base / "two" / name).read_bytes())
            self.assertEqual((base / "one/LICENSE").read_bytes(), (REPOSITORY / "LICENSE").read_bytes())
            for line in (base / "one/SHA256SUMS").read_text().splitlines():
                checksum, filename = line.split("  ", 1)
                self.assertEqual(hashlib.sha256((base / "one" / filename).read_bytes()).hexdigest(), checksum)
            previous = {path.name: path.read_bytes() for path in (base / "one").iterdir()}
            with self.assertRaises(component_package.Failure) as raised:
                package_components.build(REPOSITORY, binaries, base / "one", ["eska-project"])
            self.assertEqual(raised.exception.code, "unmanaged")
            self.assertEqual({path.name: path.read_bytes() for path in (base / "one").iterdir()}, previous)


if __name__ == "__main__":
    unittest.main()
