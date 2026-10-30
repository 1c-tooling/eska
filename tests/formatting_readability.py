#!/usr/bin/env python3
"""Check BSL readability and receiver groups through CLI/facade/RPC and an optional user snapshot."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

# Imported acceptance helpers must not leave bytecode artifacts in the source tree.
sys.dont_write_bytecode = True
from formatting_acceptance import Rpc, path


SAMPLE = '''// Проверяет оформление групп, строк и условий.
Функция Проверить(Объект, Снимок, ДатаОкончания)
Регистрация=Создать();
Регистрация.Вид=1;
Регистрация.Наименование="Проверка";
Команда=Регистрация.Команды.Добавить();
Команда.Представление=Регистрация.Наименование;
Команда.Идентификатор="Проверить";
ДанныеПотока    = Новый Структура(
"Договоры, Объекты, ДатаОкончания, ИдентификаторГлавногоЗадания",
Договоры,
Объекты,
Параметры.ДатаОкончания,
Контекст.ИдентификаторГлавногоЗадания
);
Обертка=Подготовить(ДанныеПотока);
Запрос.Текст="ВЫБРАТЬ
 | Объекты.Договор КАК Договор, Объекты.Ссылка КАК Ссылка
 |ИЗ ВТ_ОбъектыКПометке КАК Объекты";
Выборка      = Запрос.Выполнить().Выбрать();
РезультатВыполнения = "Операция завершилась с ошибкой или была отменена. Завершенные цепочки сохранены.
  |Подробности: журнал регистрации, события ПометкаДубликатовДоговоров.";
Если Снимок.Количество() <> Текущие.Количество() Тогда

Возврат Ложь;

КонецЕсли;
Возврат Объект.Владелец
= Снимок.Контрагент
И Объект.Номер
= Снимок.Номер
И Объект.Дата
<= ДатаОкончания;
КонецФункции
'''


def project(directory, source):
    """Own a minimal Designer project with a known editable root module and the real ESKA preset."""
    module = directory / "src/Ext/ManagedApplicationModule.bsl"
    module.parent.mkdir(parents=True)
    (directory / "eska.toml").write_text("[project]\ntype='configuration'\n[format]\nprofile='eska'\n")
    (directory / "src/Configuration.xml").write_text(
        "<MetaDataObject xmlns='http://v8.1c.ru/8.3/MDClasses'>"
        "<Configuration uuid='11111111-1111-1111-1111-111111111111'><Properties>"
        "<Name>Readability</Name></Properties><ChildObjects/></Configuration></MetaDataObject>"
    )
    (directory / "src/Ext/ParentConfigurations.bin").write_text("{6,0,0,0,0,0}")
    module.write_bytes(source)
    return module


def verify_examples(value):
    """Assert user-visible layout across independent examples rather than implementation details."""
    text = value.decode("utf-8-sig").replace("\r\n", "\n")
    assert '\n\n\tКоманда = Регистрация.Команды.Добавить();\n' in text
    assert '\n\n\tДанныеПотока = Новый Структура(\n' in text
    assert '\n\t);\n\n\tОбертка = Подготовить(ДанныеПотока);' in text
    assert '\tЗапрос.Текст =\n\t\t"ВЫБРАТЬ\n\t\t| Объекты.Договор' in text
    assert '\n\n\tВыборка = Запрос.Выполнить().Выбрать();\n' in text
    assert '\tРезультатВыполнения =\n\t\t"Операция завершилась' in text
    assert '\n\t\t|Подробности: журнал регистрации, события ПометкаДубликатовДоговоров.";' in text
    assert 'Тогда\n\t\tВозврат Ложь;\n\tКонецЕсли;' in text
    assert '\tВозврат Объект.Владелец = Снимок.Контрагент\n\t\tИ Объект.Номер = Снимок.Номер\n\t\tИ Объект.Дата <= ДатаОкончания;' in text
    assert "\n\n\n" not in text


def main():
    """Keep all mutations inside a unique playground directory and record actual process results."""
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ("formatter", "facade", "project"):
        parser.add_argument("--" + name, required=True, type=Path)
    parser.add_argument("--source", type=Path)
    arguments = parser.parse_args()
    binaries = {name: getattr(arguments, name).resolve() for name in ("formatter", "facade", "project")}
    repository = Path(__file__).resolve().parents[1]
    playground = Path(os.environ.get("ESKA_TEST_ROOT", repository.parent / "eska-playground")).resolve()
    stand = Path(tempfile.mkdtemp(prefix="fmt-readability-", dir=playground))
    print(stand, flush=True)
    environment = dict(os.environ, ESKA_FMT_BINARY=str(binaries["formatter"]),
                       ESKA_BSL_ANALYZER=str(stand / "missing-analyzer"))
    samples = {"examples": b"\xef\xbb\xbf" + SAMPLE.replace("\n", "\r\n").encode()}
    fixtures = repository / "crates/eska-fmt/src/tests/preset/fixtures"
    samples["receiver-groups"] = b"\xef\xbb\xbf" + (fixtures / "receiver-groups.input.bsl").read_text().replace("\n", "\r\n").encode()
    if arguments.source:
        samples["ObjectModule"] = arguments.source.read_bytes()
    report = {"complete": False, "stand": str(stand), "samples": {}}

    def command(tool, locale, directory, mode, codes):
        """Require a finite deadline, expected exit code and clean machine-readable output."""
        prefix = ["fmt"] if tool == "facade" else []
        result = subprocess.run(
            [binaries[tool], "--lang", locale, "--project-dir", directory, *prefix,
             "src/Ext/ManagedApplicationModule.bsl", *mode, "--format", "json"],
            cwd=stand, env=environment, capture_output=True, timeout=35,
        )
        assert result.returncode in codes and not result.stderr, result.stderr.decode()
        return result.returncode, json.loads(result.stdout)

    for name, original in samples.items():
        directory = stand / name
        module = project(directory, original)
        (stand / f"{name}.original.bsl").write_bytes(original)
        locales, outputs, checks = {}, [], []
        for locale in ("ru-RU", "en-US"):
            module.write_bytes(original)
            check = command("formatter", locale, directory, ["--check"], (1,) if name != "ObjectModule" else (0, 1))
            assert check[1]["schema_version"] == 1
            assert check[0] == int(check[1]["files"][0]["changed"])
            checks.append(check[1])
            facade = command("facade", locale, directory, ["--check"], (check[0],))
            assert check == facade
            preview = command("formatter", locale, directory, ["--diff"], (0,))[1]
            assert preview["files"][0]["edits"] == check[1]["files"][0]["edits"]
            assert module.read_bytes() == original
            rpc = Rpc(binaries["project"], stand, environment)
            try:
                rpc.call("initialize", {"apiVersion": {"major": 1, "minor": 33},
                         "client": {"name": "readability-acceptance", "version": "1"}, "locale": locale})
                response = rpc.call("format/document", {"source": path(module), "version": "1",
                                    "text": original.decode(), "userReadOnly": False})
                assert response["plan"]["edits"] == check[1]["files"][0]["edits"]
                assert response["effective"] == check[1]["files"][0]["effective"]
                assert module.read_bytes() == original
                rpc.finish()
            finally:
                if rpc.child.poll() is None:
                    rpc.child.kill()
                    rpc.child.wait(timeout=5)
            applied = command("formatter", locale, directory, [], (0,))[1]
            formatted = module.read_bytes()
            assert applied["files"][0]["status"] == ("applied" if check[0] else "unchanged")
            stable = command("formatter", locale, directory, ["--check"], (0,))[1]
            assert stable["files"][0]["edits"] == []
            assert command("formatter", locale, directory, ["--diff"], (0,))[1]["files"][0]["diff"] is None
            assert formatted.startswith(b"\xef\xbb\xbf") == original.startswith(b"\xef\xbb\xbf")
            if b"\r\n" in original and b"\n" not in original.replace(b"\r\n", b""):
                assert b"\n" not in formatted.replace(b"\r\n", b"")
            if name == "examples":
                verify_examples(formatted)
            if name == "receiver-groups":
                expected = (fixtures / "receiver-groups.expected.bsl").read_text()
                assert formatted == b"\xef\xbb\xbf" + expected.replace("\n", "\r\n").encode()
            outputs.append(formatted)
            locales[locale] = {"initialExit": check[0], "finalExit": 0,
                               "cliFacadeRpcParity": True, "idempotent": True}
        assert outputs[0] == outputs[1]
        assert checks[0] == checks[1]
        (stand / f"{name}.formatted.bsl").write_bytes(outputs[0])
        report["samples"][name] = {"sourceSha256": hashlib.sha256(original).hexdigest(),
                                   "formattedSha256": hashlib.sha256(outputs[0]).hexdigest(),
                                   "locales": locales}
    if arguments.source:
        assert arguments.source.read_bytes() == samples["ObjectModule"], "User source changed during verification"
        report["userSourceUnchanged"] = True
    report["complete"] = True
    (stand / "report.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n")
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
