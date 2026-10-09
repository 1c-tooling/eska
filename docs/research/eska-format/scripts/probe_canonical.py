"""Apply the proposed canonical-input patch only to an owned, pinned reference copy."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile

EXPECTED_HEAD = "9021ba07471eee8a2614bed2105a2c8699f60780"
SALSA_REVISION = "5d2686f1f94dc3d618569652d5576ce997491b3b"


def run(argv, cwd, log, env=None):
    """Capture complete evidence and stop on the first failed gate."""
    with log.open("wb") as stream:
        process = subprocess.run(argv, cwd=cwd, env=env, stdout=stream, stderr=subprocess.STDOUT)
    if process.returncode:
        raise RuntimeError(f"exit {process.returncode}; see {log}")


def main():
    """Keep source checkouts unchanged and test the patched library and its real consumer."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--analyzer", required=True, type=Path)
    parser.add_argument("--family-inputs", action="store_true", help="Test the generalized object/register/module/defined-type input")
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[4]
    playground = (repo.parent / "eska-playground").resolve(strict=True)
    reference = args.analyzer.resolve(strict=True)
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=reference, text=True).strip()
    if head != EXPECTED_HEAD or subprocess.check_output(["git", "status", "--porcelain"], cwd=reference):
        raise RuntimeError("reference must be clean at the expected commit")
    output = Path(tempfile.mkdtemp(prefix="ef18-canonical-", dir=playground))
    print(output, flush=True)
    archive = output / "reference.tar"
    with archive.open("wb") as stream:
        subprocess.run(["git", "archive", "--format=tar", head], cwd=reference, stdout=stream, check=True)
    copy = output / "reference"
    copy.mkdir()
    with tarfile.open(archive) as stream:
        stream.extractall(copy, filter="data")
    archive.unlink()
    patch = Path(__file__).with_name("canonical-family-input.patch" if args.family_inputs else "canonical-input.patch")
    probe = Path(__file__).with_name("canonical_family_probe.rs" if args.family_inputs else "canonical_probe.rs")
    run(["git", "apply", "--check", str(patch)], copy, output / "patch-check.log")
    run(["git", "apply", str(patch)], copy, output / "patch-apply.log")
    consumer = output / "consumer"
    (consumer / "src").mkdir(parents=True)
    shutil.copyfile(probe, consumer / "src/main.rs")
    manifest = '[package]\nname = "eska-canonical-probe"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n[workspace]\n\n[dependencies]\n'
    manifest += f'eska = {{ path = {json.dumps(str(repo))}, default-features = false }}\n'
    for name in ["base-db", "bsl-metadata", "ide-db", "ide-host-core", "vfs"]:
        manifest += f'{name} = {{ path = {json.dumps(str(copy / "crates" / name))} }}\n'
    manifest += 'serde_json = "1.0"\nsalsa = "=0.28.2"\n\n[profile.dev]\ndebug = 0\nincremental = false\n\n[profile.release]\ndebug = 0\nlto = false\ncodegen-units = 16\n\n[patch.crates-io]\n'
    for name in ["salsa", "salsa-macros"]:
        manifest += f'{name} = {{ git = "https://github.com/itrous/salsa.git", rev = "{SALSA_REVISION}" }}\n'
    (consumer / "Cargo.toml").write_text(manifest)
    shutil.copyfile(Path(__file__).with_name("canonical-probe.lock"), consumer / "Cargo.lock")
    env = os.environ.copy()
    env["CARGO_TARGET_DIR"] = str(output / "target")
    env["CARGO_BUILD_JOBS"] = "4"
    env["CARGO_PROFILE_TEST_DEBUG"] = "0"
    env["CARGO_PROFILE_DEV_DEBUG"] = "0"
    run(["cargo", "build", "--release", "--locked"], consumer, output / "build.log", env)
    run(["cargo", "clippy", "--release", "--locked", "--", "-D", "warnings"], consumer, output / "clippy.log", env)
    binary = output / "target/release/eska-canonical-probe"
    runtime = env.copy()
    runtime["PATH"] = str(output / "no-programs")
    runtime["BSL_SALSA_EVENTS"] = "1"
    run([str(binary), str(output / "demo")], output, output / "demo.json", runtime)
    run(["cargo", "clippy", "--locked", "-p", "ide-db", "-p", "ide-host-core", "--all-targets", "--", "-D", "warnings"], copy, output / "kernel-clippy.log", env)
    run(["cargo", "test", "--locked", "-p", "ide-db", "--lib"], copy, output / "kernel-tests.log", env)
    run(["cargo", "test", "--locked", "-p", "ide-host-core", "--lib"], copy, output / "host-tests.log", env)
    summary = {"reference_head": head, "salsa_revision": SALSA_REVISION,
               "patch_sha256": hashlib.sha256(patch.read_bytes()).hexdigest(),
               "probe_sha256": hashlib.sha256(probe.read_bytes()).hexdigest(),
               "input_mode": "families" if args.family_inputs else "mdo",
               "demo": json.loads((output / "demo.json").read_text()),
               "gates": ["build", "consumer-clippy", "kernel-clippy-all-targets", "kernel-unit", "host-unit"]}
    (output / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(summary, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()
