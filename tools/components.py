#!/usr/bin/env python3
"""Selective ESKA component installation, independent of the optional shell (Python 3.11+)."""

import argparse
from contextlib import contextmanager
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import urllib.parse
import zipfile

from component_package import (Failure, MAX_ARCHIVE, MAX_MANIFEST, TOOLS, contract, hex_digest,
                               json_bytes, keys, load, native_target, parse_json, probe, relative, require, unpack, version)
from component_installation import inspect, install, recover, remove

MESSAGES = {
    "en-US": {
        "help": "Usage: python3 components.py COMMAND [OPTIONS]\nCommands: install, remove, status, inspect, recover\n--component TOOL  --root DIRECTORY  --check  --force\nInstall: --manifest FILE_OR_HTTPS [--manifest-sha256 SHA256]\n     or: --archive FILE_OR_HTTPS --sha256 SHA256\nInspect: --executable FILE; recover: --transaction DIRECTORY\n--lang en-US|ru-RU  --format human|json  --help\nOnly receipt-owned files can be replaced or removed. --force allows a managed downgrade or changed build of the same version.\n",
        "arguments": "Invalid arguments; run with --help",
        "manifest": "Invalid installation manifest", "incompatible": "Incompatible component contract",
        "version": "Invalid stable release version", "target": "Unsupported compilation target",
        "checksum": "Checksum mismatch", "size": "Size limit exceeded", "source": "Invalid package source",
        "path": "Invalid installation path", "archive": "Invalid component archive",
        "modified": "Managed file changed since installation", "installation": "Unknown installation method",
        "probe": "Executable contract probe failed", "probe-timeout": "Contract probe timed out",
        "probe-output": "Contract probe exceeded its output limit", "busy": "Another installation is running",
        "unmanaged": "A user-owned file would be replaced", "space": "Insufficient free disk space",
        "downgrade": "Downgrade requires --force", "same-version": "Changed build of the same version requires --force",
        "recovery-required": "Recover the interrupted transaction first", "rollback": "Rollback needs explicit recovery; backups retained",
        "rollback-conflict": "Recovery found changed files; backups retained", "cancelled": "Operation cancelled",
        "io": "Installation I/O failed", "installed": "Installed", "updated": "Updated", "removed": "Removed",
        "not-installed": "Not installed", "up-to-date": "Up to date", "install-available": "Installation available",
        "update-available": "Update available", "remove-available": "Removal available", "recovered": "Recovered",
        "custom-binary": "Custom executable verified; its files remain user-owned",
    },
    "ru-RU": {
        "help": "Использование: python3 components.py КОМАНДА [ПАРАМЕТРЫ]\nКоманды: install, remove, status, inspect, recover\n--component ИНСТРУМЕНТ  --root КАТАЛОГ  --check  --force\nУстановка: --manifest ФАЙЛ_ИЛИ_HTTPS [--manifest-sha256 SHA256]\n      или: --archive ФАЙЛ_ИЛИ_HTTPS --sha256 SHA256\nСвой бинарник: --executable ФАЙЛ; восстановление: --transaction КАТАЛОГ\n--lang en-US|ru-RU  --format human|json  --help\nЗаменяются и удаляются только файлы, подтверждённые receipt. --force разрешает понижение управляемой версии или другую сборку той же версии.\n",
        "arguments": "Некорректные параметры; запустите с --help",
        "manifest": "Некорректный manifest установки", "incompatible": "Несовместимый контракт компонента",
        "version": "Некорректная стабильная версия", "target": "Неподдерживаемая целевая платформа",
        "checksum": "Контрольная сумма не совпала", "size": "Превышен допустимый размер", "source": "Некорректный источник пакета",
        "path": "Некорректный путь установки", "archive": "Некорректный архив компонента",
        "modified": "Управляемый файл изменён после установки", "installation": "Неизвестный способ установки",
        "probe": "Не удалось проверить контракт executable", "probe-timeout": "Истекло время проверки контракта",
        "probe-output": "Ответ проверки контракта превышает допустимый размер", "busy": "Уже выполняется другая установка",
        "unmanaged": "Замена затронула бы пользовательский файл", "space": "Недостаточно свободного места",
        "downgrade": "Для понижения версии нужен --force", "same-version": "Для другой сборки той же версии нужен --force",
        "recovery-required": "Сначала восстановите прерванную транзакцию", "rollback": "Нужно явное восстановление; резервные копии сохранены",
        "rollback-conflict": "При восстановлении обнаружены изменённые файлы; резервные копии сохранены", "cancelled": "Операция отменена",
        "io": "Ошибка ввода-вывода при установке", "installed": "Установлен", "updated": "Обновлён", "removed": "Удалён",
        "not-installed": "Не установлен", "up-to-date": "Установлена актуальная версия", "install-available": "Доступна установка",
        "update-available": "Доступно обновление", "remove-available": "Доступно удаление", "recovered": "Восстановлен",
        "custom-binary": "Свой executable проверен; его файлы остаются пользовательскими",
    },
}


