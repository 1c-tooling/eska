#!/usr/bin/env python3
"""Measure installed formatter entry points using owned source copies and exact source hashes."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import tempfile
import time


def sha(path):
    """Read actual bytes, including BOM and physical line endings."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def command(arguments, cwd, environment):
    """Every observed process has a bounded lifetime and retains its complete machine result."""
    started = time.perf_counter()
    result = subprocess.run(list(map(str, arguments)), cwd=cwd, env=environment, capture_output=True, timeout=60)
    elapsed = (time.perf_counter() - started) * 1000
    document = json.loads(result.stdout)
    assert result.returncode in (0, 1) and not result.stderr and document["error"] is None, (result.returncode, document, result.stderr)
    return elapsed, document


def main():
    """A fresh stand lives next to ESKA; original sources are read and copied, never formatted in place."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--installation", required=True, type=Path)
    parser.add_argument("--corpus", required=True, type=Path)
    parser.add_argument("--real-project", required=True, type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    playground = (repo.parent / "eska-playground").resolve()
    assert Path(os.environ["ESKA_TEST_ROOT"]).resolve() == playground
    stand = Path(tempfile.mkdtemp(prefix="t144-fmt-performance-", dir=playground))
    (stand / "owner.json").write_text(json.dumps({"task": "T144", "kind": "formatter-performance"}))
    print(stand, flush=True)
    binaries = args.installation.resolve(strict=True) / "bin"
    formatter, facade = binaries / "eska-fmt", binaries / "eska"
    environment = {**os.environ, "PATH": "", "ESKA_CONFIG_DIR": str(stand / "machine"), "ESKA_BSL_ANALYZER_BINARY": str(stand / "missing-analyzer")}
    for tool in ["PROJECT", "BUILD", "CHECK", "VCS"]:
        environment[f"ESKA_{tool}_BINARY"] = str(stand / ("missing-" + tool.lower()))
    inputs = stand / "inputs"
    inputs.mkdir()
    small = inputs / "small.bsl"
    small.write_bytes('\ufeffПроцедура Тест()\r\nЗначение=1;\r\nЕсли Значение=1 Тогда Сообщить("😀"); КонецЕсли;\r\nКонецПроцедуры\r\n'.encode())
    corpus = json.loads(args.corpus.read_text())
    largest = max(corpus["samples"], key=lambda item: len(item["source"].encode()))
    large = inputs / "large-real.bsl"
    large.write_bytes(largest["source"].encode())
    package = inputs / "package.sdbl"
    package.write_text(';\n'.join(f'выбрать {index} как Значение, "😀" как Имя поместить ВТ{index};\nвыбрать Значение,Имя из ВТ{index} где Значение>{index-1} упорядочить по Значение;\nDROP ВТ{index}' for index in range(20)) + ';\n')
    # Keep a complete real extension source tree; the benchmark selects one known module instead of inventing metadata.
    source_project = args.real_project.resolve(strict=True)
    real = stand / "real-project-copy"
    real.mkdir()
    shutil.copytree(source_project / "src", real / "src", symlinks=True)
    (real / "eska.toml").write_text('[project]\ntype="extension"\nsource="src"\n')
    candidates = [item for item in corpus["samples"] if item["name"].startswith("src/" + source_project.name + "/src/CommonModules/")]
    assert candidates, source_project
    relative = Path(candidates[0]["name"].split("/src/", 1)[1])
    original = source_project / "src" / relative
    copied = real / "src" / relative
    assert sha(original) == sha(copied)
    selected_hash = sha(original)
    original_tree = {str(path.relative_to(source_project / "src")): sha(path) for path in (source_project / "src").rglob("*") if path.is_file()}
    cases = [("small-bsl", inputs, small.name), ("large-real-bsl", inputs, large.name),
             ("query-package-60-statements", inputs, package.name), ("real-extension-copy-module", real, str(Path("src") / relative)), ("real-extension-copy-project", real, None)]
    records = []
    for label, cwd, scope in cases:
        files = [cwd / scope] if scope else sorted((real / "src").rglob("*.bsl"))
        before = {str(path.relative_to(cwd)): sha(path) for path in files}
        selection = [scope] if scope else []
        runs = []
        first_result = None
        for iteration in range(8):
            elapsed, result = command([formatter, "--project-dir", cwd, *selection, "--check", "--format", "json"], stand, environment)
            if first_result is None:
                first_result = result
            assert result == first_result
            runs.append(elapsed)
        elapsed, wrapper = command([facade, "--project-dir", cwd, "fmt", *selection, "--check", "--format", "json"], stand, environment)
        assert wrapper == first_result
        assert {str(path.relative_to(cwd)): sha(path) for path in files} == before
        records.append({"case": label, "source_bytes": sum(path.stat().st_size for path in files), "files": len(files), "source_sha256": before,
                        "first_process_ms": runs[0], "repeat_ms": runs[1:], "repeat_median_ms": statistics.median(runs[1:]),
                        "repeat_max_ms": max(runs[1:]), "facade_ms": elapsed, "parity": True, "source_unchanged": True})
    applied = subprocess.run([str(formatter), "--project-dir", str(real), "--format", "json"], env=environment, capture_output=True, timeout=60)
    assert applied.returncode == 0 and json.loads(applied.stdout)["error"] is None
    _, repeated = command([formatter, "--project-dir", real, "--check", "--format", "json"], stand, environment)
    assert all(not file["changed"] for file in repeated["files"])
    for relative_path, before in original_tree.items():
        if not relative_path.endswith(".bsl"):
            assert sha(real / "src" / relative_path) == before
    assert {str(path.relative_to(source_project / "src")): sha(path) for path in (source_project / "src").rglob("*") if path.is_file()} == original_tree
    assert sha(original) == selected_hash
    report = {"task": "T144", "complete": True, "system": platform.platform(), "machine": platform.machine(),
              "formatter_sha256": sha(formatter), "facade_sha256": sha(facade), "installed_root": str(args.installation.resolve()),
              "records": records, "real_project": {"original": str(source_project), "copy": str(real), "scope": str(relative),
              "copied_files": sum(path.is_file() for path in (real / "src").rglob("*")), "original_selected_unchanged": True, "original_full_tree_unchanged": True, "applied_copy_and_repeat_check_clean": True, "metadata_unchanged": True},
              "notes": ["First process after installation; OS caches were not flushed.", "Every repeat starts a new CLI process.",
                        "The real extension tree is copied completely; both a selected module and the whole copied extension are measured.",
                        "Other executables and the analyzer are absent from PATH and explicitly disabled.",
                        "Full buhmfo bulk formatting is not claimed: source/plan limits require explicit scope or exclusions."]}
    (stand / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"stand": str(stand), "complete": True, "records": records}, ensure_ascii=False), flush=True)


if __name__ == "__main__":
    main()
