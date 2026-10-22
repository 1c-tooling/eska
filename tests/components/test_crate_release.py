"""Cargo release checks cover registry manifests and explicit partial-release recovery."""

import io
import os
from pathlib import Path
import sys
import tarfile
import tempfile
import tomllib
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
import release_bundle
from component_package import Failure


class CrateReleaseTests(unittest.TestCase):
    """Model a new workspace version whose test helpers are not yet on crates.io."""

    def setUp(self):
        """Keep source manifests and registry archives in an owned playground directory."""
        directory = tempfile.TemporaryDirectory(prefix="crate-release-test-", dir=os.environ["ESKA_TEST_ROOT"])
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.patch_root = patch.object(release_bundle, "ROOT", self.root)
        self.patch_root.start()
        self.addCleanup(self.patch_root.stop)
        self.packages = self.root / "packages"
        self.packages.mkdir()
        for name, directory in [("eska", self.root), ("eska-project", self.root / "crates/eska-project")]:
            directory.mkdir(parents=True, exist_ok=True)
            content = f'[package]\nname = "{name}"\nversion.workspace = true\npublish = true\n'
            if name == "eska":
                content += ('[workspace.package]\nversion = "1.2.3"\n[workspace.dependencies]\n'
                            'eska-project = { path = "crates/eska-project", version = "=1.2.3" }\n')
            (directory / "Cargo.toml").write_text(content, encoding="utf-8")
            self.write_archive(name)
        self.config = ('[workspace]\nrelease_always = false\ngit_release_enable = false\n\n'
                       '[changelog]\nheader = "История изменений"\n')
        (self.root / "release-plz.toml").write_text(self.config, encoding="utf-8")

    def write_archive(self, name, extra="", version="1.2.3"):
        """Write the normalized Cargo manifest that a registry consumer would resolve."""
        content = f'[package]\nname = "{name}"\nversion = "{version}"\n' + extra
        content = content.encode("utf-8")
        with tarfile.open(self.packages / f"{name}-1.2.3.crate", "w:gz") as archive:
            member = tarfile.TarInfo(f"{name}-1.2.3/Cargo.toml")
            member.size = len(content)
            archive.addfile(member, io.BytesIO(content))

    def test_runtime_dependencies_and_external_test_helpers_are_allowed(self):
        """Only local test helpers must disappear; runtime edges still order publication."""
        self.write_archive("eska", '[dependencies.eska-project]\nversion = "=1.2.3"\n'
                                   '[dev-dependencies]\nserde = "1"\n')
        release_bundle.verify_crates(self.packages)

    def test_internal_test_dependencies_reject_sequential_publication(self):
        """Detect direct, renamed and target-specific dev edges hidden by workspace packaging."""
        for extra in [
            '[dev-dependencies.eska-project]\nversion = "=1.2.3"\n',
            '[dev-dependencies]\neska-project = "=1.2.3"\n',
            '[target.\'cfg(windows)\'.dev-dependencies.helper]\npackage = "eska-project"\nversion = "=1.2.3"\n',
        ]:
            with self.subTest(extra=extra):
                self.write_archive("eska", extra)
                with self.assertRaises(Failure) as error:
                    release_bundle.verify_crates(self.packages)
                self.assertEqual(error.exception.code, "registry-dev-dependency")

    def test_missing_crate_is_rejected(self):
        """A stale or partial packaging directory cannot satisfy the release gate."""
        (self.packages / "eska-project-1.2.3.crate").unlink()
        with self.assertRaises(Failure) as error:
            release_bundle.verify_crates(self.packages)
        self.assertEqual(error.exception.code, "missing-crate")

    def test_mismatched_archive_version_is_rejected(self):
        """The archive filename alone does not prove which version was packaged."""
        self.write_archive("eska", version="1.2.2")
        with self.assertRaises(Failure) as error:
            release_bundle.verify_crates(self.packages)
        self.assertEqual(error.exception.code, "version")

    def test_resume_preserves_config_and_source_version(self):
        """A retry changes only the temporary release gate, preserving Russian release notes."""
        output = self.root / "resume.toml"
        release_bundle.resume_config("1.2.3", output)
        expected = tomllib.loads(self.config)
        expected["workspace"]["release_always"] = True
        self.assertEqual(tomllib.loads(output.read_text(encoding="utf-8")), expected)
        self.assertEqual((self.root / "release-plz.toml").read_text(encoding="utf-8"), self.config)
        self.assertEqual(release_bundle.release_version(), "1.2.3")

    def test_resume_rejects_another_version_before_writing(self):
        """A mistaken manual input must not enable publication of a different workspace version."""
        output = self.root / "resume.toml"
        with self.assertRaises(Failure) as error:
            release_bundle.resume_config("1.2.4", output)
        self.assertEqual(error.exception.code, "version")
        self.assertFalse(output.exists())


if __name__ == "__main__":
    unittest.main()
