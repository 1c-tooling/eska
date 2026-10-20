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


mode = os.environ.get("ESKA_COMPONENT_FIXTURE_MODE", "streams")
if mode == "signals":
    for number in [signal.SIGINT, signal.SIGTERM, signal.SIGHUP]:
        signal.signal(number, stop)
    Path(os.environ["ESKA_COMPONENT_FIXTURE_READY"]).write_text(str(os.getpid()))
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
