# Самостоятельный eska-project, EF-10

Экспериментальный `eska-project` выполняет операции проекта без установленной
оболочки `eska`, платформы, Git или IDE. Source format и правила поддержки
принадлежат `project_core`; CLI отвечает за аргументы, выбор member, локали,
представление и exit code. Он не распаковывает CF и не создаёт Git repository.

Бинарник пока собирается отдельно, до комплектной поставки EF-21:

```sh
cargo build --no-default-features --features project-cli --bin eska-project
```

`tool-cli` включает только общую CLI presentation; `project-cli` выбирает
проектный бинарник. Source-only library по-прежнему работает без CLI features.
Общие Locale/Localizer перенесены из приложения; прежний import API приложения
сохранён через reexport. Минимальный tool не включает platform dictionaries,
Git engine, native runner или IDE runtime. Новых dependencies не добавлено.

## Команды

```sh
eska-project import designer-xml new-project --name demo
eska-project import designer-xml another-project --name demo \
  --platform-version 8.3.27.2325
eska-project import designer-xml readable-project --name demo --layout readable
eska-project --project-dir old-project migrate new-project \
  --accepted-base-sha256 "$APPROVED_OLD_BASE_SHA256" --format json
eska-project --project-dir old-project migrate new-project \
  --accepted-base-sha256 "$APPROVED_OLD_BASE_SHA256" --write --format json
eska-project --project-dir old-workspace migrate new-workspace --workspace \
  --accepted-bases approved-bases.json --write --format json
eska-project --project-dir new-project inspect --format json
eska-project --project-dir new-project prepare \
  --accepted-base-sha256 "$APPROVED_BASE_SHA256" --format json
eska-project --project-dir new-project transport
eska-project --project-dir new-project transport --write
eska-project capabilities --format json
```

`import` читает Designer XML и создаёт исключительно новый каталог:
`src`, `.eska/accepted`, `.gitattributes`, `.gitignore`, `eska.toml`.
Manifest публикуется последним. Ошибка удаляет только каталог, захваченный
данной операцией; существующий каталог не перезаписывается. Имя проекта
не переименовывает нативную конфигурацию или UUID. Platform version только
проверяется по общим правилам и записывается; поиска/запуска платформы нет.
Если версия не передана, пустой override не записывается: будущий member
наследует настройки workspace. `--freeze UUID` можно повторять;
неизвестный UUID отклоняет создание. Profile пока configuration / Designer 2.20.

`--layout family` либо отсутствие параметра сохраняет layout 1/descriptor 1.
[Явный `--layout readable`](readable-project-import.md) создаёт именованные
свойства, самостоятельные descriptors и физические resources: layout 3/descriptor 2.
Оба варианта используют общее ядро, поддержку/freeze и самостоятельный pin.
Это не миграция существующего проекта; промежуточные layout API не становятся
дополнительными CLI choices.

[`migrate`](project-migration.md) переносит standalone-проект в новую readable
копию: рабочие исходники и pinned base используют отдельные expected-byte планы.
Preview не пишет файлы; `--write` публикует или возобновляет matching output,
затем manifest. Settings/transport сохраняются побайтно, возвращённый новый
pin нужно одобрить отдельно. Исходный проект и CI approval не переключаются;
Git/прочие файлы проекта не копируются. [Workspace migration](workspace-migration.md)
включается явно через `--workspace --accepted-bases`: точные root/member settings
сохраняют наследование, каждый member требует свой approved pin, destinations
проверяются до публикации первого member. Root manifest записывается последним;
общий receipt блокирует частично опубликованные проекты. Выборочный перенос
member в standalone по-прежнему отказывает.

`import` возвращает кандидат на принятую базу и её SHA-256. CI получает
одобренную сумму отдельно; editable TOML не хранит approval digest.
`prepare` требует `--accepted-base-sha256` либо `ESKA_ACCEPTED_BASE_SHA256`.
Явный флаг имеет приоритет. Принимаются 64 hex characters, spelling
нормализуется к lowercase. Отсутствующий, неверный или не совпавший pin
не создаёт XML-поколение.

`prepare` использует текущие сохранённые входы и private `PreparedSource`.
По умолчанию любое исключение запрещённой правки означает отказ с полным
списком diagnostics. `--allow-exclusions` явно включает восстановление из
базы/исключение новых частей и возвращает все exclusions; рабочие YAML/BSL
не переписываются. По умолчанию кеш находится в `.eska/generations` выбранного
проекта. `--generations` задаёт project-relative или абсолютный каталог.
Удаление кеша воспроизводит поколение по тому же набору входов. Старый XML
не служит fallback. JSON содержит отчёт, а не передаваемое разрешение на сборку;
сборщик повторяет protected prepare и проверяет собственный snapshot.

`inspect` читает настройки и для ESKA только manifest/configuration snapshot:
Name, UUID, owner count и SHA-256 текста. Полный набор owners, база и payloads
не читаются. `validation = "manifest_and_configuration_only"` не объявляет
полную валидность проекта. Для Designer XML возвращаются настройки и
`metadata = null`. Общая discovery/selection сохраняет поддержку старого формата.

Команды над существующим проектом выбирают один member через `--project NAME`
или текущий каталог member. На workspace root без выбора возвращается отказ,
кроме явной миграции всего workspace.
Настройки и пути базы относятся к выбранному member, platform inheritance
берётся из общей модели. YAML других member не открываются.

