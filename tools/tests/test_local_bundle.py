"""Local bundle proofs use private playground fixtures and never compile Rust or invoke a platform."""

import importlib.util
import json
import os
from pathlib import Path
import signal
import struct
import subprocess
import sys
import tempfile
import time
import unittest
import zipfile
from unittest import mock

REPOSITORY = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("local_bundle", REPOSITORY / "tools/local-bundle.py")
BUNDLE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUNDLE)

CARGO = r'''
import json, os, pathlib, subprocess, sys, time
root = pathlib.Path.cwd()
mode_path = root / "mode.json"
mode = json.loads(mode_path.read_text()) if mode_path.exists() else {}
with (root / "cargo-log.jsonl").open("a") as log:
    log.write(json.dumps(sys.argv[1:]) + "\n")
if mode.get("sleep"):
    child = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
    (root / "running.json").write_text(json.dumps([os.getpid(), child.pid]))
    time.sleep(60)
if mode.get("fail_build"):
    sys.exit(12)
target = pathlib.Path(sys.argv[sys.argv.index("--target-dir") + 1])
target.mkdir(parents=True, exist_ok=True)
names = [sys.argv[index + 1] for index, arg in enumerate(sys.argv) if arg == "--bin"]
for name in names:
    executable = target / (name + (".exe" if os.name == "nt" else ""))
    body = f"""import json, pathlib, sys, time
component = {name!r}
mode_path = pathlib.Path({str(mode_path)!r})
mode = json.loads(mode_path.read_text()) if mode_path.exists() else {{}}
if mode.get('probe_sleep') == component: time.sleep(60)
if mode.get('probe_oversized') == component:
    sys.stdout.write('x' * 70000); sys.stdout.flush(); sys.exit(0)
if mode.get('stderr_oversized') == component:
    sys.stderr.write('x' * 70000); sys.stderr.flush(); sys.exit(0)
version = '0.0.0' if component in ('eska-check', 'eska-analyzer') else '0.14.2'
if mode.get('wrong_version') == component: version = '99.0.0'
if component == 'eska': print('eska ' + version); sys.exit(0)
data = {{'version': version, 'protocol_version': '0.1' if component == 'eska-analyzer' else 1}}
if component in ('eska-check', 'eska-analyzer'):
    data.update(source_core_version='0.14.2', profiles=['syntax','semantic'], source_formats=['eska'])
    data['commands'] = ['check'] if component == 'eska-check' else ['serve']
    if component == 'eska-check': data['report_schema_version'] = 1
else:
    data['commands'] = ['new','init','import','prepare'] if component == 'eska-project' else ['build','unpack','import']
if mode.get('wrong_core') == component: data['source_core_version'] = '0.13.0'
if mode.get('wrong_protocol') == component: data['protocol_version'] = True
if mode.get('wrong_array') == component: data['profiles'] = [{{'syntax': True}}, 'semantic']
if mode.get('missing_commands') == component: data.pop('commands', None)
if mode.get('wrong_commands') == component: data['commands'] = ['other']
if mode.get('malformed_commands') == component: data['commands'] = [{{'check': True}}, 'serve']
if mode.get('missing_report_schema') == component: data.pop('report_schema_version', None)
if mode.get('wrong_report_schema') == component: data['report_schema_version'] = 2
if mode.get('boolean_report_schema') == component: data['report_schema_version'] = True
value = {{'schema_version':1, 'tool':component, 'command':'capabilities', 'status':'success', 'data':data}}
if mode.get('duplicate_json') == component:
    print('{{"schema_version":1,"schema_version":1}}'); sys.exit(0)
if mode.get('self_change') == component:
    with pathlib.Path(sys.argv[0]).open('a') as changed: changed.write('\\n# changed\\n')
print(json.dumps(value))
"""
    executable.write_text("#!" + sys.executable + "\n" + body)
    executable.chmod(0o755)
    if mode.get("outside"):
        executable = root / "outside-tool"
        executable.write_text("unrelated bytes")
    print(json.dumps({"reason":"compiler-artifact", "target":{"name":name}, "executable":str(executable)}))
'''


