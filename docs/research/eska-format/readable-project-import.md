# Импорт читаемого проекта через CLI, одиннадцатая часть EF-24

`eska-project import --layout readable` создаёт новый проект с именованными
свойствами YAML и отдельными descriptors, модулями и ресурсами. Явный выбор
подключает существующий writer общей библиотеки, а не второй конвертер CLI.
Без параметра либо с `--layout family` остаётся прежнее семейное расположение.

```sh
eska-project import designer-xml new-project --name demo --layout readable \
  --platform-version 8.3.27.2325 --format json
eska-project --project-dir new-project inspect --format json
eska-project --project-dir new-project prepare \
  --accepted-base-sha256 "$APPROVED_BASE_SHA256" --format json
```

Вход — каталог Designer XML. Распаковка CF и запуск платформы относятся к
`eska-build`; import не ищет платформу, Git, оболочку или IDE. Команда создаёт
только новый каталог; существующий не перезаписывается. Platform version
проверяется и записывается в настройки, имя проекта не меняет native Name/UUID.

## Контракт и авторитет

Общее ядро `creation::import_readable_project` создаёт layout 3/descriptor 2,
accepted base и transport settings. Семейный import по-прежнему создаёт
layout 1/descriptor 1. Промежуточные версии 2 и 3 с legacy descriptors доступны
через существующие API, но отдельными CLI choices не становятся.

Нативные paths, UUID, порядок и байты ресурсов сохраняются. Значение свойства
имеет один editable источник; native details описывают bindings и transport.
`--freeze UUID` действует в обоих вариантах; неизвестный UUID отменяет создание.
Pin возвращается как кандидат для независимого принятия CI. Рабочие значения
существующего проекта не заменяют его базу и не меняют внешнее одобрение.

Manifest проекта публикуется последним. При ошибке удаляется только каталог,
захваченный текущим import. Общие prepare, reader, source map и protected export
используют тот же source format; восстановление forbidden edits не дублируется.
Это создание нового проекта, а не миграция существующего или повторный импорт.

Tool envelope сохраняет schema 1. Успешный import дополнительно возвращает
`data.source_layout = "family" | "readable"`. Capabilities добавляет
`import_layouts = ["family", "readable"]`, `source_layout_versions = [1, 2, 3]`
и `descriptor_schema_versions = [1, 2]`. Старые поля и ошибки сохранены;
`source_schema_version = 1` не заменяет новые списки поддерживаемых версий.
Help и ошибки есть на русском и английском, machine JSON не зависит от языка.

## Проверки

Прошли 15 root/14 SDK/5 adapter gates: fmt/check/strict Clippy/test/Rustdoc,
minimal core и отдельные CLI. Root — 436 unit + 552 integration, 24 opt-in
пропущены; minimal project CLI — 243 unit + 12 integration; SDK — 359,
adapter — 70 + 12. Проверены default/explicit family parity, оба языка,
readable import/inspect/prepare, freeze, native transport, exclusive destination
и cleanup после отказа. Новых dependencies и изменений версии нет.

На отдельной C1 независимый release-бинарник с PATH без платформы, Git и
оболочки создал 3 595 descriptors. Все они прошли readable JSON schema,
manifest — schema 3. Prepare обеих локалей восстановил исходные 8 778 файлов
и 8 077 каталогов побайтно, без exclusions; повторный prepare переиспользовал
generation. Pin совпал с import через библиотеку, source/base revisions и
настройка платформы сохранились. [Обезличенные результаты](readable-project-import-results.json).

Эта проверка не повторяет сборку CF или запуск клиента. Платформенный цикл
предыдущей части описан [отдельно](named-type-sets.md). Миграция проекта через
CLI, structural create/delete/rename, остальные составные домены, new/init и
приёмка CFE/EPF/ERF ещё нужны. EF-24 остаётся `IN_PROGRESS`.
