#!/usr/bin/env python3
"""Exercise real formatter/facade/framed project processes without an analyzer in a new owned stand."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import select
import subprocess
import tempfile
import time
import uuid


class Rpc:
    """One exact child and a finite deadline own all request correlation and cleanup."""

    def __init__(self, binary, root, environment):
        """Use the public standalone entry and preserve the source containment root."""
        self.child = subprocess.Popen([binary, "serve", "--stdio"], cwd=root, env=environment,
                                      stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.buffer = b""
        self.serial = 0

    def send(self, value):
        """Content-Length counts UTF-8 bytes, including Cyrillic text and embedded newlines."""
        data = json.dumps(value, ensure_ascii=False).encode()
        self.child.stdin.write(f"Content-Length: {len(data)}\r\n\r\n".encode() + data)
        self.child.stdin.flush()

    def read(self, count):
        """A broken server cannot hang acceptance beyond the per-response deadline."""
        deadline = time.monotonic() + 35
        while len(self.buffer) < count:
            assert select.select([self.child.stdout], [], [], max(0, deadline - time.monotonic()))[0], "RPC timeout"
            data = os.read(self.child.stdout.fileno(), 65536)
            assert data, "Unexpected RPC EOF"
            self.buffer += data
        result, self.buffer = self.buffer[:count], self.buffer[count:]
        return result

    def call(self, method, parameters, error=False):
        """Skip notifications while requiring exact response IDs and explicit success/error outcomes."""
        self.serial += 1
        self.send({"jsonrpc": "2.0", "id": self.serial, "method": method, "params": parameters})
        while True:
            header = b""
            while not header.endswith(b"\r\n\r\n"):
                header += self.read(1)
            length = int(header.decode().split(":", 1)[1].strip())
            response = json.loads(self.read(length))
            if "method" in response:
                continue
            assert response["id"] == self.serial, response
            assert ("error" in response) == error, response
            return response["error" if error else "result"]

    def finish(self):
        """Shutdown acknowledgement precedes exit; reap only this script's exact child."""
        self.call("shutdown", {})
        self.send({"jsonrpc": "2.0", "method": "exit"})
        self.child.stdin.close()
        assert self.child.wait(timeout=10) == 0
        assert self.child.stderr.read() == b""


def path(value):
    """Native path DTOs remain distinct from query identities and raw source strings."""
    return {"encoding": "utf-8", "value": str(value)}