class Parser(argparse.ArgumentParser):
    """Expose only our localized errors, keeping argparse's English diagnostics off product streams."""
    def error(self, message):
        """CLI parsing failures use one stable code in both supported locales."""
        raise Failure("arguments")


def option(argv, name, default):
    """Bootstrap output preferences without interpreting any argument as shell code."""
    value = default
    for index, item in enumerate(argv):
        if item.startswith(name + "="):
            value = item[len(name) + 1:]
        elif item == name and index + 1 < len(argv):
            value = argv[index + 1]
    return value


@contextmanager
def cancellation():
    """Convert terminal cancellation to an exception so owned probes and transactions unwind."""
    def stop(_number, _frame):
        """A signal handler only initiates normal cleanup."""
        raise Failure("cancelled")
    previous = {}
    try:
        for name in ("SIGINT", "SIGTERM", "SIGHUP"):
            number = getattr(signal, name, None)
            if number is not None:
                previous[number] = signal.signal(number, stop)
        yield
    finally:
        for number, handler in previous.items():
            signal.signal(number, handler)


def archive(args):
    """A release index chooses exactly one component; URL indices must be explicitly checksum-pinned."""
    require(bool(args.manifest) != bool(args.archive), "arguments")
    if args.archive:
        require(args.sha256 is not None and args.manifest_sha256 is None, "arguments")
        hex_digest(args.sha256)
        data = load(args.archive, MAX_ARCHIVE, args.sha256)
        value = unpack(data, None)
        require(args.component is None or value["tool"] == args.component, "incompatible")
        return data, args.sha256
    require(args.component in TOOLS and args.sha256 is None, "arguments")
    parsed = urllib.parse.urlsplit(args.manifest)
    require(parsed.scheme != "https" or args.manifest_sha256 is not None, "checksum")
    value = parse_json(load(args.manifest, MAX_MANIFEST, args.manifest_sha256))
    keys(value, ("schema_version", "version", "target", "components"))
    require(type(value["schema_version"]) is int and value["schema_version"] == 1 and isinstance(value["components"], dict)
            and set(value["components"]).issubset(TOOLS) and args.component in value["components"], "manifest")
    version(value["version"])
    entry = value["components"][args.component]
    keys(entry, ("archive", "sha256", "size", "contract"))
    require(len(relative(entry["archive"])) == 1 and entry["archive"].endswith(".zip"), "path")
    contract(entry["contract"], args.component, value["target"])
    require(value["version"] == entry["contract"]["version"] and type(entry["size"]) is int and 0 < entry["size"] <= MAX_ARCHIVE, "manifest")
    source = urllib.parse.urljoin(args.manifest, entry["archive"]) if parsed.scheme == "https" else str(Path(args.manifest).parent / entry["archive"])
    data = load(source, entry["size"], entry["sha256"])
    require(len(data) == entry["size"] and unpack(data, None)["contract"] == entry["contract"], "manifest")
    return data, entry["sha256"]


