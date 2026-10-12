# Миграция workspace, девятнадцатая часть EF-24

`eska-project migrate --workspace` переносит весь явно объявленный workspace в
новую readable копию. Root manifest и настройки каждого member копируются
побайтно: inherited build/workflow defaults, explicit platform overrides,
комментарии и относительные source/base paths сохраняются. Исходный workspace
и его CI approval не переключаются.

```sh
eska-project --project-dir old-workspace migrate new-workspace --workspace \
  --accepted-bases approved-bases.json --format json
eska-project --project-dir old-workspace migrate new-workspace --workspace \
  --accepted-bases approved-bases.json --write --format json
```

Запуск разрешён из root или вложенного каталога любого member. Destination и
путь pin document разрешаются относительно текущего каталога процесса; родитель
destination должен существовать. Preview не создаёт каталогов.

## Независимая карта pins

Каждый member требует свою отдельно одобренную сумму прежней базы. Документ
`--accepted-bases` имеет [schema 1](workspace-accepted-bases.schema.json):

```json
{
  "schema_version": 1,
  "members": {
    "application": "<approved SHA-256 of application base>",
    "reference": "<approved SHA-256 of reference base>"
  }
}
```

Placeholder нужно заменить на 64 hex characters. Имена совпадают с `[project].name`,
а не с native Name конфигурации или именами каталогов. Duplicate keys, unknown
fields, unsupported schema, неверные names/checksums, недостающие и лишние
members отклоняются. Hex spelling нормализуется к lowercase. Размер документа
ограничен 1 MiB.

`--accepted-base-sha256`, `ESKA_ACCEPTED_BASE_SHA256` и editable TOML не заменяют
полную карту. `--workspace` несовместим с `--project` и standalone pin;
`--accepted-bases` требует `--workspace`. Возвращённые новые pins каждого member
нужно отдельно одобрить для CI. Одобрение старой базы не разрешает новую базу.

## Общие планы и публикация

`WorkspaceAcceptedRefs` валидирует pin document; `prepare_readable_workspace`
использует обычную discovery и тот же resolver наследования. Каждый member
получает прежние независимые source/base планы из [standalone migration](project-migration.md).
Working values не принимаются в authority: новая база строится только из
проверенных старых blobs. Доступны смешанные family/readable inputs и layout
no-op с прежним pin для уже читаемого member.

Все members должны быть ESKA configuration в текущем поддержанном профиле.
Designer XML/CF напрямую, новые CFE/EPF/ERF profiles и выборочное отсоединение
member здесь не добавлены. Неподдержанный member отклоняет весь план до записи.

Root/member `eska.toml`, существующие `.gitattributes`/`.gitignore` сохраняются
побайтно. Отсутствие transport files тоже закреплено. Git state, прочие файлы
проекта, artifacts и generations не копируются; effective Git policy не
проверяется. Состав берётся только из root manifest. Aliases, case-colliding
ancestors, nested/overlapping member paths и чужие outputs отклоняются.

Outer receipt `.eska-workspace-layout.json` связывает exact root settings,
пути members, отдельные source/base revisions и pins. Перед первым member
проверяются все destinations. Каждый member публикуется через прежний exclusive
publisher; root manifest записывается после всех members. Пока outer receipt
существует, `EskaSource` блокирует вложенный проект, даже если member manifest
уже записан, а root manifest ещё отсутствует. Inspect/prepare/build через этот
вход не используют частичный workspace. Низкоуровневый reader не является
подтверждением готовности всей публикации.

Повтор той же команды из неизменённого оригинала возобновляет matching output.
Завершённая exact копия возвращает `unchanged`; foreign/edited output и
изменение root/member settings, working source или accepted bytes отказывают
без замены файлов. Незавершённый собственный output сохраняется для recovery.
Гарантия транзакции при потере питания не заявлена; нужен filesystem с hard links.

Member previews показывают изменения/pins, но не открывают независимую
публикацию внутреннего project plan. Публиковать можно только aggregate workspace.
Новых dependencies, source schema или версий проектов не добавлено.

## Machine interface

Tool envelope и команда `migrate` остаются schema 1; standalone report сохраняется.
Workspace report добавляет `scope = "workspace"`, absolute root/destination,
target layout 3/descriptor 2, `layout_changed`, `written`, `publication`,
`copied_workspace_files` и `members` в declaration order. Каждый member содержит
`name`, portable `path`, absolute `destination`, `previous_baseline`, `baseline`,
`layout_changed`, отдельные `source_changes`/`accepted_changes` и
`copied_project_files`. Preview имеет `publication = null`; публикация возвращает
`created | resumed | unchanged`. `ci_pin_updated` и
`effective_git_policy_verified` всегда false.

Capabilities: `migration_scopes = ["standalone", "workspace"]`,
`workspace_pin_schema_version = 1`. JSON не зависит от locale; help/human/errors
доступны RU/EN. Отказ отдельного member включает его стабильное имя в location.

## Проверки

Core tests покрывают mixed layouts/pins, working/base separation, frozen policy,
native parity, inheritance/overrides, exact copy/no-op/repeat, interruption и
блокировку consumers, foreign output/receipt/ancestor, stale inputs, unsupported
members, overlap и aliases/case collisions. CLI processes проверяют preview/write,
root/nested selection, invalid approval, localized help/human и readiness.

Прошли 16 root/14 SDK/5 adapter gates и оба release build:
515 unit + 563 integration, 24 opt-in пропущены; minimal project —
322 unit + 29 CLI tests, SDK — 359, adapter — 70 + 12. Добавлены
10 core и три CLI tests. Проверены fmt/check/strict Clippy, default/all-features,
минимальные core/project/build features и Rustdoc; dependencies и версии прежние.

Реальный workspace из двух C1 configuration members с разными freeze/pins
проверен отдельным release CLI при PATH без платформы, Git и оболочки. Один
member использовал family layout, второй readable; две независимые FillValue
правки сохранены только в working sources. Root/nested preview не записывал
destination, exact repeat вернул `unchanged`; неправильная карта pins отказала
в RU/EN до публикации. Exact root/member settings сохранили наследование.

Protected native result каждого member совпал с собственным независимым
контролем по 8 778 файлам/8 077 каталогам, без exclusions. Для каждого member
3 595 descriptors/8 330 объектов совпали в saved/unsaved All/Summary. Обе
accepted bases восстановлены до исходного C1 побайтно; отдельные freezes
сохранены. Working edits не попали в authority.

Оба members и оба контроля собраны в CF и выгружены через ESKA на
1С 8.3.27.2325. Для каждого сравнения 8 769 файлов/8 064 каталога равны,
кроме 7 011 ConfigDumpInfo.configVersion; четыре checksums проверены. Native,
old/new sources/bases, settings, approval и версии сохранены.
[Обезличенные результаты](workspace-migration-results.json).

Это два CF members, без приёмки новых CFE/EPF/ERF, клиента или рабочей ИБ.

EF-24 остаётся `IN_PROGRESS`: прочие structural/compound profiles ещё нужны.
