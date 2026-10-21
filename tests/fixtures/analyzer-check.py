#!/usr/bin/python3
"""Exercise incomplete external streams; real diagnostics use the pinned published application."""
import json
import os
from pathlib import Path
import sys
import time

root = Path.cwd()


def send(value):
    """Write independent complete JSONL frames."""
    print(json.dumps(value), flush=True)


command = sys.argv[1]
mode = (root / "stream-mode").read_text() if (root / "stream-mode").exists() else "ok"
if command == "--version":
    print("bsl-analyzer " + ("9.9.9" if mode == "wrong-version" else "0.3.2"))
elif command == "contract":
    send({"build_version": "0.3.2", "contract_version": "3.4", "cli": {"commands": [
        {"name": name, "args": [{"name": "config"}]} for name in ["check-config", "analyze", "lsp"]]}})
elif command == "check-config":
    pass
elif command == "analyze":
    (root / "analysis-started").write_text(str(os.getpid()))
    files = sorted(root.rglob("*.bsl"))
    if mode == "hang":
        time.sleep(30)
    if mode == "empty":
        sys.exit(0)
    send({"type": "start", "version": "9.9.9" if mode == "stream-version" else "0.3.2", "total_files": len(files)})
    total = 0
    for index, path in enumerate(files):
        diagnostics = []
        severities = {"warning": "Warning", "major": "Major", "critical": "Critical", "blocker": "Blocker"}
        if mode in severities and index == 0:
            diagnostics = [{"code": "FixtureRule", "severity": severities[mode], "message": "fixture diagnostic", "start_line": 0, "start_column": 0, "end_line": 0, "end_column": 1}]
            total += 1
        event = {"type": "file", "path": str(path), "diagnostics": diagnostics}
        if mode == "file-error":
            event["error"] = "file failed"
        if mode == "unexpected":
            event["path"] = str(root / "not-selected.bsl")
        if mode != "missing-file" or index != 0:
            send(event)
        if mode == "duplicate":
            send(event)
    if mode == "bad-json":
        print("bad JSON", flush=True)
    if mode == "truncated":
        sys.stdout.write('{"type":"done"')
    elif mode != "no-done":
        done = {"type": "done", "elapsed_secs": 0.1, "total_files": len(files), "total_diagnostics": total + (1 if mode == "bad-count" else 0), "failed_files": 1 if mode == "failed-files" else 0}
        if mode == "partial-baseline":
            done["baseline"] = {"state": "partial", "complete": False}
        send(done)
    if mode == "mutate":
        files[0].write_text("// changed during analysis\n")
    if mode == "add-file":
        (files[0].parent / "added.bsl").write_text("// new during analysis\n")
    if mode == "exit-after-done":
        print("upstream failed after done", file=sys.stderr)
        sys.exit(7)
else:
    sys.exit(4)
