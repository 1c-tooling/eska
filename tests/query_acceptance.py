#!/usr/bin/env python3
"""Measure an installed query adapter in an owned Linux/VSCodium acceptance stand."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import time
import zipfile

PAGE = os.sysconf("SC_PAGE_SIZE")
HZ = os.sysconf("SC_CLK_TCK")


def digest(path):
    """Identify frozen inputs without embedding a checkout path in the runtime."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def descendants(parent, known, include_pss):
    """Retain process start identities so a reused PID never becomes part of the measured window."""
    table = {}
    for directory in Path("/proc").iterdir():
        if not directory.name.isdigit():
            continue
        try:
            fields = (directory / "stat").read_text().rsplit(")", 1)[1].split()
            table[int(directory.name)] = {"ppid": int(fields[1]), "ticks": int(fields[11]) + int(fields[12]),
                                          "start": fields[19], "rss": int(fields[21]) * PAGE, "state": fields[0]}
        except (OSError, ValueError, IndexError):
            continue
    owned = {pid for pid, start in known.items() if table.get(pid, {}).get("start") == start}
    if parent in table and (parent not in known or table[parent]["start"] == known[parent]):
        owned.add(parent)
    while True:
        more = owned | {pid for pid, row in table.items() if row["ppid"] in owned}
        if more == owned:
            break
        owned = more
    result = {}
    for pid in owned & table.keys():
        row = table[pid]
        known[pid] = row["start"]
        try:
            row["command"] = Path(f"/proc/{pid}/comm").read_text().strip()
            io = dict(line.split(": ", 1) for line in Path(f"/proc/{pid}/io").read_text().splitlines())
            for key in ["read_bytes", "write_bytes", "rchar", "wchar"]:
                row[key] = int(io[key])
            if include_pss:
                smaps = dict(line.split(":", 1) for line in Path(f"/proc/{pid}/smaps_rollup").read_text().splitlines() if ":" in line)
                row["pss"] = int(smaps["Pss"].split()[0]) * 1024
        except (OSError, KeyError, ValueError):
            pass
        result[str(pid)] = row
    return result


