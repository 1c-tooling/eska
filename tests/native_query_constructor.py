#!/usr/bin/env python3
"""Build and reload constructor-edited XML and BSL through the installed 1C platform and ESKA."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import uuid
import xml.etree.ElementTree as ET
from native_queries import prepare, digest, snapshot


def main():
    """Keep every source, private checkpoint and native output inside a unique owned playground stand."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed-xml", type=Path, required=True)
    parser.add_argument("--project-binary", type=Path, required=True)
    parser.add_argument("--build-binary", type=Path, required=True)
    parser.add_argument("--adapter-probe", type=Path, required=True)
    parser.add_argument("--distrobox", required=True)
    parser.add_argument("--platform-version", required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    playground = Path(os.environ.get("ESKA_TEST_ROOT", repo.parent / "eska-playground")).resolve()
    stand = Path(tempfile.mkdtemp(prefix="t105-platform-", dir=playground))
    (stand / "owner.json").write_text(json.dumps({"task": "T105", "repository": str(repo)}) + "\n")
    print(stand, flush=True)
    report = {"complete": False, "platformVersion": args.platform_version, "queriesExecuted": False}
    environment = dict(os.environ, ESKA_CONFIG_DIR=str(stand / "machine-config"))

    def call(name, command):
        """Capture each real process and never interpret success from a log without its exit status."""
        result = subprocess.run(command, cwd=stand, env=environment, capture_output=True, timeout=180)
        (stand / f"{name}.stdout").write_bytes(result.stdout)
        (stand / f"{name}.stderr").write_bytes(result.stderr)
        result.check_returncode()
        return result.stdout

    try:
        root = prepare(repo, args.seed_xml.resolve(), stand, args.platform_version)
        configuration = root / "src/Configuration.xml"
        text = configuration.read_bytes().decode("utf-8-sig")
        header = text.splitlines()[1]
        text = text.replace("</ChildObjects>", "<CommonModule>QueryBSL</CommonModule></ChildObjects>", 1)
        configuration.write_bytes(("\ufeff" + text).encode())
        directory = root / "src/CommonModules/QueryBSL/Ext"
        directory.mkdir(parents=True)
        descriptor = (f'\ufeff<?xml version="1.0" encoding="UTF-8"?>\r\n{header}\r\n'
                      f'<CommonModule uuid="{uuid.uuid4()}"><Properties><Name>QueryBSL</Name><Synonym/><Comment/>'
                      '<Global>false</Global><ClientManagedApplication>false</ClientManagedApplication><Server>true</Server>'
                      '<ExternalConnection>true</ExternalConnection><ClientOrdinaryApplication>false</ClientOrdinaryApplication>'
                      '<ServerCall>false</ServerCall><Privileged>false</Privileged><ReturnValuesReuse>DontUse</ReturnValuesReuse>'
                      '</Properties></CommonModule></MetaDataObject>\r\n')
        (root / "src/CommonModules/QueryBSL.xml").write_bytes(descriptor.encode())
        module = ('\ufeff// Create a query without executing it.\r\nПроцедура Test() Экспорт\r\n'
                  '    Запрос = Новый Запрос; Запрос.Текст = "SELECT 1 AS N";\r\nКонецПроцедуры\r\n')
        (directory / "Module.bsl").write_bytes(module.encode())
        before = snapshot(root / "src")
        call("rpc", ["node", str(args.adapter_probe.resolve()), str(args.project_binary.resolve()), str(root)])
        rpc = json.loads((stand / "rpc-result.json").read_text())
        assert rpc["passed"] and len(rpc["results"]) == 2
        edited = snapshot(root / "src")
        changed = sorted(key for key in before if before[key] != edited[key])
        assert changed == sorted(["CommonTemplates/QuerySchema/Ext/Template.xml", "CommonForms/QueryLists/Ext/Form.xml", "CommonModules/QueryBSL/Ext/Module.bsl"])
        build = str(args.build_binary.resolve())
        options = ["--platform-version", args.platform_version, "--distrobox", args.distrobox, "--format", "json"]
        result = json.loads(call("build", [build, "--project-dir", str(root), "build", "--manifest", *options]))
        artifact = Path(result["artifact"]["path"])
        exported = stand / "exported"
        call("unpack", [build, "unpack", str(artifact), str(exported), *options])
        namespaces = {"s": "http://v8.1c.ru/8.1/data-composition-system/schema", "f": "http://v8.1c.ru/8.3/xcf/logform"}
        schema_queries = [p.text for p in ET.parse(exported / "CommonTemplates/QuerySchema/Ext/Template.xml").findall("s:dataSet/s:query", namespaces)]
        list_queries = [p.text for p in ET.parse(exported / "CommonForms/QueryLists/Ext/Form.xml").findall("f:Attributes/f:Attribute/f:Settings/f:QueryText", namespaces)]
        bsl = (exported / "CommonModules/QueryBSL/Ext/Module.bsl").read_text(encoding="utf-8-sig")
        assert schema_queries == ["SELECT 31 AS N", "SELECT 2 AS N"]
        assert list_queries == ["SELECT 31 AS N"]
        assert 'Запрос.Текст = "SELECT 31 AS N"' in bsl
        assert snapshot(root / "src") == edited
        report.update(complete=True, rpc=rpc, changedSources=changed, sourceBefore=before, sourceAfter=edited,
                      schemaQueries=schema_queries, listQueries=list_queries, exportedBslQuery="SELECT 31 AS N",
                      artifactSha256=digest(artifact), artifactBytes=artifact.stat().st_size,
                      projectSha256=digest(args.project_binary), builderSha256=digest(args.build_binary), sourceUnchangedByBuild=True)
    finally:
        (stand / "result.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"complete": report["complete"], "stand": str(stand)}))


if __name__ == "__main__":
    main()