def main():
    """Record actual CLI/RPC parity and query ownership, without touching existing user demos or installations."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--formatter", required=True, type=Path)
    parser.add_argument("--project", required=True, type=Path)
    parser.add_argument("--facade", required=True, type=Path)
    arguments = parser.parse_args()
    tools = {name: value.resolve() for name, value in vars(arguments).items()}
    repo = Path(__file__).resolve().parents[1]
    stand = Path(tempfile.mkdtemp(prefix="t123-fmt-rpc-", dir=(repo.parent / "eska-playground").resolve()))
    (stand / "owner.json").write_text(json.dumps({"task": "T123", "kind": "formatting-cli-rpc"}) + "\n")
    environment = dict(os.environ, ESKA_FMT_BINARY=str(tools["formatter"]), ESKA_BSL_ANALYZER=str(stand / "missing-analyzer"))
    report = {"complete": False, "stand": str(stand), "locales": {}}
    print(stand, flush=True)

    def cli(tool, locale, args):
        """Capture real process exit and pure JSON under a test-owned working directory."""
        result = subprocess.run([tools[tool], "--lang", locale, *args], cwd=stand, env=environment, capture_output=True, timeout=35)
        assert result.returncode in (0, 1), result.stderr.decode()
        return result.returncode, json.loads(result.stdout)

    for locale in ("ru-RU", "en-US"):
        before = "// Проверяет форматирование.\r\nпроцедура Тест()\r\nРезультат=1+2;\r\nконецпроцедуры"
        source = stand / "Module.bsl"
        source.write_bytes(before.encode())
        direct = cli("formatter", locale, ["Module.bsl", "--check", "--format", "json"])
        facade = cli("facade", locale, ["fmt", "Module.bsl", "--check", "--format", "json"])
        assert direct == facade and direct[0] == 1
        rpc = Rpc(tools["project"], stand, environment)
        try:
            hello = rpc.call("initialize", {"apiVersion": {"major": 1, "minor": 33}, "client": {"name": "fmt-acceptance", "version": "1"}, "locale": locale})
            assert hello["capabilities"]["formatting"]["component"] == "eska-fmt"
            output = rpc.call("format/document", {"source": path(source), "version": "7", "text": before, "userReadOnly": False})
            assert output["plan"]["edits"] == direct[1]["files"][0]["edits"]
            assert output["effective"] == direct[1]["files"][0]["effective"]
            assert source.read_bytes() == before.encode()
            project = stand / locale
            (project / "src/Ext").mkdir(parents=True)
            (project / "eska.toml").write_text("[project]\ntype='configuration'\n")
            (project / "src/Configuration.xml").write_text("<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='11111111-1111-1111-1111-111111111111'><Properties><Name>Demo</Name></Properties><ChildObjects/></Configuration></MetaDataObject>")
            (project / "src/Ext/ParentConfigurations.bin").write_text("{6,0,0,0,0,0}")
            owner = 'q = "select 1 as A";\n'
            owner_path = project / "src/Ext/ManagedApplicationModule.bsl"
            owner_path.write_text(owner)
            opened = rpc.call("workspace/open", {"start": path(project), "selection": {"kind": "current"}, "diskCache": False})
            member = opened["projects"][0]
            context = {"sessionId": opened["sessionId"], "projectId": member["projectId"], "generation": member["generation"]}
            document = rpc.call("query/open", dict(context, node={"kind": "module", "owner": member["root"]["objectId"], "role": "managed-application"}, source=path("Ext/ManagedApplicationModule.bsl"), version="1", text=owner))
            binding = dict(context, **{key: document[key] for key in ("documentId", "version", "snapshot", "contextFingerprint")})
            selected = rpc.call("query/read", dict(binding, at={"line": 0, "character": owner.index('"') + 1}))["query"]
            store = rpc.call("query/recoveryOpen", {"directory": path(stand / f"recovery-{locale}")})
            session = rpc.call("query/sessionOpen", dict(context, documentId=document["documentId"], storeId=store["storeId"], id=str(uuid.uuid4()), query=selected["identity"], format="editor", userReadOnly=False))
            formatted = rpc.call("format/query", dict(context, querySessionId=session["querySessionId"], revision=session["revision"], version="1", text=selected["text"], userReadOnly=False))
            assert formatted["plan"]["edits"] and owner_path.read_text() == owner
            blocked = rpc.call("format/document", {"source": path(owner_path), "version": "1", "text": owner, "userReadOnly": False}, error=True)
            assert blocked["data"]["details"]["code"] == "source_busy", blocked
            rpc.call("query/sessionClose", dict(context, querySessionId=session["querySessionId"]))
            restored = rpc.call("format/document", {"source": path(owner_path), "version": "1", "text": owner, "userReadOnly": False})
            assert restored["plan"]["edits"] == []
            assert owner_path.read_text() == owner
            report["locales"][locale] = {"facadeParity": True, "rpcParity": True, "queryDraftFormatted": True, "activeParentBlocked": True, "releasedParentWritable": True, "sourceUnchanged": True, "sourceSha256": hashlib.sha256(before.encode()).hexdigest()}
            (stand / f"result-{locale}.json").write_text(json.dumps({"cli": direct[1], "rpc": output, "query": formatted}, ensure_ascii=False, indent=2) + "\n")
            rpc.finish()
        finally:
            if rpc.child.poll() is None:
                rpc.child.kill()
                rpc.child.wait()
            (stand / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    report["complete"] = True
    (stand / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"complete": True, "stand": str(stand), "locales": list(report["locales"])}, ensure_ascii=False))


if __name__ == "__main__":
    main()
