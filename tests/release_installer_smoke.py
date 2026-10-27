#!/usr/bin/env python3
"""Exercise final native installers against local artifacts without touching user installation state."""

import argparse
from functools import partial
import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import subprocess
import tempfile
import threading

from native_components import metadata_session


def verify_assets(artifacts):
    """Detect upload omissions and altered bytes before invoking any native installer."""
    expected = {}
    for line in (artifacts / "SHA256SUMS").read_text(encoding="utf-8").splitlines():
        checksum, name = line.split("  ", 1)
        assert name not in expected, name
        expected[name] = checksum
    files = list(artifacts.iterdir())
    assert all(path.is_file() and not path.is_symlink() for path in files)
    actual = {path.name for path in files} - {"SHA256SUMS"}
    assert set(expected) == actual, {"missing": sorted(set(expected) - actual), "unexpected": sorted(actual - set(expected))}
    for name, checksum in expected.items():
        with (artifacts / name).open("rb") as stream:
            assert hashlib.file_digest(stream, "sha256").hexdigest() == checksum, name
    return len(expected)


def main():
    """Install twice with isolated binaries, receipt and cache, then probe every installed command."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("artifacts", type=Path)
    args = parser.parse_args()
    artifacts = args.artifacts.resolve()
    verified_assets = verify_assets(artifacts)
    repository = Path(__file__).resolve().parent.parent
    playground = Path(os.environ["ESKA_TEST_ROOT"]).resolve()
    assert playground == (repository.parent / "eska-playground").resolve()
    tools = {"eska", "eska-project", "eska-check", "eska-build", "eska-vcs", "eska-fmt"}
    suffix = ".exe" if os.name == "nt" else ""
    command = (["pwsh", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", artifacts / "eska-installer.ps1"]
               if os.name == "nt" else ["sh", artifacts / "eska-installer.sh"])
    with tempfile.TemporaryDirectory(prefix="release-installer-", dir=playground) as temporary:
        stand = Path(temporary)
        prefix = stand / "installation/bin"
        server = ThreadingHTTPServer(("127.0.0.1", 0), partial(SimpleHTTPRequestHandler, directory=str(artifacts)))
        worker = threading.Thread(target=server.serve_forever, daemon=True)
        worker.start()
        environment = {key: value for key, value in os.environ.items() if not key.startswith(("ESKA_", "INSTALLER_", "CARGO_DIST_"))}
        environment.update({"ESKA_INSTALL_DIR": str(prefix), "ESKA_NO_MODIFY_PATH": "1",
                            "ESKA_DOWNLOAD_URL": f"http://127.0.0.1:{server.server_port}",
                            "XDG_CONFIG_HOME": str(stand / "config"), "XDG_CACHE_HOME": str(stand / "cache")})
        try:
            for _ in range(2):
                result = subprocess.run(command, cwd=stand, env=environment,
                                        capture_output=True, text=True, timeout=120)
                assert result.returncode == 0, result
                assert "no checksums to verify" not in result.stdout + result.stderr
            receipt = json.loads((stand / "config/eska/eska-receipt.json").read_bytes())
            assert set(receipt["binaries"]) == {tool + suffix for tool in tools}, receipt
            assert Path(receipt["install_prefix"]) == prefix, receipt
            version = receipt["version"]
            for tool in sorted(tools):
                binary = prefix / (tool + suffix)
                contract = json.loads(subprocess.check_output([binary, "__tool-contract"], cwd=stand, env=environment, timeout=15))
                assert contract["tool"] == tool and contract["version"] == version, contract
                for locale in ("ru-RU", "en-US"):
                    subprocess.run([binary, "--lang", locale, "--help"], cwd=stand, env=environment,
                                   check=True, capture_output=True, timeout=15)
            for locale in ("ru-RU", "en-US"):
                metadata_session(prefix / ("eska-project" + suffix), locale, stand, environment, [])
            components = prefix.parent / "components"
            components.mkdir()
            for tool in ("eska-project", "eska-fmt"):
                component_receipt = components / f"{tool}.json"
                component_receipt.write_text('{"managed": "component fixture"}', encoding="utf-8")
                before = {path.relative_to(stand): path.read_bytes() for path in stand.rglob("*") if path.is_file()}
                for locale in ("ru_RU.UTF-8", "en_US.UTF-8"):
                    rejected = subprocess.run(command, cwd=stand,
                                              env={**environment, "LC_ALL": locale}, capture_output=True, text=True, timeout=120)
                    # cargo-dist's PowerShell catch writes diagnostics to the information/stdout stream.
                    assert rejected.returncode != 0, rejected
                    assert "component-installation-conflict" in rejected.stdout + rejected.stderr, rejected
                    assert tool in rejected.stdout + rejected.stderr, rejected
                    assert {path.relative_to(stand): path.read_bytes() for path in stand.rglob("*") if path.is_file()} == before
                component_receipt.unlink()
            print(json.dumps({"passed": True, "version": version, "components": sorted(tools),
                              "receipt_isolated": True, "component_ownership_preserved": True,
                              "verified_assets": verified_assets}))
        finally:
            server.shutdown()
            worker.join(timeout=10)
            server.server_close()


if __name__ == "__main__":
    main()
