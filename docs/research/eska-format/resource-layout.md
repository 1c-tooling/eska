# Физические пути модулей и ресурсов

EF-24 добавляет опциональную обратную карту версии 3. Самостоятельные YAML
остаются по [правилам версии 2](descriptor-layout.md); модули и ресурсные наборы
получают явные физические пути. Публичный CLI импорта пока создаёт семейную
версию 1. Открытие проекта ничего не переносит.

## Карта и расположение

`FileEntry.path`, `bytes` и `sha256` по-прежнему описывают первоначальный native
вход. Роль `descriptor` указывает на YAML. Роль `preserved` означает payload
по прежнему пути; новая роль `relocated` содержит обязательный `source` —
физический путь точной копии payload. `FileEntry::source_path()` разрешает
авторитетный исходник, `is_payload()` отличает его от descriptor.
[JSON schema](source-manifest-v3.schema.json) проверяется вместе с правилами core.

Перенос выполняется для root `Ext` и ресурсного `Ext` объявленного descriptor,
включая команды внутри владельца. Сегмент отделяется по проверенному native
контексту, а не глобальной заменой слова. Внутренний каталог ресурса с именем
`Ext` и неизвестные orphan paths сохраняются. Названия модулей не меняются.

| Native path | Физический исходник версии 3 |
|---|---|
| `Ext/ManagedApplicationModule.bsl` | `ManagedApplicationModule.bsl` |
| `Catalogs/Products/Ext/ObjectModule.bsl` | `Catalogs/Products/ObjectModule.bsl` |
| `Catalogs/Products/Forms/List/Ext/Form.xml` | `Catalogs/Products/Forms/List/Form.xml` |
| `Catalogs/Products/Forms/List/Ext/Form/Module.bsl` | `Catalogs/Products/Forms/List/Module.bsl` |
| `Catalogs/Products/Forms/List/Ext/Form/Items/...` | `Catalogs/Products/Forms/List/Form/Items/...` |
| `Roles/User/Ext/Rights.xml` | `Roles/User/Rights.xml` |
| `Catalogs/Products/Ext/Predefined.xml` | `Catalogs/Products/Predefined.xml` |
| `Reports/Sales/Templates/Main/Ext/Template/...` | `Reports/Sales/Templates/Main/Template/...` |
| `Ext/ParentConfigurations.bin` | `ParentConfigurations.bin` |
| `Ext/ParentConfigurations/<supplier>.cf` | `ParentConfigurations/<supplier>.cf` |

HTML с картинками, внутренние файлы макетов, XDTO/XSD и бинарные наборы
переносятся целиком под теми же внутренними именами. Writer не переписывает
их ссылки или содержимое. Отдельный модуль формы выходит к `form.yaml`;
остальная группа `Form/Items` сохраняет свою структуру. XML dialog остаётся
ресурсом до EF-12. Извлечённых `.sdbl` здесь ещё нет.

`directories` в карте остаётся native inventory, включая пустые каталоги.
Core отдельно выводит физические source directories и восстанавливает native
directories при экспорте. Проверяются case aliases, file/directory conflicts,
пересечение payload с YAML/`source.json`, служебными именами publisher и
отклонение от канонического правила переноса. `relocated` в версиях 1/2
запрещена. Hashes текущего текста остаются производными; ручное обновление
карты после обычной правки не требуется.

## Reader, поддержка и диагностика

Общий reader объявляет модули, Rights, Predefined и dialog по физическим путям,
сохраняя native descriptor и native module identity. SDK использует виртуальные
native paths внутри ядра: его conventions определяют тип модуля и форму.
Диск, редактор, completion и navigation используют физические исходники.
Навигация переводит `FileId` в объявленный source path без обхода каталога.

Защищённая сборка разрешает модуль/ресурс по закреплённой native identity.
Правила поддержки и supplier CF остаются обязательным pinned evidence даже
после переноса. Работа с текущим файлом не даёт новых прав. Заморозка владельца
наследуется отдельной формой. Запрещённые изменения восстанавливаются из базы;
strict отказывает при exclusions.

Для версии 3 [карта собранных файлов](source-map.md) использует map schema 2:
`MappedFile.path` — native результат, `origin.input.path` — физический исходник.
Их fingerprints и байты совпадают; offsets payload переносятся без изменения.
Старые layouts сохраняют map schema 1 с равенством этих путей. Descriptor/source
schema в origins, conversion rules и prepared artifact schema не меняются.

## Явные планы

`import_designer_resources` создаёт новый source tree из Designer XML.
`prepare_resource_layout` строит expected-byte план для версий 1/2/3;
`prepare_descriptor_layout` сохраняет отдельный выбор descriptor-only layout
и не понижает уже более новую версию. Общий тип результата — `SourceLayoutPlan`.
Это уточнение экспериментального Rust API без compatibility aliases.

`prepare_accepted_resource_layout` читает только independently pinned blobs.
Общий `AcceptedLayoutPlan` сохраняет native evidence, suppliers, policy/freeze
и возвращает новый pin до публикации. Рабочие правки не входят в новую базу.
Project settings и CI pin автоматически не обновляются.

Оба плана используют общий exclusive publisher: streaming payload, expected
fingerprints, recheck входа, ready marker последним, точный repeat/no-op и
восстановление с совпадающим receipt. Редактированный/чужой output отказывается;
прерывание во время private staging не повреждает исходники. Это гарантия
восстановления после остановки процесса, без обещания fsync/power-loss recovery.
Публичная команда миграции и orchestration проекта ещё не реализованы.

## Проверки

Прошли 15 root/14 SDK gates: fmt/check/strict Clippy/test/Rustdoc, изолированный
playground, минимальные features и отдельные CLI. В приложении 405 unit + 537
integration, 19 opt-in пропущены обычным запуском; SDK all-features — 358 тестов.
Проверены payload bytes/BOM/CRLF, вложенные `Ext`, empty directories, коллизии,
forged mappings, v1/v2 migration, v3 no-op, repeat/recovery/stale input,
поддержка/freeze, физические origins/offsets и old-map refusal. Реальные kernel
queries формы/common/service сохранили контекст и current/overlay поведение.

Свежая C1 и миграции её старых исходников/accepted base сохранили 3 595 descriptors,
8 330 объектов в `All` и 33 320 policy decisions. Все байты 5 183 payload совпали;
5 173 файла перенесены, source.json содержит 4 540 685 байт. Schema валидирует
новую карту и каждый из 3 595 YAML. Raw migration и восстановление новой базы
дали одинаковое дерево; strict protected XML совпал с прежним по всем байтам.

Native контроль из прежних pinned данных и новый проект с layout 3 собраны
в CF и повторно выгружены через ESKA на 1С 8.3.27.2325. Все 8 769 файлов/
8 064 каталога совпали, кроме 7 011 прежних `ConfigDumpInfo.configVersion`.
Проверены checksum artifacts, source map schema 2, новый pin и отсутствие
exclusions. Исходники, старые/новые базы и версия конфигурации не изменены.
[Обезличенные результаты](resource-layout-results.json) отделяют этот результат
от клиентского выполнения, проверки всех BSL-модулей, C2/C3 и CFE/EPF/ERF.
EF-24 остаётся `IN_PROGRESS`: читаемая оболочка, составные домены и structural
operations ещё не реализованы.
