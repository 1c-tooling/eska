#!/usr/bin/python3
"""Immutable process fixture for wrapper stream, argument and signal contracts."""
import json
import os
from pathlib import Path
import signal
import sys
import time


def stop(number, _frame):
    """Acknowledge the exact forwarded signal and let the owner observe child reaping."""
    os.write(2, f"signal:{number}\n".encode())
    sys.exit(70 + number)


def publish_ready():
    """Publish the complete PID atomically before readers treat readiness as a signal barrier."""
    ready = Path(os.environ["ESKA_COMPONENT_FIXTURE_READY"])
    staged = ready.with_name(ready.name + ".tmp")
    staged.write_text(str(os.getpid()))
    staged.replace(ready)


mode = os.environ.get("ESKA_COMPONENT_FIXTURE_MODE", "streams")
if "--fixture-mode" in sys.argv:
    mode = sys.argv[sys.argv.index("--fixture-mode") + 1]
if "--fixture-ready" in sys.argv:
    os.environ["ESKA_COMPONENT_FIXTURE_READY"] = sys.argv[sys.argv.index("--fixture-ready") + 1]

if mode.startswith("bridge-"):
    operation_index = sys.argv.index("__bridge") + 1
    operation = sys.argv[operation_index].replace("-", "_")
    result = {"schema_version": 1, "operation": operation,
              "data": {"status": "available", "commit": "a" * 40, "dirty": False}, "error": None}
    if mode == "bridge-large-plan":
        result["data"] = {"padding": "x" * (1024 * 1024 + 1)}
    if mode.startswith("bridge-onboarding-"):
        if operation == "select_platform":
            result["data"] = {"platform_version": "8.3.27.2325"}
        else:
            destination = Path(sys.argv[operation_index + 2])
            destination.mkdir()
            destination.joinpath("Object.xml").write_bytes(Path(sys.argv[operation_index + 1]).read_bytes())
            result["data"] = {"identity": {"type": "processing", "name": "wrong", "uuid": "wrong"}}
    if mode == "bridge-import-cancel":
        counter = Path(os.environ["ESKA_COMPONENT_FIXTURE_COUNTER"])
        count = int(counter.read_text()) + 1 if counter.exists() else 1
        counter.write_text(str(count))
        result["data"] = {"local_changes": 0}
        if count > 1:
            mode = "bridge-timeout"
    if mode in ("bridge-timeout", "bridge-graceful-timeout"):
        if mode == "bridge-graceful-timeout":
            def graceful_stop(number, _frame):
                """An owner may exit zero after cleanup, while the initiating timeout still wins."""
                Path(os.environ["ESKA_COMPONENT_FIXTURE_READY"] + ".stopped").write_text(str(number))
                sys.exit(0)
            signal.signal(signal.SIGTERM, graceful_stop)
        publish_ready()
        parent = os.getppid()
        while os.getppid() == parent:
            time.sleep(0.05)
        sys.exit(1)
    if mode == "bridge-schema":
        result["schema_version"] = 99
    if mode == "bridge-data":
        result["data"]["dirty"] = None
    if mode == "bridge-operation":
        result["operation"] = "other"
    if mode == "bridge-oversized":
        os.write(1, b"x" * (1024 * 1024 + 1))
    else:
        print(json.dumps(result))
        if mode == "bridge-extra":
            print(json.dumps(result))
    sys.exit(4 if mode in ("bridge-failed", "bridge-onboarding-failed") and operation != "select_platform" else 0)
elif mode == "signals":
    for number in [signal.SIGINT, signal.SIGTERM, signal.SIGHUP]:
        signal.signal(number, stop)
    publish_ready()
    parent = os.getppid()
    while os.getppid() == parent:
        time.sleep(0.05)
else:
    Path(os.environ["ESKA_COMPONENT_FIXTURE_ARGS"]).write_text(json.dumps({
        "cwd": os.getcwd(),
        "args": [os.fsencode(value).hex() for value in sys.argv[1:]],
    }))
    data = sys.stdin.buffer.read()
    os.write(1, b"stdout:\x00" + data)
    os.write(2, b"stderr:\xff\x00")
    sys.exit(197)
