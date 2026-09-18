#!/usr/bin/env python3
"""Measure a real stdio process; never invoke the 1C platform or build source configurations."""

import argparse
import hashlib
import json
import pathlib
import queue
import shutil
import subprocess
import tempfile
import threading
import time

REPO = pathlib.Path(__file__).resolve().parents[1]
PLAYGROUND = (REPO.parent / "eska-playground").resolve()


class Client:
    """Independent bounded-time pipe client with a continuously drained stdout."""

    def __init__(self, binary):
        """Start one fresh process and parse frames without production framing helpers."""
        self.process = subprocess.Popen([str(binary), "ide", "--stdio"], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.messages = queue.Queue()
        self.serial = 0
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.reader.start()

    def read(self):
        """Keep output flowing during indexing and propagate reader errors to the caller."""
        try:
            while header := self.process.stdout.readline():
                assert header.startswith(b"Content-Length: ") and header.endswith(b"\r\n")
                size = int(header.split(b":", 1)[1])
                assert self.process.stdout.read(2) == b"\r\n"
                self.messages.put(json.loads(self.process.stdout.read(size)))
        except Exception as failure:
            self.messages.put(failure)

    def request(self, method, params):
        """Return only the matching response while consuming asynchronous progress events."""
        self.serial += 1
        body = json.dumps(dict(jsonrpc="2.0", id=self.serial, method=method, params=params),
                          ensure_ascii=False).encode()
        self.process.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
        self.process.stdin.flush()
        while True:
            result = self.messages.get(timeout=60)
            if isinstance(result, Exception):
                raise result
            if "method" in result:
                continue
            assert result["id"] == self.serial, result
            assert "error" not in result, result
            return result["result"]

    def close(self):
        """Use acknowledged shutdown followed by EOF, always reap the child."""
        self.request("shutdown", {})
        self.process.stdin.close()
        assert self.process.wait(timeout=10) == 0
        assert not self.process.stderr.read()

    def peak_mib(self):
        """Linux high-water RSS includes the entire server, excluding this Python client."""
        status = pathlib.Path(f"/proc/{self.process.pid}/status").read_text()
        return int(next(line for line in status.splitlines() if line.startswith("VmHWM:")).split()[1]) / 1024


def timed(operation):
    """Measure wall-clock request/response latency in milliseconds."""
    start = time.perf_counter()
    value = operation()
    return value, (time.perf_counter() - start) * 1000


def sample(binary, root, cached):
    """Measure lazy navigation, interactive indexing, queries and one-object refresh."""
    client = Client(binary)
    try:
        client.request("initialize", dict(apiVersion=dict(major=1, minor=0),
                                         client=dict(name="benchmark", version="1")))
        opened, open_ms = timed(lambda: client.request("workspace/open", dict(
            start=dict(value=str(root), encoding="utf-8"), selection=dict(kind="current"), diskCache=cached)))
        project = opened["projects"][0]
        context = dict(sessionId=opened["sessionId"], projectId=project["projectId"], generation=project["generation"])
        call = lambda method, **params: client.request(method, context | params)
        sections = call("metadata/children", node=project["root"])["nodes"]
        catalogs = next(node for node in sections if node["id"].get("collection", {}).get("metadataKind") == "catalog")
        objects = call("metadata/children", node=catalogs["id"])["nodes"]
        catalog = next(node["id"] for node in objects if node["label"].get("text") == "Контрагенты")
        _, expand_ms = timed(lambda: call("metadata/children", node=catalog))
        _, properties_ms = timed(lambda: call("metadata/properties", objectId=catalog["objectId"]))
        warm = [timed(lambda: call("metadata/properties", objectId=catalog["objectId"]))[1] for _ in range(100)]
        start = time.perf_counter()
        call("metadata/index", action="start")
        latencies = []
        while True:
            result, latency = timed(lambda: call("metadata/index", action="status"))
            latencies.append(latency)
            if result["progress"]["state"] != "building":
                break
            time.sleep(0.01)
        index_ms = (time.perf_counter() - start) * 1000
        queries = [timed(lambda: call("metadata/search", text=query))[1]
                   for query in ["Контрагент", "код", "а", "нетТакогоОбъекта"] for _ in range(10)]
        _, filter_ms = timed(lambda: call("metadata/children", node=project["root"]))
        refreshed, refresh_ms = timed(lambda: call("metadata/refresh", node=catalog))
        context["generation"] = refreshed["generation"]
        metrics = dict(open_ms=open_ms, expand_ms=expand_ms, properties_ms=properties_ms,
                       warm_properties_p95_ms=sorted(warm)[94], index_ms=index_ms,
                       interactive_index_max_ms=max(latencies), search_p95_ms=sorted(queries)[37],
                       filter_ms=filter_ms, refresh_ms=refresh_ms, peak_rss_mib=client.peak_mib(),
                       objects=result["progress"]["indexedObjects"], failures=result["progress"]["failedDescriptors"])
        client.close()
        return metrics
    finally:
        if client.process.poll() is None:
            client.process.kill()
            client.process.wait()


def prepare(root, synthetic):
    """Create only owned fixtures under the sibling playground using the T68 dataset shape."""
    shutil.copytree(REPO / "tests/fixtures/designer/configuration", root / "src")
    (root / "eska.toml").write_text("[project]\ntype='configuration'\n")
    if not synthetic:
        return
    children = []
    for number in range(2000):
        name = "Контрагенты" if number == 0 else f"Справочник{number}"
        children.append(f"<Catalog>{name}</Catalog>")
        attributes = "".join(f"<Attribute uuid='id'><Properties><Name>Реквизит{n}</Name></Properties></Attribute>" for n in range(20))
        (root / "src/Catalogs" / f"{name}.xml").write_text(f"<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Catalog uuid='id'><Properties><Name>{name}</Name></Properties><ChildObjects>{attributes}</ChildObjects></Catalog></MetaDataObject>")
    (root / "src/Configuration.xml").write_text(f"<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='id'><Properties><Name>Синтетическая</Name></Properties><ChildObjects>{''.join(children)}</ChildObjects></Configuration></MetaDataObject>")


def main():
    """Print one raw JSON record per fresh process, preserving existing big-project caches."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("dataset", choices=["small", "synthetic", "big"])
    parser.add_argument("--binary", type=pathlib.Path, default=REPO / "target/dist/eska")
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--cached", action="store_true")
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix="eska-ide-bench-", dir=PLAYGROUND) as temporary:
        root = PLAYGROUND / "big_configuration_temp" if args.dataset == "big" else pathlib.Path(temporary)
        if args.dataset != "big":
            prepare(root, args.dataset == "synthetic")
        digest = hashlib.sha256((root / "src/Configuration.xml").read_bytes()).hexdigest()
        for run in range(args.runs):
            result = dict(dataset=args.dataset, cached=args.cached, run=run, root_sha256=digest,
                          sample=sample(args.binary.resolve(), root, args.cached))
            print(json.dumps(result, ensure_ascii=False), flush=True)
        assert hashlib.sha256((root / "src/Configuration.xml").read_bytes()).hexdigest() == digest


if __name__ == "__main__":
    main()
