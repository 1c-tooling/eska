"""Release failures stop before publication and generated notes preserve commit text."""

import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))
sys.path.insert(0, str(ROOT / "tests"))
import publish_release
import release_bundle
import release_installer_smoke
from component_package import Failure, digest
from test_installation import capabilities


class ReleaseTests(unittest.TestCase):
    """Fixtures model forge responses without network calls or publishing anything."""

    def setUp(self):
        """Keep every artifact and mock release in one owned playground directory."""
        self.directory = tempfile.TemporaryDirectory(prefix="release-test-", dir=os.environ["ESKA_TEST_ROOT"])
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.assets = self.root / "distrib"
        self.assets.mkdir()
        (self.assets / "tool.zip").write_bytes(b"real artifact bytes")
        (self.assets / "SHA256SUMS").write_text(f"{digest(self.assets / 'tool.zip')}  tool.zip\n", encoding="utf-8")
        (self.root / "release-notes.md").write_text("### Добавлено\n\n- *(ide)* Исходное сообщение\n", encoding="utf-8")
        self.remote = [{"name": path.name, "state": "uploaded", "size": path.stat().st_size,
                        "digest": "sha256:" + digest(path)} for path in self.assets.iterdir()]
        self.environment = {"GITHUB_ACTIONS": "true", "GITHUB_REPOSITORY": publish_release.REPOSITORY, "GITHUB_REF": "refs/tags/v1.2.3"}

    def test_local_publish_is_rejected(self):
        """Even an otherwise complete release cannot publish from a local validation run."""
        with patch.dict(os.environ, {"GITHUB_ACTIONS": "false"}), patch.object(publish_release, "gh") as gh:
            with self.assertRaises(Failure):
                publish_release.publish(self.root, "v1.2.3")
            gh.assert_not_called()

    def test_downloaded_assets_reject_omitted_upload_before_installation(self):
        """Every checksummed file, including cargo-dist's sha256.sum, must survive artifact upload."""
        self.assertEqual(release_installer_smoke.verify_assets(self.assets), 1)
        (self.assets / "tool.zip").unlink()
        with self.assertRaises(AssertionError):
            release_installer_smoke.verify_assets(self.assets)

    def test_downloaded_assets_reject_changed_bytes_before_installation(self):
        """Native smoke tests must validate downloaded bytes rather than only run their installers."""
        (self.assets / "tool.zip").write_bytes(b"changed")
        with self.assertRaises(AssertionError):
            release_installer_smoke.verify_assets(self.assets)

    def test_modified_artifact_never_reaches_github(self):
        """Checksums apply to uploaded bytes, not only the earlier packaging stage."""
        (self.assets / "tool.zip").write_bytes(b"changed")
        with patch.dict(os.environ, self.environment), patch.object(publish_release, "gh") as gh:
            with self.assertRaises(Failure):
                publish_release.publish(self.root, "v1.2.3")
            gh.assert_not_called()

    def test_matching_published_release_is_noop(self):
        """A rerun of a completed release never edits notes or replaces artifacts."""
        replies = [json.dumps([[{"tag_name": "v1.2.3", "draft": False, "id": 7}]]), json.dumps([self.remote])]
        with patch.dict(os.environ, self.environment), patch.object(publish_release, "gh", side_effect=replies) as gh:
            publish_release.publish(self.root, "v1.2.3")
            self.assertEqual(gh.call_count, 2)

    def test_changed_remote_asset_is_rejected(self):
        """Never clobber a conflicting draft or completed release on retry."""
        self.remote[0]["digest"] = "sha256:" + "0" * 64
        replies = [json.dumps([[{"tag_name": "v1.2.3", "draft": True, "id": 7}]]), json.dumps([self.remote])]
        with patch.dict(os.environ, self.environment), patch.object(publish_release, "gh", side_effect=replies) as gh:
            with self.assertRaises(Failure):
                publish_release.publish(self.root, "v1.2.3")
            self.assertEqual(gh.call_count, 2)

    def test_partial_draft_uploads_only_missing_assets_before_publication(self):
        """Publishing is the last call after every expected digest has been rechecked."""
        replies = [json.dumps([[{"tag_name": "v1.2.3", "draft": True, "id": 7}]]),
                   json.dumps([self.remote[:1]]), "", json.dumps([self.remote]), ""]
        with patch.dict(os.environ, self.environment), patch.object(publish_release, "gh", side_effect=replies) as gh:
            publish_release.publish(self.root, "v1.2.3")
            self.assertEqual(gh.call_args.args[:2], ("release", "edit"))
            self.assertIn("--draft=false", gh.call_args.args)
            self.assertEqual(sum(call.args[:2] == ("release", "upload") for call in gh.call_args_list), 1)

    def test_new_release_stays_draft_until_all_assets_are_verified(self):
        """Find a new draft through paginated listings; the published-tag endpoint returns 404."""
        releases = [[{"tag_name": "v1.2.2", "draft": False, "id": 6}],
                    [{"tag_name": "v1.2.3", "draft": True, "id": 7}]]
        replies = ["[[]]", "", json.dumps(releases),
                   "[[]]", "", "", json.dumps([self.remote]), ""]
        with patch.dict(os.environ, self.environment), patch.object(publish_release, "gh", side_effect=replies) as gh:
            publish_release.publish(self.root, "v1.2.3")
            create = gh.call_args_list[1].args
            self.assertIn("--draft", create)
            self.assertIn("--verify-tag", create)
            self.assertEqual(gh.call_args_list[2].args,
                             ("api", f"repos/{publish_release.REPOSITORY}/releases", "--paginate", "--slurp"))
            self.assertEqual(gh.call_args.args[:2], ("release", "edit"))
            self.assertIn("--draft=false", gh.call_args.args)

    def test_created_draft_missing_from_listing_stops_before_upload(self):
        """A missing create result must stop safely rather than upload to an unidentified release."""
        with patch.dict(os.environ, self.environment), patch.object(publish_release, "gh", side_effect=["[[]]", "", "[[]]"]) as gh:
            with self.assertRaisesRegex(Failure, "release-not-found"):
                publish_release.publish(self.root, "v1.2.3")
            self.assertEqual(gh.call_count, 3)

    def test_missing_target_prevents_release_assembly(self):
        """A successful single runner is not a complete multi-platform release."""
        with self.assertRaises(Failure):
            release_bundle.collect(self.root, self.root / "output")

    def matrix_parts(self):
        """Model Linux and Windows artifact layouts with distinct bytes and common bootstrap files."""
        targets = ["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"]
        parts = self.root / "parts"
        version = release_bundle.release_version()
        manifests = {}
        for target in targets:
            components, full = parts / ("components-" + target), parts / ("full-" + target)
            components.mkdir(parents=True)
            full.mkdir()
            index = {"schema_version": 1, "version": version, "target": target, "components": {}}
            for tool in release_bundle.TOOLS:
                name = f"{tool}-{version}-{target}.zip"
                archive = components / name
                archive.write_bytes(name.encode())
                contract = {**capabilities(tool, version), "target": target}
                index["components"][tool] = {"archive": name, "size": archive.stat().st_size,
                                             "sha256": digest(archive), "contract": contract}
            (components / "release.json").write_text(json.dumps(index), encoding="utf-8")
            for name in ("LICENSE", "components.py", "component_installation.py", "component_package.py"):
                (components / name).write_bytes(name.encode())
            name = f"eska-{target}" + (".zip" if target.endswith("windows-msvc") else ".tar.xz")
            (full / name).write_bytes(target.encode())
            checksum = digest(full / name)
            (full / (name + ".sha256")).write_text(checksum, encoding="utf-8")
            artifact = {"checksums": {"sha256": checksum}}
            manifests[name] = artifact
            (full / (target + "-dist-manifest.json")).write_text(json.dumps({
                "announcement_tag": "v" + version, "artifacts": {name: artifact}}), encoding="utf-8")
        return targets, parts, manifests

    def test_complete_matrix_preserves_installer_checksums(self):
        """Merge target manifests before finalization so generated installers can verify their downloads."""
        targets, parts, artifacts = self.matrix_parts()
        output = self.root / "assembled"
        with patch.object(release_bundle, "targets", return_value=targets):
            release_bundle.collect(parts, output)
            self.assertTrue(all((output / (target + "-dist-manifest.json")).is_file() for target in targets))
            (output / "eska-installer.sh").write_text("    # Identical to the sh version, just with a .fish file extension\n", encoding="utf-8")
            (output / "eska-installer.ps1").write_text("  # The replace call here ensures proper escaping is inlined into the receipt\n", encoding="utf-8")
            (output / "dist-manifest.json").write_text(json.dumps({"artifacts": artifacts}), encoding="utf-8")
            release_bundle.finalize(output, self.root / "notes.md")
        manifest = json.loads((output / "dist-manifest.json").read_bytes())
        self.assertEqual(manifest["artifacts"], artifacts)
        self.assertIn("## Установка", manifest["announcement_github_body"])
        self.assertFalse(list(output.glob("*-dist-manifest.json")))
        for line in (output / "SHA256SUMS").read_text().splitlines():
            checksum, name = line.split("  ", 1)
            self.assertEqual(digest(output / name), checksum)

    def test_conflicting_common_bootstrap_rejects_matrix(self):
        """A platform-specific bootstrap difference must not depend on artifact download order."""
        targets, parts, _ = self.matrix_parts()
        (parts / ("components-" + targets[1]) / "components.py").write_bytes(b"different bootstrap")
        with patch.object(release_bundle, "targets", return_value=targets), self.assertRaises(Failure):
            release_bundle.collect(parts, self.root / "assembled")

    def test_corrupted_full_archive_rejects_matrix(self):
        """The full installer archive is checked independently of selective component ZIPs."""
        targets, parts, _ = self.matrix_parts()
        (parts / ("full-" + targets[0]) / f"eska-{targets[0]}.tar.xz").write_bytes(b"changed")
        with patch.object(release_bundle, "targets", return_value=targets), self.assertRaises(Failure):
            release_bundle.collect(parts, self.root / "assembled")

    def test_notes_keep_scope_and_message_verbatim(self):
        """Russian headings and installation prose must not rewrite source commit descriptions."""
        message = "- *(ide)* Добавлено читаемое представление ссылок и типов свойств"
        (self.root / "CHANGELOG.md").write_text(f"## [1.2.3](example) - 2026-10-22\n\n### Добавлено\n\n{message}\n\n## [1.2.2]\nold\n", encoding="utf-8")
        with patch.object(release_bundle, "ROOT", self.root):
            notes = release_bundle.notes("1.2.3")
            self.assertIn(message, notes)
            self.assertIn("## Установка", notes)
            self.assertNotIn("old", notes)
            self.assertNotIn("## Install", notes)


if __name__ == "__main__":
    unittest.main()