def run(args):
    """Install the exact package, reuse a cohort profile on repeat, and sample only this launch's descendants."""
    stand, input_file = args.stand.resolve(), args.input.resolve()
    repo = Path(__file__).resolve().parents[1]
    playground = (repo / "../eska-playground").resolve()
    assert stand.is_relative_to(playground) and json.loads((stand / "owner.json").read_text())["task"] == "T105"
    assert input_file.is_relative_to(stand) and args.deadline > 0
    inputs = json.loads(input_file.read_text())
    assert inputs["mode"] in ["ordinary", "query"]
    project = Path(inputs["project"]).resolve()
    assert project.is_relative_to(stand) and Path(inputs["module"]).resolve().is_relative_to(project)
    name = f"{project.name}-{inputs['mode']}-{args.round}-{args.locale}"
    output = stand / "runs" / f"{name}-{args.phase}"
    output.mkdir(parents=True)
    profile_root = stand / "profiles" / name
    profile, extensions = profile_root / "profile", profile_root / "extensions"
    if args.phase == "cold":
        assert not profile_root.exists(), "A cold profile cannot be reused"
        # These explicit roots are derived when preparing the fixture, never from a per-edit source walk.
        for value in inputs.get("cacheRoots", []):
            cache = Path(value).resolve()
            assert cache.is_relative_to(project) and cache.name == ".eska"
            if cache.exists():
                shutil.rmtree(cache)
    extensions.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(args.vsix) as archive:
        manifest = json.loads(archive.read("extension/package.json"))
    installed = extensions / f"{manifest['publisher']}.{manifest['name']}-{manifest['version']}"
    if not installed.exists():
        result = subprocess.run([args.editor, f"--user-data-dir={profile}", f"--extensions-dir={extensions}",
                                 "--install-extension", str(args.vsix.resolve()), "--force"], capture_output=True, timeout=60)
        (output / "install.stdout").write_bytes(result.stdout)
        (output / "install.stderr").write_bytes(result.stderr)
        result.check_returncode()
    assert (installed / "out/extension.js").is_file()
    assert digest(installed / "out/extension.js") == args.bundle_sha256
    probe = output / "probe.cjs"
    shutil.copyfile(args.probe.resolve(), probe)
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(("ESKA_HOST_", "ESKA_TEST_", "ESKA_BSL_", "ESKA_PERF_", "T89_", "T90_", "T106_"))}
    environment.update(ESKA_QUERY_PERF_INPUT=str(input_file), ESKA_QUERY_PERF_LOCALE=args.locale,
                       ESKA_HOST_FIXTURE=json.dumps({"root": str(output)}))
    command = [args.editor, "--wait", "--new-window", "--verbose", f"--locale={args.locale}",
               f"--extensionDevelopmentPath={installed}", f"--extensionTestsPath={probe}",
               f"--user-data-dir={profile}", f"--extensions-dir={extensions}",
               "--disable-workspace-trust", "--disable-gpu", "--disable-extensions", str(project)]
    samples, known, maxima = [], {}, {}
    started, launch_unix_ms = time.monotonic(), time.time_ns() / 1e6
    with (output / "editor.log").open("w") as log:
        child = subprocess.Popen(command, env=environment, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        while child.poll() is None:
            rows = descendants(child.pid, known, len(samples) % 5 == 0)
            samples.append({"unixMs": time.time_ns() / 1e6, "seconds": time.monotonic() - started, "processes": rows})
            for pid, row in rows.items():
                values = maxima.setdefault(pid, {})
                for key in ["ticks", "read_bytes", "write_bytes", "rchar", "wchar"]:
                    values[key] = max(values.get(key, 0), row.get(key, 0))
            if time.monotonic() - started > args.deadline:
                os.killpg(child.pid, signal.SIGTERM)
                child.wait(timeout=15)
                raise RuntimeError(f"Editor deadline; inspect {output}")
            time.sleep(0.1)
    survivors = {pid: row for pid, row in descendants(child.pid, known, True).items() if row["state"] != "Z" and int(pid) != child.pid}
    (output / "resources.json").write_text(json.dumps(samples) + "\n")
    result = json.loads((output / "host-result.json").read_text()) if (output / "host-result.json").exists() else {"passed": False}
    resource_summary = {
        "peakRssBytes": max((sum(p["rss"] for p in row["processes"].values()) for row in samples), default=0),
        "peakPssBytes": max((sum(p.get("pss", 0) for p in row["processes"].values()) for row in samples), default=0),
        "cpuSeconds": sum(row["ticks"] for row in maxima.values()) / HZ,
        **{key: sum(row[key] for row in maxima.values()) for key in ["read_bytes", "write_bytes", "rchar", "wchar"]},
        "ownedSurvivors": survivors}
    idle = [row for row in samples if result.get("endStartedUnixMs", float("inf")) + 1000 < row["unixMs"]
            <= result.get("finalMemory", {}).get("unixMs", 0)]
    resource_summary["remainingRssBytes"] = sum(p["rss"] for p in idle[-1]["processes"].values()) if idle else None
    pss = [row for row in idle if any("pss" in p for p in row["processes"].values())]
    resource_summary["remainingPssBytes"] = sum(p.get("pss", 0) for p in pss[-1]["processes"].values()) if pss else None
    summary = {"passed": child.returncode == 0 and result.get("passed") is True and result.get("locale") == args.locale and not survivors,
               "phase": args.phase, "round": args.round, "locale": args.locale, "fixture": project.name,
               "mode": inputs["mode"], "elapsedSeconds": time.monotonic() - started,
               "launchUnixMs": launch_unix_ms, "exit": child.returncode, "resources": resource_summary,
               "vsixSha256": digest(args.vsix), "runtimeBundleSha256": args.bundle_sha256,
               "probeSha256": digest(probe), "result": result}
    (output / "summary.json").write_text(json.dumps(summary, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({key: summary[key] for key in ["passed", "phase", "round", "locale", "fixture", "mode", "elapsedSeconds"]}), flush=True)
    if not summary["passed"]:
        raise RuntimeError(f"Acceptance failed: {result.get('error', 'missing result')}; inspect {output}")


def main():
    """Require explicit frozen artifacts and an owned stand for every measurement."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--stand", type=Path, required=True)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--vsix", type=Path, required=True)
    parser.add_argument("--probe", type=Path, required=True)
    parser.add_argument("--bundle-sha256", required=True)
    parser.add_argument("--phase", choices=["cold", "repeat"], required=True)
    parser.add_argument("--round", type=int, required=True)
    parser.add_argument("--locale", choices=["en"], default="en")
    parser.add_argument("--deadline", type=int, default=360)
    parser.add_argument("--editor", default="codium")
    run(parser.parse_args())


if __name__ == "__main__":
    main()
