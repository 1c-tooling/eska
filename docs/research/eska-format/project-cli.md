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
eska-project import designer-xml queries-project --name demo --layout queries
eska-project --project-dir old-project migrate new-project \
  --accepted-base-sha256 "$APPROVED_OLD_BASE_SHA256" --format json
eska-project --project-dir old-project migrate new-project \
  --accepted-base-sha256 "$APPROVED_OLD_BASE_SHA256" --write --format json
eska-project --project-dir old-workspace migrate new-workspace --workspace \
  --accepted-bases approved-bases.json --write --format json
eska-project --project-dir current-project update-base incoming-project updated-project \
  --accepted-base-sha256 "$APPROVED_OLD_BASE_SHA256" \
  --incoming-base-sha256 "$APPROVED_NEW_BASE_SHA256" --format json
eska-project --project-dir current-project update-base incoming-project updated-project \
  --accepted-base-sha256 "$APPROVED_OLD_BASE_SHA256" \
  --incoming-base-sha256 "$APPROVED_NEW_BASE_SHA256" --write --format json
eska-project --project-dir current-workspace update-base incoming-workspace updated-workspace \
  --workspace --accepted-bases approved-old.json --incoming-bases approved-new.json \
  --write --format json
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
неизвестный UUID отклоняет создание. Designer 2.20 и фактический тип определяются
по native входу; импорт сохраняет root identity в настройках, reader и
защищённой материализации. `creation_project_types` сообщает четыре типа;
`import_layout_project_types` — ограничения каждого layout. Project core и
protected build processing/report приняты в [EF-14](external-projects.md).
Общий new/init остаётся EF-20. Анализ CFE с контекстом
основной конфигурации остаётся EF-18 и пока явно отказан.

`--layout family` либо отсутствие параметра сохраняет layout 1/descriptor 1.
[Явный `--layout readable`](readable-project-import.md) создаёт именованные
свойства, самостоятельные descriptors и физические resources: layout 3/descriptor 2.
[`--layout queries`](query-project-creation.md) создаёт полные CF/CFE/EPF/ERF
проекты с managed forms и отдельными plain `.sdbl`: layout 5/6, descriptor 2.
Family/Readable принимают configuration/extension; Queries — все четыре типа.
Варианты используют общее ядро, поддержку/freeze и самостоятельный pin.
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

[`update-base`](base-update.md) сравнивает старую базу, independently pinned
incoming accepted store и локальные изменения. Default preview показывает
точные изменения прав и конфликты; `--write` публикует отдельный standalone-проект
только без конфликтов. Прежнее дерево/база остаются доступны; рабочие edits
incoming-проекта не входят в план. Settings копируются точно, новый pin явно
передаётся последующим prepare/build. Незавершённая публикация блокирует readers
и допускает только точное возобновление. Весь workspace обновляется через
`--workspace --accepted-bases --incoming-bases`: полный набор pin и одинаковые
имена members, общий отказ при любом конфликте, точные settings и outer guard.
Отдельный member не превращается в standalone; `--incoming-project` выбирает
входную принятую поставку для standalone-операции.

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

[Создание табличных частей](table-creation.md) использует прежний `prepare`:
новый пустой/заполненный subtree Catalog/Document с явными service UUID проверяется
по принятому родителю, UUID всего состава, именам и типам колонок. Ошибка профиля,
коллизия или отсутствующая цель типа не обходится `--allow-exclusions`.

[Удаление реквизитов и колонок](attribute-deletion.md) также использует прежний
`prepare`: автор удаляет целый member из `children`, сохраняя порядок остальных.
Неизвестный native профиль восстанавливает состав. XML/BSL упоминание удалённого
поля отказывает до generation, включая режим `--allow-exclusions`.

[Переименование реквизитов/колонок](attribute-rename.md) принимает адресную правку
Name по прежнему UUID. Проверяются actual parent/profile, Rename permission,
окончательный namespace и выбранные XML/BSL зависимости; новые параметры не нужны.

[Удаление табличных частей](table-deletion.md) убирает весь inline subtree по
прежним prepare/build. Проверяются actual profile, права всех колонок и выбранные
XML/BSL/Type зависимости; manifest/hashes руками не меняются.

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
[Typed service identities](tabular-section-identities.md) табличной части
используют прежние import/prepare/migrate без дополнительных параметров.
Изменение принятого service UUID даёт прежний resource exclusion;
восстановление сохраняет независимые разрешённые свойства.
Запрещённое [удаление EnumValue](enum-value-deletion.md) восстанавливается с
`{"kind":"delete"}`. Оставшиеся typed/preserved XML зависимости отказывают
до публикации, включая режим `--allow-exclusions`.
Та же operation используется для запрещённого [удаления Attribute](attribute-deletion.md)
у Catalog/Document или их прямой табличной части; дополнительно проверяются
XML/BSL имена полей. Supplier rules каждого UUID и наследуемый ESKA freeze
сохраняют прежний порядок проверки.
Запрещённое [переименование Attribute](attribute-rename.md) восстанавливает Name
с `{"kind":"rename"}` и actual parent/member UUID; допустимые независимые
свойства сохраняются. Некорректное имя, коллизия или упоминание прежнего имени
в выбранных XML/BSL отказывает в обоих режимах до публикации.
При успехе prepare возвращаются `xml`, `baseline`, `generation`, input/native
revision, `source_map_sha256` и exclusions. Inspection, prepare и refusals
для одинаковых входов дают тот же JSON при ru/en. Это отдельный tool protocol;
прежние JSON-схемы приложения и IDE protocol не изменены.

Import возвращает `data.source_layout = "family" | "readable" | "queries"`.
Capabilities содержит `import_layouts = ["family", "readable", "queries"]`,
`source_layout_versions = [1, 2, 3, 4, 5, 6]`, `descriptor_schema_versions = [1, 2]`
и [map типов по layout](query-project-creation.md#machine-interface).
Tool envelope и прежние поля, включая `source_schema_version = 1`, сохранены;
для выбора поддерживаемых версий используются новые списки.
Capabilities также содержит `migrate`, `migration_layouts = ["readable"]` и
`migration_scopes = ["standalone"]`. Schema 1 для новой команды описана в
[контракте миграции](project-migration.md#machine-interface).
Для `update-base` добавлены `base_update_scopes = ["standalone", "workspace"]` и
`base_update_comparison = "whole_file_three_way"`. Preview с конфликтами —
exit 0 / `can_write = false`; `--write` — exit 1 с полным планом в
`error.diagnostics`. [Контракт обновления базы](base-update.md#machine-interface)
описывает остальные поля schema 1.

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
