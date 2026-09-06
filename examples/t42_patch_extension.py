"""Reproduce the T42 feasibility experiment; never process a user's configuration."""

import argparse
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
from xml.dom import minidom


VERSION = "8.3.27.2325"
MODULE_ID = "fb40f481-0d8a-43c6-a457-22f05fc978cb"
ADOPTED_ID = "68977a95-1e4c-492f-aa65-046d44b719b1"
MODULE_PATH = "CommonModules/PatchProbe/Ext/Module.bsl"
BASE_CODE = "// Return the fixture value.\nFunction Value() Export\n Return 1;\nEndFunction\n"
HEAD_CODE = BASE_CODE.replace("Return 1", "Return 2")


def write_source(path, text):
    """Write synthetic Designer sources with UTF-8 BOM and CRLF."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b"\xef\xbb\xbf" + text.replace("\r\n", "\n").replace("\n", "\r\n").encode())


def add_module(source):
    """Add one fixture child without discarding XML namespace declarations."""
    path = source / "Configuration.xml"
    document = minidom.parseString(path.read_bytes())
    children = document.getElementsByTagName("ChildObjects")
    if len(children) != 1:
        raise RuntimeError("Expected one synthetic Configuration.ChildObjects")
    module = document.createElement("CommonModule")
    module.appendChild(document.createTextNode("PatchProbe"))
    children[0].appendChild(module)
    write_source(path, document.toxml())


def module_xml(adopted=False):
    """Describe only the fixed server-call module used by this experiment."""
    identity = ADOPTED_ID if adopted else MODULE_ID
    belonging = "<ObjectBelonging>Adopted</ObjectBelonging>" if adopted else ""
    mapping = f"<ExtendedConfigurationObject>{MODULE_ID}</ExtendedConfigurationObject>" if adopted else ""
    return f'''<?xml version="1.0" encoding="UTF-8"?>
<MetaDataObject xmlns="http://v8.1c.ru/8.3/MDClasses" version="2.20">
<CommonModule uuid="{identity}"><Properties>{belonging}
<Name>PatchProbe</Name>{mapping}<Synonym/><Comment/><Global>false</Global>
<ClientManagedApplication>false</ClientManagedApplication><Server>true</Server>
<ExternalConnection>true</ExternalConnection><ClientOrdinaryApplication>false</ClientOrdinaryApplication>
<ServerCall>true</ServerCall><Privileged>false</Privileged><ReturnValuesReuse>DontUse</ReturnValuesReuse>
</Properties></CommonModule></MetaDataObject>
'''


class Probe:
    """Keep all databases, source fixtures, logs and artifacts in one owned directory."""

    def __init__(self, args):
        """Create a unique workspace in the repository's sibling playground."""
        playground = Path(__file__).resolve().parents[2] / "eska-playground"
        self.root = Path(tempfile.mkdtemp(prefix="t42-feasibility-", dir=playground))
        self.runner = ["distrobox", "enter", "--name", args.distrobox, "--"] if args.distrobox else []
        self.ibcmd = args.ibcmd
        self.client = args.client
        self.steps = []
        print(self.root, flush=True)

    def run(self, label, command, expected=0):
        """Record exact arguments and outputs; a failed expectation stops the experiment."""
        result = subprocess.run(command, capture_output=True, timeout=60)
        (self.root / f"{label}.stdout").write_bytes(result.stdout)
        (self.root / f"{label}.stderr").write_bytes(result.stderr)
        self.steps.append({"label": label, "argv": command, "exit_code": result.returncode})
        (self.root / "commands.json").write_text(json.dumps(self.steps, indent=2), encoding="utf-8")
        print(f"{label}: {result.returncode}", flush=True)
        if result.returncode != expected:
            raise RuntimeError(f"{label}: expected exit {expected}, see {self.root}")
        return result.stdout

    def command(self, label, *args, expected=0):
        """Invoke the explicitly selected platform without shell interpolation."""
        return self.run(label, self.runner + [self.ibcmd, *map(str, args)], expected)

    def git(self, label, *args):
        """Use system Git only as a synthetic experiment fixture, outside product code."""
        return self.run(label, ["git", "-C", str(self.root / "repo"),
                              "-c", f"core.hooksPath={self.root / 'empty-hooks'}", "-c", "commit.gpgsign=false",
                              "-c", "user.name=Eska T42", "-c", "user.email=t42@example.invalid", *args])

    def designer(self, label, data, *checks, expected=0):
        """Run an explicit batch check and verify the separate Designer result file."""
        result_path = self.root / f"{label}.result"
        self.run(label, self.runner + [self.client, "DESIGNER", "/F", str(data / "db-data"),
                 "/DisableStartupDialogs", "/DisableStartupMessages", "/Out", str(self.root / f"{label}.log"),
                 "/DumpResult", str(result_path), *checks], expected)
        if result_path.read_text(encoding="utf-8-sig").strip() != str(expected):
            raise RuntimeError(f"{label}: unexpected Designer result file")

    def runtime(self, label, data, expected):
        """Check the observed value, since a successful client exit can hide extension errors."""
        output = self.root / "runtime.txt"
        output.unlink(missing_ok=True)
        self.run(label, self.runner + [self.client, "ENTERPRISE", "/F", str(data / "db-data"),
                 "/DisableStartupDialogs", "/DisableStartupMessages", "/Out", str(self.root / f"{label}.log")])
        value = output.read_text(encoding="utf-8-sig").strip()
        if value != str(expected):
            raise RuntimeError(f"{label}: expected {expected}, observed {value}")
        (self.root / f"{label}.value").write_text(value, encoding="utf-8")

    def execute(self):
        """Prove a fixed Git delta, round-trip, runtime behavior and negative controls."""
        version = self.command("version", "--version").decode().strip()
        if version != VERSION:
            raise RuntimeError(f"This experiment is specified only for {VERSION}, found {version}")
        self.command("help-config", "help", "config")
        self.command("help-extension", "help", "extension")
        data = self.root / "data"
        base = self.root / "base"
        repo = self.root / "repo"
        extension = self.root / "extension"
        self.command("create", "infobase", "create", f"--data={data}")
        self.command("export-empty", "config", "export", f"--data={data}", base)
        add_module(base)
        write_source(base / "CommonModules/PatchProbe.xml", module_xml())
        write_source(base / MODULE_PATH, BASE_CODE)
        write_source(base / "Ext/ManagedApplicationModule.bsl", f'''// Record the effective value and close the fixture client.
Procedure OnStart()
 Writer = New TextWriter("{self.root}/runtime.txt", TextEncoding.UTF8);
 Writer.Write(String(PatchProbe.Value()));
 Writer.Close();
 Exit();
EndProcedure
''')
        shutil.copytree(base, repo)
        self.git("git-init", "-c", "init.templateDir=", "init", "--initial-branch=t42-fixture")
        self.git("git-add-base", "add", ".")
        self.git("git-commit-base", "commit", "-m", "test: synthetic base")
        base_oid = self.git("git-base", "rev-parse", "HEAD").decode().strip()
        write_source(repo / MODULE_PATH, HEAD_CODE)
        self.git("git-add-head", "add", MODULE_PATH)
        self.git("git-commit-head", "commit", "-m", "test: synthetic method change")
        head_oid = self.git("git-head", "rev-parse", "HEAD").decode().strip()
        changed = self.git("git-diff", "diff", "--name-only", base_oid, head_oid).decode().splitlines()
        head_code = self.git("git-blob", "show", f"{head_oid}:{MODULE_PATH}").decode("utf-8-sig").replace("\r\n", "\n")
        if changed != [MODULE_PATH] or head_code != HEAD_CODE:
            raise RuntimeError("Only the exact synthetic method delta is accepted")
        self.command("import-base", "config", "import", f"--data={data}", base)
        self.command("apply-base", "config", "apply", f"--data={data}")
        self.runtime("runtime-base", data, 1)
        self.command("create-extension", "extension", "create", f"--data={data}",
                     "--name=EskaPatch", "--name-prefix=EskaPatch_", "--purpose=patch")
        self.command("export-extension", "config", "export", f"--data={data}", "--extension=EskaPatch", extension)
        add_module(extension)
        write_source(extension / "CommonModules/PatchProbe.xml", module_xml(adopted=True))
        patch_code = head_code.replace("Function Value() Export", '&Around("Value")\nFunction EskaPatch_Value()')
        write_source(extension / MODULE_PATH, patch_code.replace("&Around", "&Instead"))
        self.command("import-invalid-bsl", "config", "import", f"--data={data}", "--extension=EskaPatch", extension)
        self.command("check-invalid-bsl", "config", "check", f"--data={data}", "--extension=EskaPatch")
        self.designer("reject-invalid-bsl", data, "/CheckConfig", "-Server", "-ExternalConnection",
                      "-Extension", "EskaPatch", expected=101)
        write_source(extension / MODULE_PATH, patch_code)
        self.command("import-patch", "config", "import", f"--data={data}", "--extension=EskaPatch", extension)
        self.command("check-patch", "config", "check", f"--data={data}", "--extension=EskaPatch")
        self.command("apply-patch", "config", "apply", f"--data={data}", "--extension=EskaPatch")
        self.designer("check-bsl", data, "/CheckConfig", "-Server", "-ExternalConnection", "-Extension", "EskaPatch")
        candidate = self.root / "candidate.cfe"
        self.command("save", "config", "save", f"--data={data}", "--extension=EskaPatch", candidate)
        fresh = self.root / "fresh"
        self.command("create-fresh", "infobase", "create", f"--data={fresh}")
        self.command("import-fresh", "config", "import", f"--data={fresh}", base)
        self.command("apply-fresh", "config", "apply", f"--data={fresh}")
        self.command("load-cfe", "config", "load", f"--data={fresh}", "--extension=EskaPatch", candidate)
        self.command("apply-cfe", "config", "apply", f"--data={fresh}", "--extension=EskaPatch")
        self.designer("can-apply", fresh, "/CheckCanApplyConfigurationExtensions", "-Extension", "EskaPatch")
        self.runtime("runtime-safe-mode", fresh, 1)
        self.command("disable-fixture-safe-mode", "extension", "update", f"--data={fresh}",
                     "--name=EskaPatch", "--safe-mode=no")
        self.runtime("runtime-patch", fresh, 2)
        self.command("disable-fixture-patch", "extension", "update", f"--data={fresh}",
                     "--name=EskaPatch", "--active=no")
        self.runtime("runtime-disabled", fresh, 1)
        self.command("reactivate-fixture-patch", "extension", "update", f"--data={fresh}",
                     "--name=EskaPatch", "--active=yes")
        self.runtime("runtime-reactivated", fresh, 2)
        roundtrip = self.root / "roundtrip"
        self.command("roundtrip", "config", "export", f"--data={fresh}", "--extension=EskaPatch", roundtrip)
        descriptor = minidom.parseString((roundtrip / "CommonModules/PatchProbe.xml").read_bytes())
        for tag, expected in [("ObjectBelonging", "Adopted"), ("ExtendedConfigurationObject", MODULE_ID)]:
            if descriptor.getElementsByTagName(tag)[0].firstChild.data != expected:
                raise RuntimeError(f"Round-trip lost {tag}")
        if (roundtrip / MODULE_PATH).read_text(encoding="utf-8-sig") != patch_code:
            raise RuntimeError("Round-trip changed the patch method")
        missing = self.root / "missing"
        self.command("create-missing", "infobase", "create", f"--data={missing}")
        self.command("load-missing", "config", "load", f"--data={missing}", "--extension=EskaPatch", candidate)
        self.command("check-missing", "config", "check", f"--data={missing}", "--extension=EskaPatch")
        self.command("reject-missing", "config", "apply", f"--data={missing}", "--extension=EskaPatch", expected=1)
        if candidate.stat().st_size == 0:
            raise RuntimeError("Empty artifact")
        candidate.rename(self.root / "patch.cfe")
        summary = {"platform": version, "base": base_oid, "head": head_oid,
                   "artifact": "patch.cfe", "runtime_values": [1, 1, 2, 1, 2],
                   "scope": "synthetic server-call common module, one constant-return method"}
        (self.root / "result.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
        print(json.dumps(summary, indent=2), flush=True)


def main():
    """Require explicit platform executables; retain evidence in the owned playground directory."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ibcmd", required=True, help="Path to ibcmd in the selected runner")
    parser.add_argument("--client", required=True, help="Path to 1cv8 in the selected runner")
    parser.add_argument("--distrobox", help="Optional existing container name")
    Probe(parser.parse_args()).execute()


if __name__ == "__main__":
    main()
