#!/usr/bin/env python3
"""Validate XML query edits by building and reloading an owned 1C configuration through ESKA."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import uuid
import xml.etree.ElementTree as ET


def digest(path):
    """Hash exact source bytes independently from XML normalization."""
    return hashlib.sha256(path.read_bytes()).hexdigest()


def snapshot(root):
    """Include every source file so a query plan cannot hide an unrelated change."""
    return {str(p.relative_to(root)): digest(p) for p in root.rglob("*") if p.is_file()}


class QueryClient:
    """Use an independent framed client rather than the production TypeScript codec."""

    def __init__(self, binary, root):
        """Own one project server solely for this fixture."""
        self.process = subprocess.Popen([str(binary), "serve", "--stdio"], cwd=root,
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        self.serial = 0

    def request(self, method, params):
        """Correlate responses while skipping request-independent notifications."""
        self.serial += 1
        body = json.dumps({"jsonrpc": "2.0", "id": self.serial, "method": method, "params": params},
                          ensure_ascii=False).encode()
        self.process.stdin.write(f"Content-Length: {len(body)}\r\n\r\n".encode() + body)
        self.process.stdin.flush()
        while True:
            headers = {}
            while True:
                line = self.process.stdout.readline()
                if line == b"\r\n":
                    break
                if not line:
                    raise RuntimeError("project server EOF")
                name, value = line.decode("ascii").strip().split(":", 1)
                headers[name.lower()] = value.strip()
            response = json.loads(self.process.stdout.read(int(headers["content-length"])))
            if response.get("id") != self.serial:
                continue
            if "error" in response:
                raise RuntimeError(json.dumps(response["error"], ensure_ascii=False))
            return response["result"]

    def close(self):
        """Reap the exact owned process even after a failed assertion."""
        if self.process.poll() is None:
            try:
                self.request("shutdown", {})
                self.process.stdin.close()
                self.process.wait(timeout=10)
            except Exception:
                self.process.kill()
                self.process.wait()
        if self.process.returncode != 0:
            raise RuntimeError("project server failed")


def prepare(repo, seed, stand, version):
    """Copy a platform-created empty configuration; never modify the supplied seed."""
    md = {"m": "http://v8.1c.ru/8.3/MDClasses"}
    descriptor = seed / "Configuration.xml"
    document = ET.parse(descriptor)
    children = document.find("m:Configuration/m:ChildObjects", md)
    if children is None or any(p.tag != f"{{{md['m']}}}Language" for p in children):
        raise ValueError("--seed-xml must be an empty configuration, with only optional Language objects")
    root = stand / "project"
    shutil.copytree(seed, root / "src")
    text = descriptor.read_bytes().decode("utf-8-sig")
    header = text.splitlines()[1]
    additions = "<CommonTemplate>QuerySchema</CommonTemplate><CommonForm>QueryLists</CommonForm>"
    if "</ChildObjects>" in text:
        text = text.replace("</ChildObjects>", additions + "</ChildObjects>", 1)
    else:
        text = text.replace("<ChildObjects/>", f"<ChildObjects>{additions}</ChildObjects>", 1)
    (root / "src/Configuration.xml").write_bytes(("\ufeff" + text).encode())
    for kind, name, payload, properties in [
        ("CommonTemplate", "QuerySchema", "schema.xml", "<TemplateType>DataCompositionSchema</TemplateType>"),
        ("CommonForm", "QueryLists", "form.xml",
         "<FormType>Managed</FormType><IncludeHelpInContents>false</IncludeHelpInContents>"
         '<UsePurposes><v8:Value xsi:type="app:ApplicationUsePurpose">PlatformApplication</v8:Value></UsePurposes>'
         "<UseStandardCommands>true</UseStandardCommands><ExtendedPresentation/><Explanation/>"),
    ]:
        folder = root / "src" / f"{kind}s"
        (folder / name / "Ext").mkdir(parents=True)
        value = f'\ufeff<?xml version="1.0" encoding="UTF-8"?>\r\n{header}\r\n<{kind} uuid="{uuid.uuid4()}">'
        value += f"<Properties><Name>{name}</Name><Synonym/><Comment/>{properties}</Properties></{kind}></MetaDataObject>\r\n"
        (folder / f"{name}.xml").write_bytes(value.encode())
        target = "Template.xml" if kind == "CommonTemplate" else "Form.xml"
        shutil.copyfile(repo / "tests/fixtures/query-editing" / payload, folder / name / "Ext" / target)
    (root / "src/Ext").mkdir(exist_ok=True)
    (root / "src/Ext/ParentConfigurations.bin").write_text("{6,0,0,0,0,0}", encoding="utf-8")
    (root / "eska.toml").write_text(
        f'[project]\nname="query-platform-test"\ntype="configuration"\n[build]\nplatform_version="{version}"\n',
        encoding="utf-8")
    return root


def edit_queries(project_binary, root):
    """Apply only the backend's selected scalar replacements to both XML source kinds."""
    before = snapshot(root / "src")
    client = QueryClient(project_binary, root)
    changes = []
    try:
        hello = client.request("initialize", {"apiVersion": {"major": 1, "minor": 13},
                               "client": {"name": "native-query-test", "version": "1"}, "locale": "en-US"})
        assert hello["capabilities"]["queryEditing"]["editorLineEndings"]
        opened = client.request("workspace/open", {"start": {"value": str(root), "encoding": "utf-8"},
                                                 "selection": {"kind": "current"}})
        project = opened["projects"][0]
        key = {"sessionId": opened["sessionId"], "projectId": project["projectId"], "generation": project["generation"]}

        def rpc(method, fields):
            """Keep session/member/generation binding explicit on every query request."""
            return client.request(method, {**key, **fields})

        groups = rpc("metadata/children", {"node": project["root"]})["nodes"]
        common = next(p for p in groups if p["id"].get("collection", {}).get("kind") == "common")
        groups += rpc("metadata/children", {"node": common["id"]})["nodes"]
        for kind, source, value in [
            ("common-template", "CommonTemplates/QuerySchema/Ext/Template.xml", "SELECT 11 AS N"),
            ("common-form", "CommonForms/QueryLists/Ext/Form.xml", "SELECT 12 AS N"),
        ]:
            group = next(p for p in groups if p["id"].get("collection", {}).get("metadataKind") == kind)
            [owner] = rpc("metadata/children", {"node": group["id"]})["nodes"]
            document = rpc("query/open", {"node": owner["id"], "source": {"value": source, "encoding": "utf-8"}, "version": "1"})
            assert document["writable"]
            binding = {k: document[k] for k in ["documentId", "version", "snapshot", "contextFingerprint"]}
            [first, *_] = rpc("query/list", binding)["queries"]
            query = rpc("query/read", {**binding, "query": first["identity"]})["query"]
            plan = rpc("query/plan", {**binding, "query": query["identity"], "text": value, "comments": [], "lineEnding": "crlf"})
            path = root / "src" / source
            original = path.read_bytes()
            output = original
            for replacement in reversed(plan["replacements"]):
                start, end = replacement["range"]["start"], replacement["range"]["end"]
                output = output[:start] + replacement["text"].encode() + output[end:]
            assert original.count(b"SELECT 1 AS N") == 1
            assert output == original.replace(b"SELECT 1 AS N", value.encode(), 1)
            assert hashlib.sha256(output).hexdigest() == plan["outputSnapshot"]
            path.write_bytes(output)
            rpc("query/close", {"documentId": document["documentId"]})
            changes.append({"source": source, "identity": query["identity"], "text": value, "sha256": plan["outputSnapshot"]})
    finally:
        client.close()
    after = snapshot(root / "src")
    assert {p for p in before if before[p] != after[p]} == {p["source"] for p in changes}
    return {"before": before, "after": after, "changes": changes}


