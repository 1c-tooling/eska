# Адресная модель identity/layout — EF-12, часть 2

В общем `project_core::source_format` добавлен `ManagedFormLayout` и адресные
`SourceReader::read_form_layout` / `parse_form_layout`. Это read model текущего
native dialog и валидатор предложения layout. Импорт, XML payloads и схемы
исходников пока прежние; редактируемый `form.yaml` и writer ещё нужны.

Модель основана на [инвентаризации C1](managed-form-inventory.md).
Профиль — `{http://v8.1c.ru/8.3/xcf/logform}Form`, version 2.20.
Unknown UI class/shape или иной профиль возвращает `source_form_profile_unverified`;
действующий raw-resource reader по-прежнему может читать исходные байты.

## Границы модели

`elements` хранит определения по native decimal ID, отдельно от упорядоченных
`layout.roots` / `layout.children`. `fixed` хранит настоящие owner/element edges
для AutoCommandBar, menu, tooltip и табличных additions. Fixed узлы могут иметь
свои ChildItems, но сами не получают право свободного переноса.
29 UI classes имеют явные child domains: Pages/Page, Table/ColumnGroup,
command containers и обычные visual groups не взаимозаменяемы.

UI, root Attributes и Commands имеют отдельные ID/name scopes. Columns проверяются
в каждом непосредственном Columns/AdditionalColumns, без общего namespace формы.
Корневой AutoCommandBar сохраняет ID -1; остальные ID имеют каноническую
неотрицательную decimal запись. Fixed aliases могут повторяться/быть пустыми.
Служебный ConditionalAppearance внутри Attributes остаётся opaque содержимым,
не реквизитом или элементом UI.

`AdditionSource.Item` разрешается к regular Table, `Form.Command.*` — к root
Commands. Numeric/builtin/прочие CommandName и DataPath не переинтерпретируются.
Типы, события, настройки СКД и произвольные свойства не становятся редактируемыми
из-за успешного чтения этого graph.

## Проверки layout

`validate` требует один incoming parent для каждого UI ID, допустимый класс
вложения, существующие targets и отсутствие циклов/недостижимых элементов.
Фиксированный slot имеет допустимого owner и единственную роль у этого owner.
Обход графа итеративный; глубина layout не превращается в рекурсивный стек.

`validate_layout_change` дополнительно сравнивает определения, ID namespaces,
fixed bindings и известные references с принятой моделью. Меняется только порядок
и parent обычных элементов. Сохраняется ближайший Table context каждого узла:
перенос колонки в группу/другую таблицу требует отдельного inverse rule.
Метод не выдаёт права поддержки и не применяет изменение к XML.

## Адресное чтение

Reader открывает только manifest/configuration, как раньше. Saved layout читается
по точному declared dialog path через прежнюю проверку пути, кодировки и byte limit.
Overlay проходит ту же модель без filesystem чтения. UUID формы/metadata descriptor
и модуль связываются существующими declared identities; numeric UI ID их не заменяет.
Сломанный owner YAML, соседний dialog или отсутствие текущего файла не активируют
скрытый XML fallback и не заставляют читать все формы при открытии проекта.

CLI/config/JSON protocols, locale keys и версии схем не менялись. Модель доступна
в минимальной библиотеке; новые ошибки содержат machine codes и source path/ID.
Следующая часть — lossless единый form.yaml, затем protected writer/CST/source map
и реальный экспорт изменённой формы через платформу.

## Проверенный корпус

Все 823 declared dialogs C1 прочитаны через новый SourceReader без ошибок:
52 502 UI definitions, 18 571 ordinary и 33 931 fixed элементов, отдельные
4 971 Attributes/2 303 Commands. Разрешены 1 822 AdditionSource и 2 804 local
commands. Для каждой формы saved/overlay модели совпали, no-op layout proposal
принят, bytes файла сохранились. Это чтение существующего платформенного dump,
не новая CF/runtime приёмка изменённой формы. C2/C3 здесь не проверялись.

11 новых model tests и один addressed reader test проверяют разрешённые moves,
отказ при изменении Table context/definition/fixed owner, недостающие/двойные
parents, disconnected cycles, неверное nesting, коллизии scopes/names, unknown
profile и локальные references. Reader test сохраняет BOM/CRLF, учитывает byte
limit и подтверждает overlay без существующего файла/валидного owner YAML.
[Обезличенные результаты](managed-form-layout-results.json).

Полные fmt/check/all-targets/all-features strict Clippy/default test прошли
после уточнения appearance profile. Дополнительно прошли all-features test,
минимальная библиотека, project/build CLI, Rustdoc с запретом warnings,
SDK/adapter feature gates и diff check. Приложение: 604 unit и 570 integration;
project/build CLI: 44/9; SDK: 359; adapter: 70 unit и 12 integration.
Обычный прогон пропускает 24 существующих opt-in tests. CF/client/runtime цикл
этой частью не выполнялся.