@unittest.skipUnless(sys.platform == "linux", "Atomic local publication is exercised on Linux")
class LocalBundleTests(unittest.TestCase):
    """Synthetic processes test the publication boundary and actual argument/receipt contracts."""

    def setUp(self):
        """Every fixture belongs to one exclusive directory in the explicitly configured sibling playground."""
        playground = Path(os.environ["ESKA_TEST_ROOT"]).resolve(strict=True)
        self.assertEqual(playground, (REPOSITORY.parent / "eska-playground").resolve(strict=True))
        self.directory = tempfile.TemporaryDirectory(prefix="ef21-local-bundle-", dir=playground)
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.repository = self.root / "source repository with spaces"
        self.repository.mkdir()
        analysis = self.repository / "tools/analysis-core"
        analysis.mkdir(parents=True)
        (self.repository / "Cargo.toml").write_text('[package]\nname="eska"\nversion="0.14.2"\n')
        (analysis / "Cargo.toml").write_text('[package]\nname="eska-analysis"\nversion="0.0.0"\n')
        for path in (self.repository / "Cargo.lock", analysis / "Cargo.lock"):
            path.write_text("version = 4\n")
        self.inputs = [self.repository / name for name in (
            "src/lib.rs", "locales/en-US/main.ftl", "tools/analysis-core/src/lib.rs",
            "tools/bsl-adapter/src/lib.rs", "tools/bsl-kernel/crates/ide-db/src/lib.rs")]
        for path in self.inputs:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"initial input\n")
        self.cargo = self.root / "fake cargo with spaces"
        self.cargo.write_text("#!" + sys.executable + "\n" + CARGO)
        self.cargo.chmod(0o755)

    def mode(self, **values):
        """Only the owned fake producer receives fault controls; production utility has no test switches."""
        (self.repository / "mode.json").write_text(json.dumps(values))

    def build(self, name="bundle with spaces", components=None, previous=None, timeout=1):
        """Use the public build operation with real subprocess pipes and private Cargo artifact paths."""
        destination = self.root / name
        receipt, digest = BUNDLE.build(self.repository, destination, explicit=components, previous=previous,
                                       cargo=self.cargo, build_timeout=5, probe_timeout=timeout)
        return destination, receipt, digest

    def assert_no_publication(self, destination):
        """Failures remove only owned staging and leave the requested destination completely unclaimed."""
        self.assertFalse(os.path.lexists(destination))
        self.assertEqual(list(self.root.glob(".eska-local-bundle-*")), [])

    def explorer(self, name="explorer with spaces.vsix", change=None, extra=None):
        """Write an owned static VSIX fixture; admission never executes or installs its JavaScript."""
        path = self.root / name
        value = {"name": "eska-explorer", "publisher": "1c-tooling", "version": "0.0.11",
                 "main": "./out/extension.js", "engines": {"vscode": "^1.109.0"},
                 "contributes": {"configuration": {"properties": {
                     "eska.explorer.backend": {"enum": ["designer-xml", "analyzer"]},
                     "eska.explorer.analyzerExecutable": {"default": "eska-analyzer"}}}}}
        if change:
            change(value)
        with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("extension/package.json", json.dumps(value))
            archive.writestr("extension/out/extension.js", "throw new Error('must not execute');")
            if extra:
                archive.writestr(*extra)
        return path

    def test_package_preserves_verified_cli_selection_and_source_without_cargo(self):
        """A complete new bundle contains exact CLI/VSIX bytes while old inputs and compiler logs stay unchanged."""
        old, receipt, digest = self.build(components=["eska-check", "eska-analyzer"])
        vsix = self.explorer()
        tracked = [p for p in old.rglob("*") if p.is_file()] + [vsix, self.repository / "cargo-log.jsonl"]
        before = {p: (p.read_bytes(), p.stat().st_mtime_ns, p.stat().st_mode) for p in tracked}
        destination = self.root / "paired bundle"
        paired, paired_digest = BUNDLE.package(old, destination, vsix, probe_timeout=1)
        self.assertEqual(paired["components"], ["eska-check", "eska-analyzer"])
        self.assertEqual(paired["files"], receipt["files"])
        self.assertEqual(paired["previous_receipt_sha256"], digest)
        self.assertEqual(paired["explorer"]["manifest"]["extension_id"], "1c-tooling.eska-explorer")
        self.assertEqual((destination / BUNDLE.EXPLORER_PATH).read_bytes(), vsix.read_bytes())
        self.assertEqual(BUNDLE.verify(destination, timeout=1), (paired, paired_digest))
        self.assertEqual(before, {p: (p.read_bytes(), p.stat().st_mtime_ns, p.stat().st_mode) for p in tracked})
        selective = self.root / "IDE only"
        selected, _ = BUNDLE.package(old, selective, vsix, probe_timeout=1, explicit=["eska-analyzer"])
        self.assertEqual(selected["components"], ["eska-analyzer"])
        self.assertEqual([p.name for p in (selective / "bin").iterdir()], ["eska-analyzer"])
        with self.assertRaises(BUNDLE.Failure) as error:
            BUNDLE.package(old, self.root / "unavailable tool", vsix, explicit=["eska", "eska-analyzer"])
        self.assertEqual(error.exception.code, "components_unavailable")

    def test_rebuild_preserves_optional_explorer_and_requires_explicit_removal(self):
        """Selective updates retain the paired IDE artifact; omission is explicit and cannot leave it without an analyzer."""
        old, _, _ = self.build(components=["eska-analyzer"])
        paired = self.root / "paired"
        BUNDLE.package(old, paired, self.explorer(), probe_timeout=1)
        _, receipt, _ = self.build("inherited explorer", previous=paired)
        self.assertEqual(receipt["components"], ["eska-analyzer"])
        self.assertEqual(receipt["explorer"], BUNDLE.verify(paired, 1)[0]["explorer"])
        before = (self.repository / "cargo-log.jsonl").read_bytes()
        with self.assertRaises(BUNDLE.Failure) as error:
            self.build("missing analyzer", ["eska-project"], paired)
        self.assertEqual(error.exception.code, "explorer_analyzer_missing")
        self.assertEqual((self.repository / "cargo-log.jsonl").read_bytes(), before)
        destination = self.root / "without explorer"
        receipt, _ = BUNDLE.build(self.repository, destination, previous=paired, explicit=["eska-project"],
                                  without_explorer=True, cargo=self.cargo, probe_timeout=1)
        self.assertNotIn("explorer", receipt)
        self.assertEqual(receipt["components"], ["eska-project"])

    def test_optional_explorer_refuses_invalid_manifest_and_archive_before_cargo(self):
        """Malformed, foreign, aliased or duplicate manifests never start a build or publish a partial bundle."""
        changes = [lambda p: p.update(publisher="other"), lambda p: p.update(version=True),
                   lambda p: p.update(main="../escape.js"), lambda p: p.update(contributes={})]
        inputs = [self.explorer("invalid-" + str(index) + ".vsix", change=change) for index, change in enumerate(changes)]
        inputs.append(self.explorer("escape.vsix", extra=("../escape", "unrelated")))
        link = zipfile.ZipInfo("extension/linked"); link.external_attr = (0o120777 << 16)
        inputs.append(self.explorer("link.vsix", extra=(link, "../outside")))
        duplicate = self.explorer("duplicate.vsix")
        with zipfile.ZipFile(duplicate, "a") as archive:
            with self.assertWarns(UserWarning):
                archive.writestr("extension/package.json", "{}")
        inputs.append(duplicate)
        broken = self.root / "broken.vsix"; broken.write_bytes(b"invalid ZIP"); inputs.append(broken)
        for index, vsix in enumerate(inputs):
            destination = self.root / ("rejected-" + str(index))
            with self.assertRaises(BUNDLE.Failure):
                BUNDLE.build(self.repository, destination, explicit=["eska-analyzer"], explorer=vsix, cargo=self.cargo)
            self.assert_no_publication(destination)
        self.assertFalse((self.repository / "cargo-log.jsonl").exists())

    def test_explorer_changed_during_build_is_not_silently_accepted(self):
        """VSIX bytes selected before compilation remain part of the exact input boundary."""
        vsix = self.explorer()
        original = BUNDLE.copy_artifacts
        def mutate(*args):
            """Replace the selected valid archive after compilation but before the bundle copy."""
            original(*args)
            self.explorer(change=lambda p: p.update(version="0.0.12"))
        destination = self.root / "changed explorer"
        with mock.patch.object(BUNDLE, "copy_artifacts", side_effect=mutate), self.assertRaises(BUNDLE.Failure) as error:
            BUNDLE.build(self.repository, destination, explicit=["eska-analyzer"], explorer=vsix, cargo=self.cargo)
        self.assertEqual(error.exception.code, "source_changed")
        self.assert_no_publication(destination)

    def test_corrupt_deflate_is_a_locale_independent_machine_error(self):
        """Broken compressed metadata cannot escape the public JSON error boundary as a traceback."""
        path = self.explorer()
        with zipfile.ZipFile(path) as archive:
            member = archive.getinfo("extension/package.json")
        data = bytearray(path.read_bytes())
        name_size, extra_size = struct.unpack_from("<HH", data, member.header_offset + 26)
        start = member.header_offset + 30 + name_size + extra_size
        data[start:start + member.compress_size] = b"\xff" * member.compress_size
        path.write_bytes(data)
        outputs = []
        for locale in ["ru-RU", "en-US"]:
            destination = self.root / ("broken-deflate-" + locale)
            child = subprocess.run([sys.executable, str(REPOSITORY / "tools/local-bundle.py"), "--lang", locale,
                "build", str(destination), "--repository", str(self.repository), "--components", "eska-analyzer",
                "--cargo", str(self.cargo), "--explorer-vsix", str(path)], capture_output=True)
            self.assertEqual(child.returncode, 1)
            self.assertEqual(child.stderr, b"")
            outputs.append(json.loads(child.stdout))
            self.assert_no_publication(destination)
        self.assertEqual(outputs[0], outputs[1])
        self.assertEqual(outputs[0]["error"]["code"], "explorer_archive_invalid")
        self.assertFalse((self.repository / "cargo-log.jsonl").exists())

    def test_explorer_limit_covers_actual_read_posthash_and_copy(self):
        """Growing a selected archive between admissions cannot bypass its read, hash or copy bound."""
        path = self.explorer()
        saved = path.read_bytes()
        with mock.patch.object(BUNDLE, "EXPLORER_LIMIT", len(saved) - 1), self.assertRaises(BUNDLE.Failure) as error:
            BUNDLE.inspect_explorer(path)
        self.assertEqual(error.exception.code, "explorer_archive_oversized")
        read = BUNDLE.explorer_bytes
        def growing_read(source):
            """Change the selected path only after its bounded snapshot was obtained."""
            raw = read(source)
            source.write_bytes(raw + b"x")
            return raw
        with mock.patch.object(BUNDLE, "EXPLORER_LIMIT", len(saved)), mock.patch.object(BUNDLE, "explorer_bytes", side_effect=growing_read), self.assertRaises(BUNDLE.Failure) as error:
            BUNDLE.inspect_explorer(path)
        self.assertEqual(error.exception.code, "file_oversized")
        path.write_bytes(saved)
        inspect = BUNDLE.inspect_explorer
        def growing_inspect(source):
            """Grow the selected file after admission and before its actual copy read."""
            report = inspect(source)
            source.write_bytes(saved + b"x")
            return report
        bundle = self.root / "private copying"; bundle.mkdir()
        with mock.patch.object(BUNDLE, "EXPLORER_LIMIT", len(saved)), mock.patch.object(BUNDLE, "inspect_explorer", side_effect=growing_inspect), self.assertRaises(BUNDLE.Failure) as error:
            BUNDLE.copy_explorer(path, bundle, ["eska-analyzer"])
        self.assertEqual(error.exception.code, "explorer_archive_oversized")

    def test_verify_fences_explorer_hash_receipt_path_and_alias(self):
        """A paired receipt never grants arbitrary paths or accepts a replaced extension archive."""
        old, _, _ = self.build(components=["eska-analyzer"])
        paired = self.root / "paired"
        receipt, _ = BUNDLE.package(old, paired, self.explorer(), probe_timeout=1)
        archive = paired / BUNDLE.EXPLORER_PATH
        archive.write_bytes(self.explorer("changed.vsix", change=lambda p: p.update(version="0.0.12")).read_bytes())
        with self.assertRaises(BUNDLE.Failure) as error:
            BUNDLE.verify(paired, 1)
        self.assertEqual(error.exception.code, "explorer_changed")
        receipt["explorer"]["path"] = "../outside.vsix"
        (paired / BUNDLE.RECEIPT).write_text(json.dumps(receipt))
        with self.assertRaises(BUNDLE.Failure) as error:
            BUNDLE.verify(paired, 1)
        self.assertEqual(error.exception.code, "receipt_invalid")

    def test_package_cli_is_locale_independent_and_refuses_raced_destination(self):
        """Actual package CLI emits stable JSON, and no-replace publication preserves an intervening directory."""
        old, _, _ = self.build(components=["eska-analyzer"])
        vsix = self.explorer()
        receipts = []
        for locale in ["ru-RU", "en-US"]:
            destination = self.root / locale
            child = subprocess.run([sys.executable, str(REPOSITORY / "tools/local-bundle.py"), "--lang", locale,
                                    "package", str(old), str(destination), "--explorer-vsix", str(vsix)], capture_output=True)
            self.assertEqual(child.returncode, 0, child.stderr.decode())
            self.assertEqual(child.stderr, b"")
            receipts.append(json.loads(child.stdout))
        self.assertEqual(receipts[0], receipts[1])
        publish = BUNDLE.publish
        def raced(source, destination, function):
            """Claim the final name only after all artifact admission has finished."""
            destination.mkdir(); (destination / "keep").write_bytes(b"user bytes")
            publish(source, destination, function)
        destination = self.root / "raced package"
        with mock.patch.object(BUNDLE, "publish", side_effect=raced), self.assertRaises(BUNDLE.Failure) as error:
            BUNDLE.package(old, destination, vsix, 1)
        self.assertEqual(error.exception.code, "destination_exists")
        self.assertEqual((destination / "keep").read_bytes(), b"user bytes")
        self.assertFalse((destination / BUNDLE.RECEIPT).exists())

    def test_full_set_recipes_versions_and_read_only_verify(self):
        """The real five names retain two package versions, exact core/protocol data and stable hashes."""
        root, receipt, digest = self.build()
        self.assertEqual(receipt["components"], list(BUNDLE.COMPONENTS))
        self.assertEqual(receipt["source"]["versions"], {"eska":"0.14.2", "eska-analysis":"0.0.0", "source_core":"0.14.2"})
        for entry in receipt["files"]:
            self.assertEqual(entry["probe"]["protocol_version"], BUNDLE.PROTOCOLS[entry["component"]])
            self.assertEqual(entry["probe"]["source_core_version"], "0.14.2")
        before = {path: (path.read_bytes(), path.stat().st_mtime_ns) for path in root.rglob("*") if path.is_file()}
        verified, actual = BUNDLE.verify(root, timeout=1)
        self.assertEqual((verified, actual), (receipt, digest))
        self.assertEqual(before, {path: (path.read_bytes(), path.stat().st_mtime_ns) for path in before})
        recipes = [json.loads(line) for line in (self.repository / "cargo-log.jsonl").read_text().splitlines()]
        self.assertEqual(len(recipes), 3)
        self.assertNotIn("--no-default-features", recipes[0])
        self.assertIn("--no-default-features", recipes[1])
        self.assertIn("project-cli,build-cli", recipes[1])
        self.assertIn("check-cli,analyzer-cli", recipes[2])
        self.assertTrue(all("--locked" in arguments for arguments in recipes))

    def test_selective_rebuild_inherits_set_and_explicit_change_replaces_it(self):
        """A check-only receipt never widens implicitly into a full toolkit or installs companions."""
        old, receipt, digest = self.build("check only", ["eska-check"])
        old_bytes = (old / BUNDLE.RECEIPT).read_bytes()
        _, inherited, _ = self.build("inherited", previous=old)
        self.assertEqual(inherited["components"], ["eska-check"])
        self.assertEqual(inherited["previous_receipt_sha256"], digest)
        _, explicit, _ = self.build("explicit", ["eska-project", "eska-build"], old)
        self.assertEqual(explicit["components"], ["eska-project", "eska-build"])
        self.assertEqual((old / BUNDLE.RECEIPT).read_bytes(), old_bytes)
        self.assertEqual(receipt["components"], ["eska-check"])

    def test_source_fingerprint_covers_linked_inputs_and_ignores_outputs(self):
        """Every linked package and locale is fenced by bytes; private compiler/cache/test outputs are excluded."""
        before = BUNDLE.source_fingerprint(self.repository)
        for name in ("tools/analysis-core/target/new-output", "tools/.cache/generated", "tools/tests/test_fixture"):
            path = self.repository / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(b"excluded output")
        self.assertEqual(BUNDLE.source_fingerprint(self.repository), before)
        for path in self.inputs:
            saved = path.read_bytes()
            timestamp = path.stat().st_mtime_ns
            path.write_bytes(b"changed input\n")
            os.utime(path, ns=(timestamp, timestamp))
            self.assertNotEqual(BUNDLE.source_fingerprint(self.repository), before)
            path.write_bytes(saved)
            self.assertEqual(BUNDLE.source_fingerprint(self.repository), before)
        alias = self.repository / "src/alias.rs"
        alias.symlink_to(self.inputs[0])
        with self.assertRaises(BUNDLE.Failure) as error:
            BUNDLE.source_fingerprint(self.repository)
        self.assertEqual(error.exception.code, "source_input_not_regular")

    def test_source_edits_during_build_refuse_without_erasing_edits(self):
        """A source edit between component recipes prevents mixed-source publication while retaining the caller's bytes."""
        original = BUNDLE.capture
        for index, path in enumerate(self.inputs):
            saved = path.read_bytes()
            edited = saved + b"intervening caller edit\n"
            destination = self.root / ("source-changed-" + str(index))
            def changed(arguments, *args, **kwargs):
                """Change one actual build input after the first compiler invocation, before later components."""
                output = original(arguments, *args, **kwargs)
                if "build" in arguments:
                    path.write_bytes(edited)
                return output
            with mock.patch.object(BUNDLE, "capture", side_effect=changed), self.assertRaises(BUNDLE.Failure) as error:
                BUNDLE.build(self.repository, destination, cargo=self.cargo, probe_timeout=1)
            self.assertEqual(error.exception.code, "source_changed")
            self.assertEqual(path.read_bytes(), edited)
            self.assert_no_publication(destination)
            path.write_bytes(saved)

    def test_source_inventory_and_cargo_control_changes_refuse_publication(self):
        """Added, removed and changed lock/control bytes all belong to the same final source fence."""
        original = BUNDLE.inspect_binary
        for operation in ("add", "remove", "lock", "manifest"):
            path = self.repository / {"add":"src/new.rs", "remove":"src/lib.rs",
                                      "lock":"Cargo.lock", "manifest":"Cargo.toml"}[operation]
            saved = path.read_bytes() if path.exists() else None
            def changed(*args, **kwargs):
                """Fault the inventory during the probe, after all selected compiler work has finished."""
                value = original(*args, **kwargs)
                if operation == "remove":
                    path.unlink()
                else:
                    path.write_bytes((saved or b"") + b"# caller change\n")
                return value
            destination = self.root / ("inventory-" + operation)
            with mock.patch.object(BUNDLE, "inspect_binary", side_effect=changed), self.assertRaises(BUNDLE.Failure) as error:
                BUNDLE.build(self.repository, destination, explicit=["eska-check"], cargo=self.cargo, probe_timeout=1)
            self.assertEqual(error.exception.code, "source_changed")
            self.assert_no_publication(destination)
            if saved is None:
                path.unlink()
            else:
                path.write_bytes(saved)

    def test_cli_receipt_is_locale_independent(self):
        """Local shell locale cannot change machine fields, receipt bytes or the compatibility evidence."""
        outputs, receipts = [], []
        for locale in ("ru_RU.UTF-8", "en_US.UTF-8"):
            destination = self.root / locale
            language = "ru-RU" if locale.startswith("ru") else "en-US"
            command = [sys.executable, str(REPOSITORY / "tools/local-bundle.py"), "--lang", language, "build", str(destination),
                       "--repository", str(self.repository), "--components", "eska-check", "--cargo", str(self.cargo)]
            result = subprocess.run(command, capture_output=True, env={**os.environ, "LANG":locale, "LC_ALL":locale}, timeout=5)
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            self.assertEqual(result.stderr, b"")
            outputs.append(json.loads(result.stdout))
            receipts.append((destination / BUNDLE.RECEIPT).read_bytes())
        self.assertEqual(outputs[0], outputs[1])
        self.assertEqual(receipts[0], receipts[1])

    def test_actual_help_has_both_languages_and_environment_default(self):
        """Help precedes required-subcommand parsing and uses explicit language before the scoped environment default."""
        command = [sys.executable, str(REPOSITORY / "tools/local-bundle.py")]
        for language, marker in (("ru-RU", "Локальный набор"), ("en-US", "Local ESKA")):
            for options in (["--lang", language, "--help"], ["build", "--help", "--lang=" + language]):
                result = subprocess.run([*command, *options], capture_output=True,
                                        env={**os.environ, "ESKA_LANG":"unsupported"}, timeout=5)
                self.assertEqual(result.returncode, 0)
                self.assertEqual(result.stderr, b"")
                self.assertIn(marker, result.stdout.decode())
                for option in ("--components", "--from", "--probe-timeout", "--build-timeout"):
                    self.assertIn(option.encode(), result.stdout)
            result = subprocess.run([*command, "--help"], capture_output=True,
                                    env={**os.environ, "ESKA_LANG":language}, timeout=5)
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stderr, b"")
            self.assertEqual(result.stdout.decode(), BUNDLE.HELP[language])
        result = subprocess.run([*command, "--help"], capture_output=True,
                                env={key:value for key, value in os.environ.items() if key != "ESKA_LANG"}, timeout=5)
        self.assertEqual(result.stdout.decode(), BUNDLE.HELP["ru-RU"])

    def test_invalid_arguments_are_identical_json_in_both_languages(self):
        """Parsing failures contain stable machine codes and never argparse's untranslated human stderr."""
        command = [sys.executable, str(REPOSITORY / "tools/local-bundle.py")]
        for arguments in ([], ["unknown"], ["build"], ["verify", "x", "--probe-timeout", "text"],
                          ["build", "x", "--components", "unknown"], ["build", "x", "--unknown"],
                          ["--lang", "unsupported"], ["--lang"]):
            responses = []
            for language in ("ru-RU", "en-US"):
                result = subprocess.run([*command, "--lang", language, *arguments], capture_output=True, timeout=5)
                self.assertEqual(result.returncode, 1)
                self.assertEqual(result.stderr, b"")
                responses.append(result.stdout)
                self.assertEqual(json.loads(result.stdout)["error"]["code"], "arguments_invalid")
            self.assertEqual(responses[0], responses[1])
        self.assertFalse((self.repository / "cargo-log.jsonl").exists())

    def test_existing_destination_and_broken_symlink_refuse_before_cargo(self):
        """Existing directories, files and even broken links retain their bytes and identities."""
        for kind in ("directory", "file", "symlink"):
            destination = self.root / kind
            if kind == "directory":
                destination.mkdir()
                (destination / "keep").write_bytes(b"owned by user")
            elif kind == "file":
                destination.write_bytes(b"owned by user")
            else:
                destination.symlink_to(self.root / "absent")
            with self.assertRaises(BUNDLE.Failure) as error:
                BUNDLE.build(self.repository, destination, cargo=self.cargo)
            self.assertEqual(error.exception.code, "destination_exists")
            self.assertTrue(os.path.lexists(destination))
        self.assertFalse((self.repository / "cargo-log.jsonl").exists())
        self.assertEqual((self.root / "directory/keep").read_bytes(), b"owned by user")
        self.assertEqual((self.root / "file").read_bytes(), b"owned by user")

    def test_raced_in_empty_or_populated_destination_is_never_replaced(self):
        """No-replace rename protects the raced-in directory itself as well as any caller's file bytes."""
        original = BUNDLE.publish
        for populated in (False, True):
            destination = self.root / ("raced-full" if populated else "raced-empty")
            identity = []
            def raced(source, target, function):
                """Simulate another actor claiming the name after the final build and probe."""
                target.mkdir()
                identity.append(target.stat().st_ino)
                if populated:
                    (target / "keep").write_bytes(b"intervening user bytes")
                original(source, target, function)
            with mock.patch.object(BUNDLE, "publish", side_effect=raced), self.assertRaises(BUNDLE.Failure) as error:
                BUNDLE.build(self.repository, destination, explicit=["eska-check"], cargo=self.cargo, probe_timeout=1)
            self.assertEqual(error.exception.code, "destination_exists")
            self.assertEqual(destination.stat().st_ino, identity[0])
            self.assertFalse((destination / BUNDLE.RECEIPT).exists())
            if populated:
                self.assertEqual((destination / "keep").read_bytes(), b"intervening user bytes")
        self.assertEqual(list(self.root.glob(".eska-local-bundle-*")), [])

    def test_failed_build_or_escape_artifact_never_publishes(self):
        """Compiler failure and forged artifact paths cannot copy or delete bytes outside private staging."""
        for fault, expected in (({"fail_build":True}, "process_failed"), ({"outside":True}, "artifact_outside_staging")):
            self.mode(**fault)
            destination = self.root / expected
            with self.assertRaises(BUNDLE.Failure) as error:
                BUNDLE.build(self.repository, destination, explicit=["eska-check"], cargo=self.cargo, probe_timeout=1)
            self.assertEqual(error.exception.code, expected)
            self.assert_no_publication(destination)
        self.assertEqual((self.repository / "outside-tool").read_text(), "unrelated bytes")

    def test_version_core_protocol_and_malformed_capabilities_refuse_publication(self):
        """Observed core versions and typed protocol/array shapes precede publishing any successful receipt."""
        for fault in ("wrong_version", "wrong_core", "wrong_protocol", "wrong_array", "duplicate_json", "self_change"):
            self.mode(**{fault: "eska-check"})
            destination = self.root / fault
            with self.assertRaises(BUNDLE.Failure):
                BUNDLE.build(self.repository, destination, explicit=["eska-check"], cargo=self.cargo, probe_timeout=1)
            self.assert_no_publication(destination)

    def test_analysis_commands_and_checker_report_schema_are_required_before_publication(self):
        """Advertised profiles alone cannot approve a missing entry point or a boolean/unknown report schema."""
        for component in ("eska-check", "eska-analyzer"):
            faults = ["missing_commands", "wrong_commands", "malformed_commands"]
            if component == "eska-check":
                faults += ["missing_report_schema", "wrong_report_schema", "boolean_report_schema"]
            for fault in faults:
                self.mode(**{fault: component})
                destination = self.root / (component + "-" + fault)
                with self.assertRaises(BUNDLE.Failure) as error:
                    BUNDLE.build(self.repository, destination, explicit=[component], cargo=self.cargo, probe_timeout=1)
                self.assertEqual(error.exception.code, "probe_incompatible")
                self.assert_no_publication(destination)

    def test_bounded_stdout_stderr_and_timeout_do_not_publish(self):
        """A blocked or noisy probe has bounded memory and elapsed time and its child is reaped."""
        for fault, expected in (("probe_sleep", "process_timeout"), ("probe_oversized", "process_output_oversized"), ("stderr_oversized", "process_output_oversized")):
            self.mode(**{fault: "eska-check"})
            destination = self.root / fault
            started = time.monotonic()
            with self.assertRaises(BUNDLE.Failure) as error:
                BUNDLE.build(self.repository, destination, explicit=["eska-check"], cargo=self.cargo, probe_timeout=0.1)
            self.assertEqual(error.exception.code, expected)
            self.assertLess(time.monotonic() - started, 3)
            self.assert_no_publication(destination)

    def test_verify_rejects_mutated_hash_or_receipt_escape_without_writes(self):
        """Verification trusts neither a replaced binary nor a receipt requesting files outside its fixed names."""
        root, receipt, _ = self.build(components=["eska-check"])
        binary = root / "bin/eska-check"
        saved = binary.read_bytes()
        binary.write_bytes(saved + b"\n# changed\n")
        with self.assertRaises(BUNDLE.Failure) as error:
            BUNDLE.verify(root)
        self.assertEqual(error.exception.code, "hash_mismatch")
        self.assertEqual(binary.read_bytes(), saved + b"\n# changed\n")
        binary.write_bytes(saved)
        receipt["files"][0]["path"] = "../outside-tool"
        raw = json.dumps(receipt).encode()
        (root / BUNDLE.RECEIPT).write_bytes(raw)
        with self.assertRaises(BUNDLE.Failure) as error:
            BUNDLE.verify(root)
        self.assertEqual(error.exception.code, "receipt_invalid")
        self.assertEqual((root / BUNDLE.RECEIPT).read_bytes(), raw)

    def test_invalid_selection_and_receipt_identity_refuse(self):
        """Invalid sets and malformed source identities cannot silently widen or inherit a previous bundle."""
        for components in ([], ["eska-check", "eska-check"], ["unknown"], [1], [{"eska-check":True}]):
            with self.assertRaises(BUNDLE.Failure) as error:
                BUNDLE.selected_components(components)
            self.assertEqual(error.exception.code, "components_invalid")
        root, receipt, _ = self.build(components=["eska-check"])
        for coordinate, value in (("profile", "unknown"), ("previous_receipt_sha256", "short"),
                                   ("source.fingerprint.sha256", "short"), ("source.fingerprint.file_count", True),
                                   ("source.controls.root_lock_sha256", 1), ("source.versions.source_core", "0.13.0")):
            changed = json.loads(json.dumps(receipt))
            owner = changed
            fields = coordinate.split(".")
            for field in fields[:-1]:
                owner = owner[field]
            owner[fields[-1]] = value
            raw = json.dumps(changed).encode()
            (root / BUNDLE.RECEIPT).write_bytes(raw)
            with self.assertRaises(BUNDLE.Failure) as error:
                BUNDLE.verify(root)
            self.assertEqual(error.exception.code, "receipt_invalid")
            self.assertEqual((root / BUNDLE.RECEIPT).read_bytes(), raw)

    def test_unsupported_publisher_refuses_before_build(self):
        """An unimplemented no-replace ABI cannot silently choose an overwrite-capable rename."""
        with mock.patch.object(BUNDLE.sys, "platform", "darwin"), self.assertRaises(BUNDLE.Failure) as error:
            self.build(components=["eska-check"])
        self.assertEqual(error.exception.code, "atomic_publication_unsupported")
        self.assertFalse((self.repository / "cargo-log.jsonl").exists())

    def test_interrupted_compiler_group_has_no_partial_bundle(self):
        """A real SIGINT interrupts the parent, kills its private Cargo group and leaves no published name."""
        self.mode(sleep=True)
        destination = self.root / "interrupted"
        command = [sys.executable, str(REPOSITORY / "tools/local-bundle.py"), "build", str(destination),
                   "--repository", str(self.repository), "--components", "eska-check", "--cargo", str(self.cargo)]
        process = subprocess.Popen(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 5
            marker = self.repository / "running.json"
            while not marker.exists() and time.monotonic() < deadline:
                time.sleep(0.01)
            self.assertTrue(marker.exists())
            process.send_signal(signal.SIGINT)
            output, errors = process.communicate(timeout=5)
            self.assertEqual(process.returncode, 130, errors.decode())
            self.assertEqual(json.loads(output)["error"]["code"], "interrupted")
            self.assert_no_publication(destination)
            parent, child = json.loads(marker.read_text())
            with self.assertRaises(ProcessLookupError):
                os.kill(parent, 0)
            status = Path("/proc") / str(child) / "status"
            deadline = time.monotonic() + 1
            while status.exists() and "State:\tZ" not in status.read_text() and time.monotonic() < deadline:
                time.sleep(0.01)
            if status.exists():
                self.assertIn("State:\tZ", status.read_text())
        finally:
            if process.poll() is None:
                process.kill()
                process.wait(timeout=5)


if __name__ == "__main__":
    unittest.main()
