# Общие реквизиты с явным составом

`CommonAttribute` имеет отдельную canonical модель и listing в закреплённом
native kernel. SDK передаёт UUID, имя, Type, explicit Content и ownership из
единственного текущего `ParsedObject`. Сохранённый XML не подставляется;
параллельный parser или реестр исходников не создаётся.

Общий реквизит действительно добавляет одинаковое имя и тип нескольким объектам.
Это отдельное назначение от разделения данных.
[Описание платформы](https://v8.1c.ru/platforma/obshchiy-rekvizit/).
Reference MCP подтвердил AutoUse: Use/DontUse и Content.Use: Use/DontUse/Auto.

## Принятый срез

Поддерживаются обычные реквизиты с явными `AutoUse=DontUse`,
`DataSeparation=DontUse` и Content. Namespace-aware записи `xr:Item` содержат
Metadata, Use и необязательный пустой ConditionalSeparation. Проверяются
повторные targets/scalars, namespace identity и точные типы через общий адаптер.
`Use` добавляет поле; `DontUse` и `Auto` при отключённом AutoUse не добавляют.
Пустой Content сохраняет отсутствие членства.

Цели — текущие моделируемые persistent object/register families: Catalog,
Document, ExchangePlan, BusinessProcess, Task, три ChartOf* и четыре регистра.
Другие families, nested targets, неизвестные records, непустое условное разделение,
AutoUse=Use и DataSeparation=Use не получают готовой модели. Отсутствующие
контролирующие поля не заменяются предположенным default.

## Native потребитель

Per-file resolver сначала собирает объект/регистр с видимыми вкладами CF/CFE,
затем добавляет поля общих реквизитов того же visibility chain. Native BSL
completion и SDBL lowering используют эти поля через прежние metadata APIs.
Регистр сохраняет UUID общего реквизита. Конфликт с локальным полем сохраняет
наличие, но даёт Unknown type; прежний известный тип не выбирается произвольно.

Adopted contribution требует доказанного ExtendedConfigurationObject UUID.
Он меняет supplied targets, сохраняя остальные targets базы. Pending/invalid
вклад и неверное заимствование не получают substitute из другой версии/root.
SDK preflight преобразует такой отказ в существующий root-qualified
`analysis_composition_incomplete`, до доказательства missing table/field.
Native Salsa cancellation остаётся отменой запроса.

Текущий тип или Content меняет поле без изменения query input. Invalid overlay
отзывает модель; watch не заменяет dirty bytes, close возвращает реальный saved
source. Видимость и RootId сохраняют раздельные equal-path inputs базы,
зависимостей и выбранной CFE. Hidden sibling не участвует.

## API и границы

Экспериментальные `CanonicalMetadata` и `MetadataListingData` получили
CommonAttribute variant/list. Native `common_attributes_for_file` возвращает
текущие вклады либо FileId недоступного/неподтверждённого объявления. Это не новый
wire protocol; CLI/config/JSON schemas, package versions и dependencies прежние.
Предыдущие unsupported-family tests используют Bot, сохраняя отказ на
неподдержанной семантической family.

CommonAttribute не переведён в StructureOnly. Полная семантика общих реквизитов,
автоиспользование и разделение данных остаются в работе. Другие metadata families,
EF-18/EF-27 и MVP этой частью не закрываются. Новый запуск платформы 1С для
этого среза не выполнялся.

## Проверки

Frozen native/adapter/SDK gate: 21 команда, включая fmt, check, strict Clippy
all-targets/all-features, strict rustdoc, default/all-features SDK tests и
debug/release binaries. Kernel — 4422 passed / 25 ignored; adapter — 103;
SDK — 218 default / 521 all-features, без ignored. Все 1595 Rust/Cargo inputs
сохранились между началом и концом проверки. Корневые 736 Rust/Cargo inputs
не менялись с предыдущего полного gate; здесь повторно не запускались.

Explorer: 339 Node tests без skipped. Восемь empty-PATH checker cases
CF/configured-CFE × включённое/выключенное членство × RU/EN подтвердили
появление и отзыв UnknownField с одинаковым machine output между локалями.
Exit 2 отражает остальные diagnostics fixture, а не сбой запуска checker.

Установленный прежний VSIX с новым frozen release analyzer проверил текущие
поля generated CF и foreign base configured CFE, invalid/retraction,
restart/replay, import и сохранность исходников. Прежние native CF consumer и
standalone native CFE syntax/refusal cases также прошли; 19 дочерних процессов
завершились. Native Save, физический ввод пользователя и новая платформа 1С
не проверялись.

Первый installed прогон не наблюдал retirement control marker после import.
Он не принят; логи сохранены. Повтор с наблюдением настоящих watcher events
прошёл без изменений production Explorer. Причина первого сбоя не доказана;
успешный повтор не закрывает это ограничение общей приёмки EF-11/EF-25.

Readonly проверка большой native конфигурации дала ready для всех четырёх
CommonAttribute. Композиция остановилась на следующей неподдержанной family
CommonCommand: cold 15.34 s, peak RSS 394076160 bytes. Исходники сохранились,
процесс завершился. Это отказ, а не принятая производительность; связь именно
этой CF/CFE пары по-прежнему не подтверждена.

[Машинные результаты](analyzer-common-attributes-results.json) содержат gates,
размеры binaries и разделённые границы synthetic/installed/native проверки.
