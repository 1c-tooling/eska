#!/usr/bin/env python3
"""Execute baseline and ESKA-formatted BSL in a new owned platform fixture."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time
import uuid
import xml.etree.ElementTree as ET


def main():
    """Keep source, runtime database and all diagnostic evidence under the sibling playground."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seed-xml", required=True, type=Path)
    parser.add_argument("--build-binary", required=True, type=Path)
    parser.add_argument("--platform-version", required=True)
    parser.add_argument("--distrobox", required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    playground = (repo.parent / "eska-playground").resolve()
    stand = Path(tempfile.mkdtemp(prefix="fmt-native-", dir=playground))
    print(stand, flush=True)
    cases = json.load(sys.stdin)
    task = "T122" if any(case.get("server") for case in cases) else "T121"
    (stand / "owner.json").write_text(json.dumps({"task": task, "repository": str(repo)}) + "\n")
    assert 0 < len(cases) <= 128
    (stand / "cases.json").write_text(json.dumps(cases, ensure_ascii=False, indent=2) + "\n")
    root = stand / "project"
    source = root / "src"
    shutil.copytree(args.seed_xml.resolve(), source)
    descriptor = source / "Configuration.xml"
    tree = ET.fromstring(descriptor.read_text(encoding="utf-8-sig"))
    ns = {"m": "http://v8.1c.ru/8.3/MDClasses"}
    children = tree.find("m:Configuration/m:ChildObjects", ns)
    assert children is not None and all(item.tag == f"{{{ns['m']}}}Language" for item in children)
    configuration = descriptor.read_text(encoding="utf-8-sig")
    header = configuration.splitlines()[1]
    additions = []
    startup = ["// Сравнивает результаты модулей до и после форматирования.", "Процедура ПриНачалеРаботыСистемы()", "    Вывод = Новый ЗаписьТекста(ПараметрЗапуска, КодировкаТекста.UTF8);"]
    for index, case in enumerate(cases):
        assert set(case) in ({"name", "before", "after", "expected"}, {"name", "before", "after", "expected", "server"})
        assert isinstance(case["expected"], int) and case["name"].replace("_", "").isalnum()
        names = []
        for phase in ("before", "after"):
            name = f"Fmt{index}{phase.title()}"
            names.append(name)
            additions.append(f"<CommonModule>{name}</CommonModule>")
            directory = source / "CommonModules" / name / "Ext"
            directory.mkdir(parents=True)
            server = case.get("server", False)
            properties = (f"<Name>{name}</Name><Synonym/><Comment/><Global>false</Global>"
                          f"<ClientManagedApplication>{str(not server).lower()}</ClientManagedApplication><Server>{str(server).lower()}</Server>"
                          "<ExternalConnection>false</ExternalConnection><ClientOrdinaryApplication>false</ClientOrdinaryApplication>"
                          f"<ServerCall>{str(server).lower()}</ServerCall><Privileged>false</Privileged><ReturnValuesReuse>DontUse</ReturnValuesReuse>")
            xml = f'\ufeff<?xml version="1.0" encoding="UTF-8"?>\r\n{header}\r\n<CommonModule uuid="{uuid.uuid4()}"><Properties>{properties}</Properties></CommonModule></MetaDataObject>\r\n'
            (source / "CommonModules" / f"{name}.xml").write_bytes(xml.encode())
            (directory / "Module.bsl").write_bytes(case[phase].encode())
        startup += [f"    До = {names[0]}.Run();", f"    После = {names[1]}.Run();",
                    f"    Если До <> После Или До <> {case['expected']} Тогда",
                    f'        Вывод.ЗаписатьСтроку("FAIL {case["name"]}: " + Строка(До) + " / " + Строка(После));',
                    "    Иначе", f'        Вывод.ЗаписатьСтроку("PASS {case["name"]}: " + Строка(До));', "    КонецЕсли;"]
    startup += ["    Вывод.Закрыть();", "    ЗавершитьРаботуСистемы(Ложь);", "КонецПроцедуры", ""]
    additions = "".join(additions)
    if "</ChildObjects>" in configuration:
        configuration = configuration.replace("</ChildObjects>", additions + "</ChildObjects>", 1)
    else:
        configuration = configuration.replace("<ChildObjects/>", f"<ChildObjects>{additions}</ChildObjects>", 1)
    descriptor.write_bytes(("\ufeff" + configuration).encode())
    (source / "Ext").mkdir(exist_ok=True)
    (source / "Ext/ParentConfigurations.bin").write_text("{6,0,0,0,0,0}")
    (source / "Ext/ManagedApplicationModule.bsl").write_bytes(("\ufeff" + "\r\n".join(startup)).encode())
    (root / "eska.toml").write_text(f'[project]\ntype="configuration"\n[build]\nplatform_version="{args.platform_version}"\n')
    environment = dict(os.environ, ESKA_CONFIG_DIR=str(stand / "machine-config"))
    report = {"complete": False, "platform": args.platform_version, "caseCount": len(cases), "steps": []}

    def run(name, command, timeout=120):
        """Record exact exit status and logs; a successful build does not imply code execution."""
        (stand / f"{name}-arguments.json").write_text(json.dumps([str(value) for value in command], ensure_ascii=False) + "\n")
        start = time.monotonic()
        result = subprocess.run(command, cwd=root, env=environment, capture_output=True, timeout=timeout)
        (stand / f"{name}.stdout").write_bytes(result.stdout)
        (stand / f"{name}.stderr").write_bytes(result.stderr)
        report["steps"].append({"name": name, "exitCode": result.returncode, "seconds": round(time.monotonic() - start, 3)})
        assert result.returncode == 0, f"{name} failed; inspect {stand}"
        return result

    try:
        run("build", [args.build_binary.resolve(), "build", "--distrobox", args.distrobox, "--format", "json"])
        artifacts = list((root / "build").glob("*.cf"))
        assert len(artifacts) == 1, artifacts
        prefix = ["distrobox", "enter", args.distrobox, "--"]
        platform = f"/opt/1cv8/x86_64/{args.platform_version}"
        run("platform-version", prefix + [f"{platform}/ibcmd", "--version"])
        run("create", prefix + [f"{platform}/ibcmd", "infobase", "create", f"--db-path={stand / 'ib'}", f"--data={stand / 'data'}", f"--load={artifacts[0]}", "--apply", "--force"])
        result_file = stand / "evaluated.txt"
        run("client", prefix + ["timeout", "--signal=TERM", "--kill-after=5", "60s", f"{platform}/1cv8", "ENTERPRISE", "/F", str(stand / "ib"), "/DisableStartupDialogs", "/DisableStartupMessages", "/C", str(result_file), "/Out", str(stand / "client.log")], 80)
        rows = result_file.read_text(encoding="utf-8-sig").splitlines()
        assert len(rows) == len(cases) and all(row.startswith("PASS ") for row in rows), rows
        report.update(complete=True, evaluated=rows, artifactSha256=hashlib.sha256(artifacts[0].read_bytes()).hexdigest(), builderSha256=hashlib.sha256(args.build_binary.read_bytes()).hexdigest())
    finally:
        (stand / "result.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps({"complete": report["complete"], "stand": str(stand), "caseCount": len(cases)}, ensure_ascii=False))


if __name__ == "__main__":
    main()