def main():
    """Create a fresh owned stand, capture native evidence and report completion only after re-export."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed-xml", type=Path, required=True)
    parser.add_argument("--distrobox", required=True)
    parser.add_argument("--platform-version", required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    playground = Path(os.environ.get("ESKA_TEST_ROOT", repo.parent / "eska-playground")).resolve()
    stand = Path(tempfile.mkdtemp(prefix="t101-native-queries-", dir=playground))
    print(stand, flush=True)
    (stand / "owner.json").write_text(json.dumps({"kind": "t101-native-queries", "repository": str(repo)}) + "\n")
    report = {"complete": False, "platformVersion": args.platform_version, "runner": "distrobox", "queriesExecuted": False}
    environment = dict(os.environ, ESKA_CONFIG_DIR=str(stand / "machine-config"))

    def call(name, command):
        """Retain stdout/stderr and validate the actual process outcome."""
        result = subprocess.run(command, cwd=stand, env=environment, capture_output=True, timeout=120)
        (stand / f"{name}.stdout").write_bytes(result.stdout)
        (stand / f"{name}.stderr").write_bytes(result.stderr)
        if result.returncode:
            raise RuntimeError(f"{name}: exit {result.returncode}; inspect {stand}")
        return json.loads(result.stdout)

    try:
        root = prepare(repo, args.seed_xml.resolve(), stand, args.platform_version)
        report["queryEdits"] = edit_queries(repo / "target/debug/eska-project", root)
        build = repo / "target/debug/eska-build"
        tools = ["--platform-version", args.platform_version, "--distrobox", args.distrobox, "--format", "json"]
        output = call("build", [str(build), "--project-dir", str(root), "build", "--manifest", *tools])
        artifact = Path(output["artifact"]["path"])
        exported = stand / "exported"
        call("unpack", [str(build), "unpack", str(artifact), str(exported), *tools])
        namespaces = {"s": "http://v8.1c.ru/8.1/data-composition-system/schema", "f": "http://v8.1c.ru/8.3/xcf/logform"}
        schema = exported / "CommonTemplates/QuerySchema/Ext/Template.xml"
        form = exported / "CommonForms/QueryLists/Ext/Form.xml"
        values = [p.text for p in ET.parse(schema).findall("s:dataSet/s:query", namespaces)]
        lists = [p.text for p in ET.parse(form).findall("f:Attributes/f:Attribute/f:Settings/f:QueryText", namespaces)]
        assert values == ["SELECT 11 AS N", "SELECT 2 AS N"], values
        assert lists == ["SELECT 12 AS N"], lists
        assert snapshot(root / "src") == report["queryEdits"]["after"]
        report.update(complete=True, schemaQueries=values, dynamicListQueries=lists, artifactSha256=digest(artifact),
                      artifactBytes=artifact.stat().st_size, sourceUnchangedByBuild=True,
                      backendSha256=digest(repo / "target/debug/eska-project"), builderSha256=digest(build))
    finally:
        (stand / "result.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"complete": report["complete"], "stand": str(stand)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
