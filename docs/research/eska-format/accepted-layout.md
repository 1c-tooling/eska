# Миграция закреплённой базы ESKA

Третья часть EF-24 добавляет `prepare_accepted_descriptor_layout(&AcceptedBase)`.
План преобразует отдельно закреплённые исходники в раздельное расположение
версии 2. Каталог рабочего проекта в API отсутствует: текущие правки не могут
стать новой политикой или эталонными значениями.

[Продолжение EF-24](resource-layout.md) добавляет `prepare_accepted_resource_layout`
для карты версии 3. Оба API возвращают `AcceptedLayoutPlan`; версия 2 остаётся
отдельным выбором, более новый layout не понижается. Описание ниже фиксирует
третью часть и её прежний платформенный результат.

## Источник доверия

До подготовки вызывается `AcceptedBase::open(old_directory, independently_supplied_pin)`.
Проверяются marker, все обязательные blobs, полная source-модель, native rules,
supplier CF и freeze. План повторно проверяет старое хранилище и читает YAML
только по content addresses из этой базы. Используется тот же CST-перенос, что
в [миграции исходников](descriptor-layout.md).

Native UUID, значения/порядок, исходные input hashes, configuration identity,
список поставщиков, их payloads и explicit freeze не изменяются. Меняются
физические source paths и fingerprints перенесённых YAML/source.json.
`accepted.json` остаётся версии 1: его схема уже описывает полный снимок
исходников и не дублирует карту формата. Blob `source.json` содержит карту 2.
Непрозрачные большие payloads сохраняют прежние content addresses.

План возвращает `source_changes`, `source_manifest`, `previous_reference` и
`accepted_reference`. Новый pin вычисляется до записи по точным bytes будущего
marker. Он не заменяет прежний pin в настройках проекта или CI автоматически.

## Публикация и повторение

`plan.publish(new_directory)` пишет новое content-addressed хранилище.
Исходники и база используют общий внутренний publisher с двумя конкретными
потребителями; новые traits, plugins и dependencies не добавлены.

Receipt связывает полный старый и будущий storage inventory. Файлы устанавливаются
через приватный staging и hard link без замены существующих имён. `accepted.json`
публикуется последним; `AcceptedBase::open`/verify отказывают, пока есть receipt,
даже если marker уже записан. Совпадающий завершённый результат — `unchanged`.
Partial output возобновляется только с matching receipt и неизменённым
подмножеством ожидаемых bytes; чужие данные и правки приводят к отказу.

База, уже содержащая раздельные исходники, даёт пустой план: точный pin, оформление
YAML и storage bytes сохраняются, включая посторонние обычные notes. Зарезервированные
имена receipt/staging и их каталоги не разрешают подменить служебные файлы.
Изменение любого старого blob после preview блокирует публикацию до создания
результата. Гарантия восстановления после потери питания не заявлена;
файловая система должна поддерживать hard links.

## Использование нового эталона

После публикации вызывающий код открывает результат с `accepted_reference` и
может восстановить его через обычный `AcceptedBase::restore`. Рабочие исходники
переносятся отдельным `prepare_descriptor_layout`: их values сохраняются,
а эталонные values берутся исключительно из pinned базы. Protected preparation
сравнивает оба результата по прежним native UUID и support rules.

Project orchestration ещё нужна: перенести обе части, проверить текущие правки
и только затем явно обновить путь accepted base и independently supplied pin.
Обновление существующего проекта, CI trust reference и публичная команда миграции
в этой части не реализованы. Открытие старого проекта остаётся read-only.

## Проверки

Шесть новых unit-тестов покрывают native-byte parity, policy/freeze, отсутствие
рабочей authority, exact no-op/copy, repeat/recovery, early marker, edited/foreign
output, overlap, stale blobs и reserved names. Также расширены проверки коллизий
source map: reserved root не может стать обычным каталогом payloads.

Opt-in `real_accepted_descriptor_migration_acceptance` принимает реальную базу
C1 из read-only playground и отдельный старый pin. Проверяются `All` для каждого
descriptor, operation-specific policy по всем UUID, strict protected generations,
побайтовое равенство native output и неизменность старого хранилища.
Реальная C1 прошла: 3 595 descriptors/8 330 объектов совпали в `All`, все 33 320
policy decisions сохранились. Old/new strict protected XML побайтово одинаков;
исключений нет, старые blobs не изменились. Повторное применение — `unchanged`,
следующий план пуст.

Все 15 root и 14 SDK gates прошли: 400 unit + 537 integration, 18 opt-in
пропущены обычным запуском; SDK all-features — 355 тестов. Полный набор включает
fmt/check/strict Clippy/test/Rustdoc и minimal/project-cli/build-cli boundaries.
Старая native версия и ESKA с мигрированной базой/явным новым pin собраны в CF
и повторно выгружены через `eska-build`/`eska` на 1С 8.3.27.2325. Все 8 769
файлов/8 064 каталогов совпали, кроме прежних `configVersion`; старая и новая
базы, рабочие исходники, входной XML и версия не изменились в платформенном
цикле. [Обезличенный отчёт](accepted-layout-results.json) фиксирует результат.
Клиентское выполнение и CFE/EPF/ERF этими проверками не подтверждаются.
