"""Archive packaging preserves selected scope, reproducible bytes and installer licensing."""

import hashlib
import io
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock
import zipfile

from test_installation import REPOSITORY, fixture
import component_package
import package_components


@unittest.skipUnless(sys.platform.startswith("linux"), "Executable fixture requires Linux")
class PackagingTests(unittest.TestCase):
    """The graph fixture avoids Cargo/network while exercising the real producer and process probe."""

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
