# Собственный контекст внешней обработки и отчёта

`eska-analyzer --profile semantic` принимает EPF/ERF как
`canonical_external`. Один настоящий external root передаёт текущий объект,
реквизиты, табличные части и формы в тот же kernel. Не создаются Configuration,
семантическая база или отдельный индекс. Explicit `LocalSyntax` и CLI-профиль
`syntax` сохраняют прежнюю синтаксическую границу четырёх типов.

Core нормализует native external tags до DataProcessor/Report. Только проверенный
project profile возвращает внешнюю разновидность в adapter; имя и UUID остаются
исходными. Оба adapter результата совпадают с закреплённым native parser.
Current owner/form slots применяются до любого XML loader; pending и ошибочный
ввод не восстанавливают прежние модели с диска.

## Возможности и неполный контекст

Own completion, hover, definition, symbols и другие объявленные read-only language
operations используют подготовленные текущие inputs. Поле external main attribute
формы сохраняет настоящий underlying owner. Для cold hover/definition подготовка
идёт через существующие bounded input APIs; предварительный completion не нужен.

Собственные модели не определяют globals, common modules и метаданные неизвестной
базы. Поэтому `metadata_context_complete` остаётся false; completion возвращает
`isIncomplete: true`. Неподтверждённые foreign targets и полная семантическая
диагностика получают `analysis_configuration_context_required`. Semantic checker
отказывает с exit 3 до full input loading. CFE/base/dependency composition этой
частью не подключена.

Диагностика текущего owner YAML доступна отдельно: capability
`experimental.eska.metadata.diagnostics` указывает `textDocument/diagnostic`,
scope `declared-owner-yaml`, profile `metadata`. Она не требует контекста базы.
Общий BSL semantic diagnostic provider для external не объявляется.
Edit plans и принятие поддержки не включаются этим semantic scope.

## Текущий ввод и проверки

Dirty YAML меняет self/form types; dirty BSL остаётся текстом редактора.
Watch event не заменяет overlay. Invalid owner очищает current model, close
возвращает сохранённые данные. Общие cancellation, source budgets и import
retirement сохраняются; active import требует нового сеанса.

Для runtime controls в собственном playground независимые native EPF/ERF
дополнены строковым реквизитом, собраны и выгружены на 1С 8.3.27.2325, затем
импортированы в Queries. Это подтверждает native storage/сборку данного реквизита;
исполнение клиентского кода и семантика foreign metadata проверяются отдельно.

Четыре framed runtime cases, EPF/ERF × RU/EN, подтвердили real field completion,
current rename и form context обработки, dirty BSL symbols, invalid type/repair,
metadata diagnostics и import retirement. Root YAML watch честно требует reopen;
повторная передача caller drafts сохранила текущие YAML/BSL. Source, accepted
storage, история и settings сохранили bytes/mtime. У отчёта этой native fixture
формы нет; form contexts обоих типов дополнительно проверены Rust fixtures.

Все 22 проверки прошли: полный набор root, SDK и adapter, строгий Clippy,
Rustdoc и отдельные build CLI tests. Root default: 869 unit и 621 integration;
root all-features: 869 unit, 646 integration, 27 build и 69 project tests.
SDK: 199 unit, 172 analyzer и 24 checker tests; adapter: 76 unit и 12 integration.
Подготовленные navigation/permissions helpers отдельно прошли 6 и 4 tests,
но подключаются следующим изменением.

Обезличенные результаты native, runtime и Rust-проверок:
[external-own-context-results.json](external-own-context-results.json).