def execute(args):
    """Selection never falls back to another global installation or overwrites an explicit custom binary."""
    root = args.root.expanduser().absolute()
    require(not root.is_symlink(), "path", root)
    require(args.command == "install" or not any((args.archive, args.manifest, args.sha256, args.manifest_sha256, args.force)), "arguments")
    require(args.command == "inspect" or args.executable is None, "arguments")
    require(args.command == "recover" or args.transaction is None, "arguments")
    if args.command == "install":
        data, checksum = archive(args)
        return install(root, data, checksum, args.check, args.force)
    if args.command in ("remove", "status"):
        require(args.component in TOOLS, "arguments")
        return remove(root, args.component, args.check) if args.command == "remove" else inspect(root, args.component)
    if args.command == "inspect":
        require(args.executable is not None and not args.check, "arguments")
        path = args.executable.expanduser().resolve(strict=True)
        with tempfile.TemporaryDirectory(prefix="eska-component-probe-", dir=os.environ.get("ESKA_TEST_ROOT")) as temporary:
            value = probe(path, Path(temporary))
        contract(value, args.component or value.get("tool"))
        native_target(value["target"])
        return {"schema_version": 1, "status": "custom-binary", "tool": value["tool"], "contract": value,
                "method": "custom", "executable": str(path)}
    require(args.command == "recover" and args.transaction is not None and not args.check and args.component is None, "arguments")
    return recover(root, args.transaction.expanduser().absolute())


def main(argv=None):
    """Keep machine output locale-independent and human messages available in both supported locales."""
    argv = sys.argv[1:] if argv is None else argv
    language = option(argv, "--lang", "ru-RU" if os.environ.get("LANG", "").startswith("ru") else "en-US")
    language = language if language in MESSAGES else "en-US"
    output_format = option(argv, "--format", "human")
    if output_format == "json" and hasattr(sys.stdout, "reconfigure"):
        sys.stdout.reconfigure(encoding="utf-8")
    try:
        parser = Parser(add_help=False)
        parser.add_argument("command", nargs="?", choices=("install", "remove", "status", "inspect", "recover"))
        parser.add_argument("--component", choices=TOOLS)
        parser.add_argument("--root", type=Path, default=Path.home() / ".eska")
        for name in ("archive", "manifest", "sha256", "manifest-sha256"):
            parser.add_argument("--" + name)
        parser.add_argument("--executable", type=Path)
        parser.add_argument("--transaction", type=Path)
        parser.add_argument("--lang", choices=tuple(MESSAGES))
        parser.add_argument("--format", choices=("human", "json"))
        for name in ("check", "force", "help"):
            parser.add_argument("--" + name, action="store_true")
        args = parser.parse_args(argv)
        if args.help:
            print(MESSAGES[language]["help"], end="")
            return 0
        require(args.command is not None, "arguments")
        with cancellation():
            result = execute(args)
        if output_format == "json":
            print(json_bytes(result).decode("utf-8"), end="")
        else:
            print(f"{MESSAGES[language][result['status']]}: {result['tool']} {result.get('version', '')}".rstrip())
        return 0
    except (Failure, OSError, ValueError, KeyError, zipfile.BadZipFile, subprocess.SubprocessError) as error:
        code = getattr(error, "code", "io")
        detail = error.detail if isinstance(error, Failure) else str(getattr(error, "filename", "") or "")
        if output_format == "json":
            print(json_bytes({"schema_version": 1, "status": "error", "code": code, "detail": detail}).decode("utf-8"), end="")
        else:
            print(MESSAGES[language].get(code, MESSAGES[language]["io"]) + (f": {detail}" if detail else ""), file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
