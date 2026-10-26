#!/usr/bin/env python3
"""Compare query revisions with the real T114 DCS/virtual-table runner in a fresh owned infobase."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time


def main():
    """Reuse only a checked fixture CF; all database writes and results belong to the new stand."""
    repo = Path(__file__).resolve().parents[1]
    playground = (repo.parent / "eska-playground").resolve()
    artifact = Path(sys.argv[1]).resolve()
    assert artifact.is_relative_to(playground) and artifact.name == "matrix.cf"
    fixture = artifact.parents[2]
    assert json.loads((fixture / "owner.json").read_text())["task"] == "T114"
    stand = Path(tempfile.mkdtemp(prefix="t122-native-queries-", dir=playground))
    (stand / "owner.json").write_text(json.dumps({"task": "T122", "repository": str(repo)}) + "\n")
    payload = json.load(sys.stdin)
    (stand / "input.json").write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n")
    prefix = ["distrobox", "enter", sys.argv[3], "--"]
    platform = f"/opt/1cv8/x86_64/{sys.argv[2]}"
    report = {"complete": False, "platform": sys.argv[2], "stand": str(stand), "steps": []}
    print(stand, flush=True)

    def run(name, arguments, timeout):
        """Retain bounded process evidence independently of assertions about evaluated query values."""
        start = time.monotonic()
        result = subprocess.run(arguments, capture_output=True, timeout=timeout)
        (stand / f"{name}.stdout").write_bytes(result.stdout)
        (stand / f"{name}.stderr").write_bytes(result.stderr)
        report["steps"].append({"name": name, "exitCode": result.returncode, "seconds": round(time.monotonic() - start, 3)})
        assert result.returncode == 0, f"{name} failed: {stand}"

    try:
        run("create", prefix + [f"{platform}/ibcmd", "infobase", "create", f"--db-path={stand / 'ib'}", f"--data={stand / 'data'}", f"--load={artifact}", "--apply", "--force"], 120)
        run("client", prefix + ["timeout", "--signal=TERM", "--kill-after=5", "60s", f"{platform}/1cv8", "ENTERPRISE", "/F", str(stand / "ib"), "/DisableStartupDialogs", "/DisableStartupMessages", "/C", str(stand), "/Out", str(stand / "client.log")], 80)
        rows = json.loads((stand / "result.json").read_text(encoding="utf-8-sig"))
        report.update(caseCount=len(rows), results=rows)
        assert len(rows) == len(payload["cases"]) and all(row["passed"] and row["executed"] for row in rows), rows
        report["complete"] = True
    finally:
        (stand / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"complete": True, "caseCount": len(rows), "stand": str(stand)}))


if __name__ == "__main__":
    main()
