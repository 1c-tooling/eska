# Текущие последовательности

`Sequence` получает отдельную canonical модель, listing и current slot.
Модель сохраняет UUID, имя, измерения с их настоящими UUID и типами, ссылки
на документы и регистры. Она не подменяется регистром. Адаптер использует общий
descriptor parser, reader квалифицированных ссылок и `TypeDescription` converter;
нового XML loader нет.

Собственное объявление требует явных Documents/RegisterRecords и типов новых
измерений. Заимствование проверяет имя и UUID оригинала, включая каждое
заимствованное измерение. Отсутствующий borrowed Type наследуется, явный тип
меняет текущее значение; собственное новое измерение не может совпасть с прежним
по имени или UUID. Ссылки Documents/RegisterRecords дополняются без повторов.
Неизвестные target kinds, дубли, неверный ownership и неполные свойства
возвращают typed issue. Invalid current contribution не заменяется прежней базой.

## Native BSL

Настоящий `Sequences` / `Последовательности` предлагает только текущие видимые
имена. Созданные платформой `SequenceManager`, `SequenceRecordSet` и
`SequenceRecord` сохраняют настроенное имя последовательности. Методы берутся
из закреплённого каталога платформы с настоящими IDs, параметрами, арностью
и контекстами выполнения. Нормализация ошибочных placeholders справочника
вынесена в общий native helper с существующим FilterCriterion.

`CreateRecordSet` возвращает собственный набор записей, `Add`/`Get` и `For Each`
сохраняют тип его записи. У записи есть native Period/Recorder и текущие
типизированные измерения. Тип Recorder уточняется по объявленным документам.
Completion и lookup используют одну проекцию; известная запись с отсутствующим
полем даёт `UnresolvedField`. Непроверенная модель не считается закрытым типом.
Hover и signature help методов используют тот же native call binding.

Модуль набора записей связывается с owner через объявленный FileId. Его
`ThisObject` и implicit properties получают настоящий тип набора записей
последовательности. Stable slots, root visibility, UUID-проверенное наследование
и отказ текущей композиции сохраняются во всех этих запросах.

Холодный запрос готовит собственное семейство через прежний input controller.
Native receiver discovery учитывает aliases, user shadowing и границы тел;
вызов в соседней процедуре не разрешает чтение. SDK, checker и Explorer
используют одни current inputs, foreign ACK, watcher, close и restart/replay.
Канонический корень не читает XML sidecar вместо недоступного descriptor.

Экспериментальные Rust APIs расширены семнадцатым listing family:
`CanonicalMetadata`, constructors/inputs listing, `ConfigsDatabase`,
`ObjectResolver` и `ModuleMetadata.sequence`. Все текущие потребители обновлены.
CLI/config/source schemas, wire negotiation и версии не менялись. Новых crates
или dependencies нет; девять локальных пакетов остаются под прежним pin
`9021ba07471eee8a2614bed2105a2c8699f60780`.

[Назначение последовательности в платформе](https://v8.1c.ru/platforma/posledovatelnost/).

## Проверки

Все 27 gates выполнены на одном наборе 1669 Rust/Cargo/locale inputs: шесть
корневых, пять kernel, пять adapter и одиннадцать SDK. Прошли fmt, check,
strict Clippy, default/all-features tests, rustdoc с `-D warnings` и debug/release
бинарники. Корень — 1511 default / 1632 all-features, по 24 ignored; kernel —
4430 passed / 25 ignored; adapter — 119; SDK — 243 default / 559 all-features,
без ignored. Итоговые inputs не менялись между проверками.

Проверены namespace RU/EN, generated methods, typed record fields, `ThisObject`,
iteration, native method hover/signatures, invalid/repair/watch/close, CFE
inheritance, неправильный original UUID и hidden sibling. Native тест подтвердил
нулевой churn `load_configuration` при изменении canonical sequence.
SDK/process checker различает корректный Date, несовместимый Boolean и
недоступную композицию: exit 0/1/2, одинаковый JSON RU/EN.

Усилена прежняя проверка мягких полей структуры параметров опций. Вместо
несуществующего `StringLength` используется настоящий `Month`: чтение мягкого
поля остаётся unknown, а явный `Month(False)` даёт `TypeMismatch`. Это проверяет
нужное свойство без универсального преобразования аргумента в строку у `StrLen`.
Production policy изменяемой Structure не менялась.

Explorer — 371/371 Node tests, без skipped. Установленный прежний VSIX подтвердил
native completion для записи и её record-set module, method hover, current type
без изменения BSL, invalid retraction, неправильный CFE UUID, repair и replay.
Все 29 дочерних процессов завершились; инвентари 84/73/133 файлов и native-CFE
сохранились. Все 55 production JS modules VSIX совпали с текущей компиляцией;
production client code и пакет не менялись, test helpers в VSIX не включены.
Native-origin CF сохранила прежнюю consumer-матрицу; native-origin CFE явно
отказала в semantic context, сохранив syntax.

Большая native-origin CF дала 1/1 готовую последовательность с четырьмя
измерениями и без parse issues. Один bounded explicit-base/selected-CFE запрос
остановился на `SettingsStorage`: `analysis_composition_incomplete`, cold
18.56 s, наблюдаемый peak RSS 394944512 bytes. Source reserve — 243732863 из
268435456 bytes. После `stale_revision` выполнен один повтор только memory
telemetry с возвращённой сервером текущей revision. Heap estimate остаётся
неполным: две живые таблицы без heap hook. Warm/update циклы после отказа
не запускались; оба процесса завершились с exit 0. Исходные bytes/mode/mtime
сохранились. Новые копии конфигурации и базы 1С не создавались.

[Машинные результаты](analyzer-sequences-results.json).

## Границы и остаток MVP

Здесь не добавлены SDBL tables последовательностей, configured projection
колонок GetBounds/Unload, проверка содержимого dimension structures методов
или весь metadata navigation/editing. Соответствующие native generic signatures
сохранены. DocumentMap/RegisterRecordsMap остаются в исходнике и не используются
для новой проверки зависимостей. Платформа 1С, Native Save и физический ввод
в этом срезе не запускались; vendor association большой CF/CFE не подтверждена.

EF-18 остаётся `IN_PROGRESS`: следующий обнаруженный отказ — SettingsStorage;
полное покрытие и общий memory budget ещё нужны. EF-27, editable opening и
guarded Save EF-11, обычная установка/обновление/удаление EF-21 и сквозная
платформенная/клиентская приёмка EF-16 также остаются в работе. MVP не завершён.
