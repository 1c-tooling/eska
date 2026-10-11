# Текущие предопределённые элементы в анализаторе

`Predefined.xml` справочников и трёх видов планов подключён к общему resource
controller. Его identity задаёт [reader](predefined-reader.md); источник не
определяется по имени элемента или содержимому файла. Saved/overlay/close,
version preflight, cancellation, bounded UTF-8 и source quota совпадают с
rights/dialog. XML остаётся отдельным входом со своим origin, hash и ревизией.

## Модель и ошибки

Текущие YAML и resource items собираются в существующий `MetadataObject`.
Дополнительный индекс или Salsa slot элементов не создаётся. Главный slot
владельца отслеживает пару ревизий; правка ресурса переиспользует YAML и не
пересоздаёт соседа или nested form/module-context memo. Неверный YAML очищает
все зависимые модели, неверный ресурс — только главный объект.

При отсутствии declared ресурса caller явно передаёт пустой список. Pending,
недоступный или invalid declared input не заменяется этим списком. Новый native
`try_parse_predefined_xml` возвращает ошибку syntax/root, использует тот же
ordered extraction Name/UUID/ChildItems, что прежний tolerant reader.
Тело legacy `parse_predefined_xml` и его callers не менялись. Пустой
`<PredefinedData/>` корректен. Парсер не является полной проверкой схемы 1С;
прочие свойства элемента остаются preserved данными и не входят в native model.

Invalid XML сохраняет доступные current bytes/hash/reservation, но очищает
canonical object с `analysis_predefined_data_invalid` и точным resource path.
Range в YAML не выдумывается. Size, encoding, deletion, cancellation и source
quota возвращают прежние отдельные failures. Disk fallback и прежние items
не используются. Открытый ресурс сохраняет власть над saved refresh/watch;
close читает актуальные disk bytes, включая их текущую ошибку.

## Потребители

Нативное автодополнение manager access получает текущие items из того же
tracked объекта. При неизменном BSL-тексте `Catalogs.Items.Saved` после правки
ресурса предлагает `Current`, а invalid input перестаёт предлагать старые
элементы. Checked list сохраняет native порядок, UUID строки и отсутствие
`name_en`; дополнительные имена или свойства не выдумываются.

Metadata read готовит только ресурсы выбранного владельца, включая его формы.
Semantic preparation и saved checker используют тот же controller и model.
Tree/model state и metadata diagnostics показывают resource failure отдельно
от валидности YAML. XML pull-диагностика с собственными ranges пока не добавлена.

ESKA protocol `0.1` добавляет `experimental.eska.predefinedDataSync=true`.
Input events используют `input.kind=predefined_data`, enclosing `owner` и
прежние поля raw state/origin/hash. Динамическая регистрация watches включает
точные объявленные basenames, в том числе `**/Predefined.xml`. Checker schema 1,
Role JSON, source schemas, версии и зависимости прежние.

Экспериментальный API адаптера изменён: `metadata_object(descriptor, items)`
теперь принимает готовый `Vec<PredefinedItem>` по значению. Все внутренние
callers обновлены; `Vec::new()` означает явное отсутствие данных.

## Проверки

Четыре unit tests сессии проверяют все четыре семейства, absent/stray inputs,
native model, current completion, retained Arc/memos, YAML/resource failures,
close, версии, квоту, размер и cancellation. Они заменили первоначальный
reader-only guard. Один batch test сравнивает saved/resident findings и
coverage. Три process IDE tests проверяют document sync, native completion,
watch deletion/recreation RU/EN; один checker test — JSON/human exit codes и
resource cause без invented YAML range. Добавлены один adapter и два native tests.

23 tool/6 adapter/5 kernel/15 root gates прошли: fmt/check/strict Clippy,
Rustdoc, feature matrix, tests и release builds/examples. SDK — 307 passed;
adapter — 70 unit/8 file; kernel workspace — 2 632 passed/20 прежних ignored;
root — 910 passed/14 прежних ignored. Graph 206/214 и 495 original upstream
hashes сохранены. Release analyzer — 50 560 280 bytes, checker — 46 594 432.

Пример сверяет current session и independent native reader по одному владельцу,
затем освобождает его saved source reservations:

```bash
cargo run --manifest-path tools/analysis-core/Cargo.toml --release --locked --example project_predefined -- /path/to/eska/source /path/to/native/source
```

Все 24/114/167 composed objects C1/C2/C3 совпали с native моделью, кроме ранее
известного tabular caption whitespace. Все 329/5 435/6 992 items совпали полностью:
всего 305 объектов и 12 756 элементов. Сохранились 99/459/671 hashes.
Максимальный source reserve выбранного owner — 367 670/1 019 344/1 025 945 bytes,
после каждого освобождения — ноль. Это source bytes, не замер heap/RSS.

Шесть real stdio-сценариев C1/C2/C3 RU/EN прошли на отдельных частичных копиях
с полным составом и выбранным реальным владельцем. Правка ресурса меняла actual
completion, сохраняя 2/4/1 его nested forms. Непредоставленные payloads остальных
owners намеренно отсутствовали; полная семантика этих копий не заявляется.
UTF-16/non-BMP/CRLF, hashes, editor authority, close, delete/recreate и нормальный
exit без stdin EOF проверены. Это уведомления протокола, не настоящие OS/client watches.

Checker C1 с выбранным `SelfAssign` сохранил 26 findings в 1 975 модулях и все
960 metadata issues, профиль/settings и JSON RU/EN. Предопределённые ресурсы
добавили 85 982 source bytes: резерв 204 388 259 из 268 435 456. Все 7 829 hashes
сохранились; exit 2 остаётся из-за прежней неполноты моделей. Контроль 978 общих
форм и 1 641 роли C1/C2/C3 снова дал native parity и неизменные hashes.
Результаты — в [машинном отчёте](analyzer-predefined-results.json).

Платформа и XML export не запускались: компонент читает и анализирует current
source. Другие модели, общий memory budget, остальные IDE edits и Explorer
ещё в работе; EF-18 — `IN_PROGRESS`, EF-12 и следующие этапы не начаты.
