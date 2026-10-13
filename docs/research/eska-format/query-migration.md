# Явная миграция отдельных запросов — EF-23, часть 6

`prepare_query_layout` создаёт expected-byte план перехода существующих
family/separated/resources/readable/form layouts в [map 5](query-project.md).
Он не меняет входное дерево. Сначала применяются необходимые прежние descriptor,
readable и form projections; затем тот же extractor, что использует импорт,
переносит decoded text в единственные `.sdbl`. Actual dataset/Attribute scope
остаётся в reference XML, provenance/UUID/native fingerprints — в карте.

Для форм сохраняются definitions, properties, layout и остальные native stencils.
Меняются только XML scalar values в `form.native.root`, содержащие query nodes.
CST splicing сохраняет окружающий YAML, flow/block syntax и annotations;
сверка comments и полной decoded модели обязательна. Unsupported profile/trivia
отказывают до создания destination. Оpaque unrelated resources не декодируются;
все объявленные СКД, включая schema без query, участвуют в новом layout.

Новые query paths не могут заменить existing unrelated payload. Missing/invalid
inputs, collisions, foreign outputs и changed-byte publication отказывают.
Существующая exclusive publication использует прежние exact receipts: owned
partial output можно продолжить, edited/foreign bytes нельзя перезаписать.
Новый `source.json` становится последним ready marker. Map 5 для всех targets
остаётся byte no-op; прежние targets сохраняют map 4. Только query target
явно повышает map 4 до 5. Opening и CLI defaults не мигрируют проект.

## Независимая база и проект

`prepare_accepted_query_layout` преобразует только independently pinned blobs.
Working files не читаются и не принимаются. Native policy, supplier CF/freeze,
input revision и identities сохраняются; возвращённый новый `AcceptedRef`
выбирается явно. Existing map 5 сохраняет все bytes и прежний pin.

`prepare_query_project` переносит standalone project через общий план source/base
и publication; `prepare_query_workspace` планирует всех members до записи.
Exact root/member settings, конфигурация и наследование сохраняются. Каждый
member требует отдельный approved old pin; новые pins доступны в preview.
Project/workspace receipts не позволяют открыть partial output и публикуют
ready settings последними. Новые публичные CLI options/protocols не добавлены.

Миграция сохраняет текущий unprotected native результат независимо от базы,
но не подтверждает допустимость текущих правок. Protected export использует
новый independently pinned snapshot, прежние Resource права и pinned recipes.
Например, ручная смена empty XML recipe в старом inline query может получить
strict exclusion; миграция её не принимает. После перехода заполнение пустого
`.sdbl` не меняет recipe и проходит protected export при разрешённых правах.

## Проверки

Восемь owned tests покрывают пять прежних вариантов layout, native byte equality,
independent pins/freeze/current values, byte no-op без downgrade, CST locality,
resumable source/base publication, invalid/empty СКД и path collisions,
standalone/workspace settings и missing trust. Отдельно проверено заполнение
empty plain query с неизменным `form.yaml` и нулём strict exclusions.
Все Cargo/Rustdoc и SDK gates прошли: 708 unit, 570 integration, 359 SDK tests;
24 прежних ignored.

[C1 results](query-migration-results.json): 153 query resources и 150 plain files.
Рабочие inline-правки сохранены в 126 native resources; independently migrated
base содержит исходные значения. Оба полных вывода сверены по 8 769 files и
8 064 directories. Прежние supplier decisions сохранены: 46 разрешённых и
104 запрещённых значения; protected output изменяет 41 resource и совпадает
с независимым контролем. Original project/base/control и exact settings
сохранены; повторная публикация и map 5 no-op подтверждены.
Новый CF/build/dump и выполнение отчёта/списка ещё нужны.
EF-23 остаётся `IN_PROGRESS`; analysis scope — EF-27.
