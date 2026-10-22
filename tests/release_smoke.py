#!/usr/bin/env python3
"""Install real ZIPs into an isolated prefix and exercise the installed metadata server."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from native_components import metadata_session


def main():
    """Use only a new owned sibling playground directory on the native runner."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle", type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    playground = Path(os.environ["ESKA_TEST_ROOT"]).resolve()
    if playground != (root.parent / "eska-playground").resolve():
        raise ValueError("ESKA_TEST_ROOT must be the sibling playground")
    index = json.loads((args.bundle / "release.json").read_bytes())
    with tempfile.TemporaryDirectory(prefix="release-smoke-", dir=playground) as temporary:
        stand = Path(temporary)
        prefix = stand / "installation"
        binaries = {}
        for tool, item in index["components"].items():
            command = [sys.executable, "-B", root / "tools/components.py", "install", "--root", prefix,
                       "--archive", (args.bundle / item["archive"]).resolve(), "--sha256", item["sha256"], "--format", "json"]
            for _ in range(2):
                subprocess.run(command, cwd=stand, check=True, capture_output=True, timeout=60)
            binary = prefix / "bin" / (tool + (".exe" if os.name == "nt" else ""))
            actual = json.loads(subprocess.check_output([binary, "__tool-contract"], cwd=stand, timeout=15))
            assert actual == item["contract"], actual
            binaries[tool] = binary
        environment = dict(os.environ)
        for tool, binary in binaries.items():
            if tool != "eska":
                environment[tool.upper().replace("-", "_") + "_BINARY"] = str(binary)
        evidence = []
        for locale in ("ru-RU", "en-US"):
            for tool in binaries.values():
                subprocess.run([tool, "--lang", locale, "--help"], cwd=stand, env=environment,
                               check=True, capture_output=True, timeout=15)
            metadata_session(binaries["eska-project"], locale, stand, environment, evidence)
        print(json.dumps({"passed": True, "version": index["version"], "target": index["target"], "components": list(binaries)}))


if __name__ == "__main__":
    main()
