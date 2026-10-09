"""Build a pinned, private analyzer consumer and retain reproducible evidence in playground."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

EXPECTED_HEAD = "9021ba07471eee8a2614bed2105a2c8699f60780"
SALSA_REVISION = "5d2686f1f94dc3d618569652d5576ce997491b3b"


def run(argv, cwd, log, env=None):
    """Save complete subprocess output without mutating either source checkout."""
    with log.open("wb") as stream:
        process = subprocess.run(argv, cwd=cwd, env=env, stdout=stream, stderr=subprocess.STDOUT)
    if process.returncode:
        raise RuntimeError(f"exit {process.returncode}; see {log}")


def main():
    """Claim a unique demo, validate the selected reference, then compile and query offline."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--analyzer", required=True, type=Path)
    parser.add_argument("--source", action="append", type=Path, default=[])
    parser.add_argument("--build-cache", type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[4]
    playground = (repo.parent / "eska-playground").resolve(strict=True)
    reference = args.analyzer.resolve(strict=True)
    head = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=reference, text=True).strip()
    if head != EXPECTED_HEAD:
        raise RuntimeError(f"reference changed: expected {EXPECTED_HEAD}, got {head}")
    if subprocess.check_output(["git", "status", "--porcelain"], cwd=reference):
        raise RuntimeError("reference has uncommitted changes")
    output = Path(tempfile.mkdtemp(prefix="ef17-analyzer-", dir=playground))
    print(output, flush=True)
    consumer = output / "consumer"
    (consumer / "src").mkdir(parents=True)
    shutil.copyfile(Path(__file__).with_name("analyzer_probe.rs"), consumer / "src/main.rs")
    dependencies = [f"eska = {{ path = {json.dumps(str(repo))}, default-features = false }}"]
    for name in ["base-db", "bsl-metadata", "ide-db", "ide-host-core", "vfs"]:
        dependencies.append(f"{name} = {{ path = {json.dumps(str(reference / 'crates' / name))} }}")
    dependencies.append('serde_json = "1.0"')
    # Workspace patches do not propagate from a dependency to its consumer.
    dependencies.append('salsa = "=0.28.2"')
    manifest = '[package]\nname = "eska-analysis-probe"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n[workspace]\n\n[dependencies]\n'
    manifest += "\n".join(dependencies) + '\n\n[profile.dev]\ndebug = 0\nincremental = false\n\n[profile.release]\ndebug = 0\nlto = false\ncodegen-units = 16\n'
    manifest += '\n[patch.crates-io]\n'
    for name in ["salsa", "salsa-macros"]:
        manifest += f'{name} = {{ git = "https://github.com/itrous/salsa.git", rev = "{SALSA_REVISION}" }}\n'
    (consumer / "Cargo.toml").write_text(manifest)
    lock = Path(__file__).with_name("analyzer-probe.lock")
    if lock.is_file():
        shutil.copyfile(lock, consumer / "Cargo.lock")
    env = os.environ.copy()
    target = output / "target" if args.build_cache is None else args.build_cache.resolve()
    if not target.is_relative_to(playground):
        raise RuntimeError("build cache must be inside the sibling playground")
    env["CARGO_TARGET_DIR"] = str(target)
    env["CARGO_BUILD_JOBS"] = "4"
    run(["cargo", "build", "--release", "--locked"], consumer, output / "build.log", env)
    run(["cargo", "clippy", "--release", "--locked", "--", "-D", "warnings"], consumer, output / "clippy.log", env)
    host = next(line.split(": ", 1)[1] for line in subprocess.check_output(["rustc", "-vV"], text=True).splitlines() if line.startswith("host: "))
    run(["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", host], consumer, output / "dependencies.json", env)
    native = output / "native"
    shutil.copytree(repo / "tests/fixtures/eska-format/designer", native)
    catalog = native / "Catalogs/Sample.xml"
    catalog.write_bytes(catalog.read_bytes().replace(b"<Name>Description</Name>", b"<Name>SourceField</Name>"))
    binary = target / "release/eska-analysis-probe"
    runtime = env.copy()
    runtime["PATH"] = str(output / "no-programs")
    run([str(binary), "demo", str(native), str(output / "source")], output, output / "demo.json", runtime)
    syntax_consumer = output / "syntax-consumer"
    (syntax_consumer / "src").mkdir(parents=True)
    shutil.copyfile(Path(__file__).with_name("syntax_probe.rs"), syntax_consumer / "src/main.rs")
    syntax_manifest = '[package]\nname = "eska-syntax-probe"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n[workspace]\n\n[dependencies]\n'
    for name in ["parser", "syntax"]:
        syntax_manifest += f'{name} = {{ path = {json.dumps(str(reference / "crates" / name))} }}\n'
    syntax_manifest += 'serde_json = "1.0"\n\n[profile.release]\ndebug = 0\nlto = false\ncodegen-units = 16\n'
    (syntax_consumer / "Cargo.toml").write_text(syntax_manifest)
    shutil.copyfile(Path(__file__).with_name("syntax-probe.lock"), syntax_consumer / "Cargo.lock")
    run(["cargo", "build", "--release", "--locked"], syntax_consumer, output / "syntax-build.log", env)
    run(["cargo", "clippy", "--release", "--locked", "--", "-D", "warnings"], syntax_consumer, output / "syntax-clippy.log", env)
    run(["cargo", "metadata", "--locked", "--format-version", "1", "--filter-platform", host], syntax_consumer, output / "syntax-dependencies.json", env)
    run([str(target / "release/eska-syntax-probe")], output, output / "syntax.json", runtime)
    results = []
    for index, source in enumerate(args.source):
        log = output / f"measure-{index}.json"
        run([str(binary), "measure", str(source.resolve(strict=True))], output, log, runtime)
        results.append({"source": str(source.resolve()), "result": json.loads(log.read_text())})
    summary = {"reference_head": head, "salsa_revision": SALSA_REVISION, "host": host, "rust": subprocess.check_output(["rustc", "--version"], text=True).strip(),
               "binary_bytes": binary.stat().st_size, "binary_sha256": hashlib.file_digest(binary.open("rb"), "sha256").hexdigest(),
               "demo": json.loads((output / "demo.json").read_text()), "syntax": json.loads((output / "syntax.json").read_text()), "measurements": results}
    (output / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(summary, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()
