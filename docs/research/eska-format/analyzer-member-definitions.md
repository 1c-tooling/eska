# Переход из BSL к дочерним метаданным YAML

`textDocument/definition` открывает реквизит объекта, табличную часть, колонку
строки или значение перечисления в текущем YAML. Например,
`Объект = Справочники.Sample.СоздатьЭлемент(); Объект.Поле` ведёт к реквизиту
`Поле` именно справочника `Sample`. URI и диапазон относятся к физическому
YAML владельца; UUID и hash подтверждают выбранный дочерний объект.

## Разрешение символа

`AnalysisSession::module_metadata_definition` использует прежние CST, вывод
типа receiver и ConfigIndex. Общая модель готового владельца подтверждает
символ до выбора координат. Переход к объекту коллекции сохраняется;
новый компонент добавляет путь к его дочернему объекту.

[Фасад HIR закреплённого ядра](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/hir/src/type_facade.rs)
различает происхождение поля. UserAttribute и TabularSectionRowColumn выбирают
Attribute, TabularSection — табличную часть. Для значения перечисления
проверяются тип Enum manager и наличие поля. Свойства платформы и поля
`Структура` не становятся YAML-целями по совпадению имени. Inferred alias
использует тот же фактический тип receiver.

Принадлежность поля определяется metadata facet ядра. Для строки табличной
части выбирается владелец и его реальная таблица. Затем только внутри текущего
главного документа проверяются parent ID, kind и имя каждого шага. Неоднозначный
путь не выбирает первое совпадение. Для координат используется прежний
[metadata locate](analyzer-metadata-locations.md).

Нового индекса, parser, XML fallback или чтения соседних владельцев нет.
Перечисление полей выполняет существующий HIR facade для выбранного receiver.
Прямой `bsl-types` dependency закреплён на том же revision и уже был транзитивным.
Код языкового ядра в этой части не изменён.

Подтверждаются только ready канонические модели выбранного root project.
Непрочитанный или ошибочный YAML убирает цель. Form data, поля записей и фильтров
регистров, predefined values, query text, идентификатор пространства общего
модуля и union receiver этим компонентом не покрыты. Готовность дерева
метаданных не равна полноте языковой модели.

## Протокол и текущий ввод

Params и форма [Location LSP 3.18](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_specifications/lsp/3.18/language/definition.md)
сохраняются. Capability расширена:

```json
{
  "scope": "ready-bsl-and-canonical-metadata",
  "metadataTargets": "typed-objects-and-members",
  "incompleteResults": "error"
}
```

`result.eska.target` содержит текущие UUID, source, native_path, origin,
source_sha256 и byte range. Caller BSL имеет собственный hash. Диапазон UTF-16
охватывает весь object mapping. Snapshot, coverage, resources, отмена и deadline
используют прежние общие механизмы; новых options или config keys нет.
BSL-цели и отдельные переходы по UUID/QName сохраняются.

При переименовании значения перечисления в YAML прежний BSL-символ теряет
цель, новое имя открывает тот же UUID с новым диапазоном и hash. Invalid overlay
не возвращает сохранённую цель. Закрытие черновика возвращает модель с диска.
Неподтверждённая цель при неполноте даёт `navigation_incomplete`.

## Проверки

Прошли 45 unit/file, 46 analyzer process и 9 checker process tests. Все 19 tool
gates четырёх feature profiles, strict Clippy, Rustdoc и release прошли.
Root: 892 passed / 14 прежних ignored; minimal 164/3, project-cli 180/3,
build-cli 199/3; все 15 gates прошли. Проверены typed receivers, точный parent,
enum rename, текущие ranges/hash, структура и свойства платформы, invalid/close,
сохранение предыдущих definition/cancellation/deadline contracts. Process tests
подтвердили текущую цель при отсутствии файла на диске в собственном fixture.

На C1/C2/C3 в RU/EN прошли 18 сценариев: реквизит, табличная часть с колонкой
строки и значение перечисления. Получены 108 подтверждённых definition и
108 независимых locate; всего выполнено 210 definition, включая pending,
invalid, stale, structure и rename refusals. Saved/editor/close, Unicode case
folding, inferred alias, BOM в BSL, non-BMP и CRLF дали одинаковые текущие
UTF-16/byte ranges и hashes. Машинные traces совпали между локалями.

| Corpus | Saved запросы выбранных целей, мс | Peak RSS, KiB |
|---|---:|---:|
| C1 | 0,437–8,251 | 68524–75536 |
| C2 | 0,442–53,185 | 219340–220636 |
| C3 | 0,462–69,062 | 246900–247576 |

Это ограниченные запросы к явно прочитанному владельцу и маленькому editor BSL.
Для сценария таблицы первый запрос выполняет inference, второй использует
текущую сессию; таблица не выдаётся за cold-start SLA или полный анализ.
Перечисление полей не обходило всю конфигурацию. Полный heap/RSS budget
этими замерами не подтверждён.

Четыре выбранных файла каждого сценария — manifest, configuration YAML,
owner YAML и saved BSL — сохранили исходные bytes. Остальные файлы отдельно
не хешировались. PATH пустой; normal exit не ждал stdin EOF; собственные
временные settings C2/C3 удалены. Платформа 1С и настоящий VS Code client
в этой проверке не запускались.

Runtime graph сохранил 206 package names / 214 resolved versions.
Release server — 48847280 bytes. [Машинный отчёт](analyzer-member-definitions-results.json)
содержит gates, точные counts и замеры без имён конфигураций или локальных путей.

EF-18 остаётся IN_PROGRESS: остальные metadata receivers, все IDE edits,
полнота анализа, общий memory budget, watcher и Explorer ещё не завершены.
