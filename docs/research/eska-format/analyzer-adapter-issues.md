# Подробные ошибки канонических моделей

Состояние `model=invalid` по-прежнему сохраняет `code=analysis_adapter_invalid`
и `range`. В `model_issue` добавлен объект `issue` с конкретной причиной.
Experimental protocol `0.1`, команды и прежний IDE protocol 1.x не менялись.
Одну DTO-проекцию используют страницы владельцев, metadata read/refresh и события
текущего YAML. Core хранит прежний `AdapterIssue`; JSON задаёт только transport.

| `issue.kind` | Дополнительные поля |
| --- | --- |
| `properties_not_loaded` | нет |
| `wrong_kind` | `actual`: канонический metadata kind |
| `invalid_uuid` | нет |
| `duplicate_property` | `name` |
| `invalid_property` | `name` |
| `unsupported_ownership` | `value` |
| `missing_property` | `name` |
| `unsupported_type` | `key`: `{namespace, name}` |
| `unsupported_type_field` | `key`: `{namespace, name}` |
| `unresolved_type_name` | `text` |
| `missing_root` | нет |
| `incomplete_descriptor` | нет |
| `unsupported_standard_attribute` | `name` |
| `unsupported_property_value` | `name`, `value` |

`namespace` — исходный URI или `null` для неквалифицированного имени. Значения
`name`/`text`/`value` сохраняются без перевода и нормализации. Клиент может выбрать
своё объяснение по `kind`; Rust Debug и локализованный текст не являются схемой.
`range` остаётся диапазоном UTF-8 bytes текущего YAML, связанным с source hash,
origin и snapshot. Диапазон может охватывать свойство или сохранённый XML-фрагмент,
а не только проблемный token. `input=ready` не означает готовую BSL-модель:
parsed tree доступно отдельно. Исправление очищает `model_issue`; close берёт
сохранённые байты. Для unsupported metadata kind сохраняется прежняя форма
`{code: analysis_metadata_kind_unsupported, kind}` без `issue`.

## Проверки

Этот замер относится к `a184b29`. Следующий компонент подключает те же причины
к [диагностике текущего документа YAML](analyzer-metadata-diagnostics.md).

Два unit-теста фиксируют все 14 вариантов и точные поля, включая null namespace
и Unicode. Process test в RU/EN проверяет страницы/чтение/уведомления, сдвиг
диапазона редакторским комментарием, исправление и возврат сохранённой ошибки.
Пройдены 68 unit/file, 69 analyzer process и девять checker process tests, всего
146; minimal profile — 54. Все 19 tool gates и 15 root gates прошли:
fmt/check/strict Clippy/Rustdoc, тесты четырёх профилей и release builds.
Root — 893 passed / 14 прежних ignored; minimal 165/3, project-cli 181/3,
build-cli 200/3. Адаптер и kernel patches в этой части не менялись.

Шесть stdio-сценариев C1/C2/C3 в RU/EN используют собственные частичные копии
payload с полным составом. Изменение `Server` одного реального общего модуля
в editor-only input даёт `invalid_property`, два текущих диапазона и один hash
в каждом ответе о состоянии. Коррекция/close восстанавливают готовую модель.
В C1 отдельно проверены оба реальных владельца с `cfg:AnyRef`: справочник и план
характеристик. `unsupported_type.key` точно совпал с независимо прочитанным
namespace исходного XML; parsed tree сохранилось, диапазон сдвинулся с текущим
редакторским текстом и восстановился после close. Сам тип не стал поддержанным.

Bytes 5/3/3 выбранных оригиналов и их копий сохранены; временные проекты удалены.
Native XML не был входом сервера, PATH пустой, normal exit не ждал stdin EOF.
Остальные payload намеренно отсутствовали; полная семантика не проверялась.
Peak RSS — 53344–197092 KiB, время сценария — 0,31–0,89 s; это ограниченный замер.
Граф прежний: 206 package names / 214 resolved versions; release server —
49123168 bytes. [Машинный отчёт](analyzer-adapter-issues-results.json) сохраняет
результаты без имён конфигураций, настоящих UUID и локальных путей.

Новые BSL-исходники, платформа 1С, VS Code client и OS watcher здесь не проверялись.
Полная семантика, остальные модели, все IDE edits, общий heap/RSS budget и Explorer
остаются в работе. EF-18 — `IN_PROGRESS`.