`transport` показывает точное добавление к `.gitattributes`; `--write` публикует
[expected-byte plan](git-transport.md). Он не проверяет effective Git attributes
и сообщает `effective_git_policy_verified = false`. `capabilities` перечисляет
только текущие команды, профиль и ограничения. Структурное редактирование,
baseline update и unpack нативных артефактов не добавлены.
Layout migration переносит прежнюю базу; это не baseline update с новой политикой.

Пути machine interface должны представляться lossless UTF-8. Неподдержанный
root encoding отклоняется до создания проекта. Нативные относительные пути
дополнительно проверяются общей моделью `SourcePath`.

## Представление

Общие `--lang ru|en`, `--format human|json`, `--project-dir`, `--help`,
`--version` доступны независимо от оболочки. Help, human results и ошибки
локализованы сразу для обеих локалей. JSON stdout не содержит human message;
при отказе локализованный текст идёт в stderr. Успех/help/version — exit 0,
ошибка аргументов, источников или publication — exit 1.

Tool envelope имеет schema 1:

```json
{"schema_version":1,"tool":"eska-project","command":"inspect","status":"success","data":{}}
{"schema_version":1,"tool":"eska-project","command":"prepare","status":"error","error":{"code":"accepted_base_pin_required","location":[],"diagnostics":null}}
```

При ошибке parsing `command = null`; `diagnostics.parser` сохраняет machine
identifier категории Clap. Core failures сохраняют `code` и `location`;
strict prepare возвращает полный массив exclusions в `error.diagnostics`.
Запрещённая [перестановка детей](child-order.md) использует operation
`{"kind":"reorder"}`; strict отказывает до публикации, а явный
`--allow-exclusions` восстанавливает порядок с диагностикой.
Для запрещённых [новых значений перечисления](enum-value-creation.md) используется
существующая operation `{"kind":"create"}`. Новые параметры не добавлены.
Она же используется для запрещённого [создания Attribute](attribute-creation.md)
у принятого Catalog/Document. Для [колонок существующей табличной части](column-creation.md)
UUID и namespace context относятся к принятой табличной части того же владельца;
используются та же operation и обычные параметры prepare. Общий selector
проверяет новый field profile. Имена, QName/Type targets и неизвестная native
семантика не обходятся через
`--allow-exclusions`.
Запрещённое [удаление EnumValue](enum-value-deletion.md) восстанавливается с
`{"kind":"delete"}`. Оставшиеся typed/preserved XML зависимости отказывают
до публикации, включая режим `--allow-exclusions`.
При успехе prepare возвращаются `xml`, `baseline`, `generation`, input/native
revision, `source_map_sha256` и exclusions. Inspection, prepare и refusals
для одинаковых входов дают тот же JSON при ru/en. Это отдельный tool protocol;
прежние JSON-схемы приложения и IDE protocol не изменены.

Import дополнительно возвращает `data.source_layout = "family" | "readable"`.
Capabilities добавляет `import_layouts = ["family", "readable"]`,
`source_layout_versions = [1, 2, 3]` и `descriptor_schema_versions = [1, 2]`.
Tool envelope и прежние поля, включая `source_schema_version = 1`, сохранены;
для выбора поддерживаемых версий используются новые списки.
Capabilities также содержит `migrate`, `migration_layouts = ["readable"]` и
`migration_scopes = ["standalone"]`. Schema 1 для новой команды описана в
[контракте миграции](project-migration.md#machine-interface).

## Проверки

Актуальная проверка обоих layout choices, 12 CLI integration tests и читаемой
C1 приведена в [одиннадцатой части EF-24](readable-project-import.md).
Актуальная миграция, 16 CLI integration tests и сохранение независимой базы —
в [двенадцатой части](project-migration.md).
Ниже сохранены результаты первоначальной EF-10.

Три core tests проверяют полное создание, сохранение name/UUID/BSL/каталогов,
offline mode, exclusive destination и cleanup при ошибке. Девять integration
tests запускают реальный независимый бинарник: help/capabilities обеих локалей,
import/inspect/prepare, cache recreation, pin flag/environment/precedence,
bad YAML без XML fallback, strict frozen refusal и явное восстановление,
attributes preview/write, workspace selection/inheritance, argument/destination
errors и Unix non-UTF-8 cwd без создания проекта.

Полные fmt/check/strict Clippy/test/Rustdoc прошли; default application —
341 unit и 529 integration, minimal source/build — 147/163 unit,
minimal project CLI — 150 unit и 9 integration. 14 opt-in tests приложения
и три в каждом варианте ядра пропущены. Minimal normal graph содержит 66 packages
без gix engine, HTTP, Tokio, Ctrl-C runner, TTY, updater, архиваторов и IDE runtime.
`gix-validate` остаётся небольшим валидатором переносимых имён.
Release-size сравнение относится к поставке EF-21.

На собственной полной копии C1 отдельный бинарник выполнил import, inspect и
prepare при PATH без платформы, Git и оболочки. Все 8 778 нативных файлов и
8 077 каталогов сохранены, reader увидел 2 645 owners. Pin, input/native revision,
generation и source-map checksum совпали с предыдущей принятой сборкой 1С.
Настройка platform version 8.3.27.2325 сохранена; исходные байты не изменились.
Проверялось то же XML-поколение, уже принятое платформой; повторная сборка 1С,
применение к БД, клиент и полный Designer CheckConfig здесь не запускались.
