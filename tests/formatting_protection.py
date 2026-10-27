#!/usr/bin/env python3
"""Verify supplier and controlled-method protection through actual CLI/facade/RPC processes."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import uuid

from formatting_acceptance import Rpc, path


def fixture(root):
    """Create a small owned Designer export with locked, supported and unrestricted modules."""
    ext = root / "src/Ext"
    ext.mkdir(parents=True)
    (root / "eska.toml").write_text("[project]\ntype='configuration'\n")
    configuration = "11111111-1111-1111-1111-111111111111"
    locked = "22222222-2222-2222-2222-222222222222"
    own = "33333333-3333-3333-3333-333333333333"
    (root / "src/Configuration.xml").write_text(
        f"<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><Configuration uuid='{configuration}'>"
        "<Properties><Name>FmtProtection</Name></Properties><ChildObjects>"
        "<CommonModule>Locked</CommonModule><CommonModule>Own</CommonModule>"
        "</ChildObjects></Configuration></MetaDataObject>")
    controlled = ('&ChangeAndValidate("Run")\nProcedure Custom()\n'
                  '   A=1; // reference  \n#Insert\nB= 2;\n#EndInsert\nEndProcedure')
    texts = {"Ext/ManagedApplicationModule.bsl": 'Procedure Run()\n    A=1;\nQ="select 1 as A,2 as B";\nEndProcedure\n',
             "CommonModules/Locked/Ext/Module.bsl": 'invalid "unterminated',
             "CommonModules/Own/Ext/Module.bsl": f"X=1;\n{controlled}\n\nProcedure Other()\nC=3;\nEndProcedure\n"}
    for name, object_uuid in [("Locked", locked), ("Own", own)]:
        descriptor = root / f"src/CommonModules/{name}.xml"
        descriptor.parent.mkdir(exist_ok=True)
        descriptor.write_text(f"<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'><CommonModule uuid='{object_uuid}'>"
                              f"<Properties><Name>{name}</Name></Properties></CommonModule></MetaDataObject>")
    for relative, text in texts.items():
        source = root / "src" / relative
        source.parent.mkdir(parents=True, exist_ok=True)
        source.write_text(text)
    rules = ["6", "0", "1", configuration, "0", configuration, '"1"', '"Vendor"', '"Demo"', "2",
             "1", "0", configuration, configuration, "0", "0", locked, locked, "0", "0", *(["0"] * 13)]
    (ext / "ParentConfigurations.bin").write_text("{" + ",".join(rules) + "}")
    return texts, controlled


def cli(tool, root, locale, arguments, environment, expected):
    """Require the actual exit status and keep machine output independent of locale."""
    result = subprocess.run([tool, "--lang", locale, *arguments], cwd=root, env=environment,
                            capture_output=True, timeout=40)
    assert result.returncode == expected, (result.returncode, result.stdout, result.stderr)
    return json.loads(result.stdout)


def draft(rpc, root, owner, locale, store=None):
    """Open the real source-bound query session used by IDE adapters, without applying its edits."""
    opened = rpc.call("workspace/open", {"start": path(root), "selection": {"kind": "current"}, "diskCache": False})
    member = opened["projects"][0]
    context = {"sessionId": opened["sessionId"], "projectId": member["projectId"], "generation": member["generation"]}
    document = rpc.call("query/open", dict(context, node={"kind": "module", "owner": member["root"]["objectId"], "role": "managed-application"},
                                           source=path("Ext/ManagedApplicationModule.bsl"), version="1", text=owner))
    binding = dict(context, **{key: document[key] for key in ("documentId", "version", "snapshot", "contextFingerprint")})
    offset = owner.index("select")
    selected = rpc.call("query/read", dict(binding, at={"line": owner[:offset].count("\n"), "character": offset - owner.rfind("\n", 0, offset) - 1}))["query"]
    if store is None:
        store = rpc.call("query/recoveryOpen", {"directory": path(root.parent / f"recovery-{locale}-{uuid.uuid4()}")})
    session = rpc.call("query/sessionOpen", dict(context, documentId=document["documentId"], storeId=store["storeId"],
                                                 id=str(uuid.uuid4()), query=selected["identity"], format="editor", userReadOnly=False))
    parameters = dict(context, querySessionId=session["querySessionId"], revision=session["revision"], version="1", text=selected["text"], userReadOnly=False)
    return context, session, parameters, store


def main():
    """Write reproducible ru/en evidence only in a new directory under the sibling playground."""
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("formatter", "project", "facade"):
        parser.add_argument("--" + name, required=True, type=Path)
    arguments = parser.parse_args()
    tools = {name: value.resolve() for name, value in vars(arguments).items()}
    repo = Path(__file__).resolve().parents[1]
    stand = Path(tempfile.mkdtemp(prefix="fmt-protection-rpc-", dir=(repo.parent / "eska-playground").resolve()))
    (stand / "owner.json").write_text(json.dumps({"task": "fmt support protection", "repository": str(repo)}))
    environment = dict(os.environ, ESKA_FMT_BINARY=str(tools["formatter"]), ESKA_BSL_ANALYZER=str(stand / "missing-analyzer"))
    report = {"complete": False, "stand": str(stand), "locales": {}}
    print(stand, flush=True)
    try:
        for locale in ("ru-RU", "en-US"):
            root = stand / locale
            originals, controlled = fixture(root)
            owner_path = root / "src/Ext/ManagedApplicationModule.bsl"
            owner = originals["Ext/ManagedApplicationModule.bsl"]
            own_path = root / "src/CommonModules/Own/Ext/Module.bsl"
            records = []
            for mode in ("--check", "--diff", None):
                # Reset only this run's unrestricted module for equal direct/facade input.
                snapshots = []
                for tool, prefix in (("formatter", []), ("facade", ["fmt"])):
                    own_path.write_text(originals["CommonModules/Own/Ext/Module.bsl"])
                    args = [*prefix, *([mode] if mode else []), "--style", '{"column_limit":0}', "--format", "json"]
                    output = cli(tools[tool], root, locale, args, environment, int(mode == "--check"))
                    snapshots.append(output)
                    by_name = {Path(file["path"]).relative_to(root / "src").as_posix(): file for file in output["files"]}
                    assert by_name["Ext/ManagedApplicationModule.bsl"]["skip_reason"] == "selection_required"
                    assert by_name["CommonModules/Locked/Ext/Module.bsl"]["skip_reason"] == "source_read_only"
                    for relative in ("Ext/ManagedApplicationModule.bsl", "CommonModules/Locked/Ext/Module.bsl"):
                        assert (root / "src" / relative).read_text() == originals[relative]
                        assert by_name[relative]["source_sha256"] is None
                    assert controlled in own_path.read_text()
                assert snapshots[0] == snapshots[1]
                records.append(snapshots[0])
            rpc = Rpc(tools["project"], root, environment)
            try:
                rpc.call("initialize", {"apiVersion": {"major": 1, "minor": 33}, "client": {"name": "fmt-protection", "version": "1"}, "locale": locale})
                parameters = {"source": path(owner_path), "version": "1", "text": owner, "userReadOnly": False}
                for selection in (None, {"start": 0, "end": 0}, {"start": 0, "end": len(owner.encode())}):
                    rejected = rpc.call("format/document", dict(parameters, **({"range": selection} if selection else {})), error=True)
                    assert rejected["data"]["details"]["code"] == "selection_required", rejected
                start = owner.index("    A=1;")
                selected = {"start": start, "end": start + len("    A=1;")}
                formatted = rpc.call("format/document", dict(parameters, range=selected))
                assert formatted["plan"]["edits"]
                assert all(start <= edit["range"]["start"] <= edit["range"]["end"] <= selected["end"] for edit in formatted["plan"]["edits"])
                expanded = rpc.call("format/document", dict(parameters, range={"start": start + 4, "end": start + 5}), error=True)
                assert expanded["data"]["details"]["code"] == "selection_expansion", expanded
                context, session, parameters, store = draft(rpc, root, owner, locale)
                assert rpc.call("format/query", parameters)["plan"]["edits"]
                rpc.call("query/sessionClose", dict(context, querySessionId=session["querySessionId"]))
                rpc.call("workspace/close", {"sessionId": context["sessionId"]})
                changed = '&ИзменениеИКонтроль("Run")\n' + owner
                owner_path.write_text(changed)
                context, session, parameters, _store = draft(rpc, root, changed, locale, store)
                rejected = rpc.call("format/query", parameters, error=True)
                assert rejected["data"]["details"]["code"] == "change_control_protected", rejected
                rpc.call("query/sessionClose", dict(context, querySessionId=session["querySessionId"]))
                assert owner_path.read_text() == changed
                rpc.finish()
            finally:
                if rpc.child.poll() is None:
                    rpc.child.kill()
                    rpc.child.wait()
            report["locales"][locale] = {"facadeParity": True, "bulkApplyCheckDiffSkipSupported": True,
                "protectedSyntaxNotParsed": True, "supportedSelectionBounded": True,
                "controlledMethodPreserved": True, "controlledQueryRejected": True,
                "supportedPrivateQueryFormatted": True,
                "lockedSha256": hashlib.sha256((root / "src/CommonModules/Locked/Ext/Module.bsl").read_bytes()).hexdigest()}
            (root / "cli-results.json").write_text(json.dumps(records, ensure_ascii=False, indent=2))
        report["complete"] = True
    finally:
        (stand / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(report, ensure_ascii=False))


if __name__ == "__main__":
    main()
