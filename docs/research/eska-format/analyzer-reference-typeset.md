# AnyRef в текущем TypeSet

Первое холодное semantic completion большой настроенной CF/CFE-композиции
отказало с `analysis_composition_incomplete`. Адресное чтение причины показало:
YAML успешно разобран, но адаптер отклоняет `AnyRef` в `TypeSet` одного текущего
описателя. В нативном Designer XML этот token действительно находится в
`{http://v8.1c.ru/8.1/data/core}TypeSet` и разрешается в namespace
`http://v8.1c.ru/8.1/data/enterprise/current-config`.

Это известная категория ссылочного типа. Официальный
[XML converter 1С](https://edt.1c.ru/dev/edt/2024.2/apidocs/com/_1c/g5/v8/dt/xml/ChangeAnyRefTypeOutputStream.html)
поддерживает преобразование AnyRef ↔ AnyIBRef. С учётом фактического TypeSet и
существующей native модели оба написания сопоставлены с `AttributeType::AnyRef`.
Это решение о семантике входного типа; XML/YAML не переписываются.

Общий `type_description::type_set` обслуживает все потребители TypeDescription,
включая поля объектов и регистров. Native XML type-set reader получает ту же
известную альтернативу. Нового enum, serialized shape, dependency или fallback
для неизвестных типов нет. Адаптер по-прежнему сравнивает resolved namespace и
local name, поэтому префикс alias допустим, а чужой namespace — нет.

Конкретное `<Type>cfg:AnyRef</Type>` этим изменением не принимается: доказан
именно TypeSet. Future/unresolved/foreign QName и неправильный регистр сохраняют
отказ. Native XML reader сохраняет прежнюю обработку prefix spelling; общего
QName refactor здесь нет. Metadata readiness guard не ослаблен.

Полный adapter unit suite — 80/80; focused type-description — 11 групп,
native type parser — 15 групп. Focused группы входят в полный adapter count.
Проверены обе формы TypeSet, QName alias/default namespace, mixed alternatives
и порядок, object/register consumers, отрицательные Type/foreign/unknown cases.
Adapter fmt/check/strict all-target/all-feature Clippy и native strict Clippy
прошли. Источник ошибки установлен на собственной копии реального большого CF;
исходные проекты не изменялись.

Первый отказавший большой прогон занял около 19 секунд с пиком RSS около
366 MiB и не принят как успешная композиция. Наличие этого исправления само
по себе не подтверждает готовность всей большой CF/CFE, matching native пары,
платформенного выполнения или полного MVP. Дальнейшие проверки фиксируются
отдельно от этих unit/native parser результатов.

После исправления immutable release binary повторно прочитал тот же owner:
его bytes/hash не изменились, parse issues — 0, модель стала Ready, adapter issue
исчез. Адресное чтение заняло около 22 ms.
Одна повторная большая composition попытка остановилась на следующем пробеле:
CommandGroup имеет current parsed input, но ещё Unsupported model. Completion
занял 17,99 s, наблюдаемый RSS peak — 366,61 MiB. Watchdog 60 s/1,5 GiB не сработал,
child завершился штатно; warm requests и field cycles не запускались.

Actual post-request counters получены с одним штатным stale-snapshot retry:
metadata source — 192 828 536 bytes, module text — 69 803 921, query text — 0;
общая source reservation — 262 632 457 при лимите 268 435 456 bytes.
Native heap estimate — 101 167 572 bytes, его coverage неполная и это не RSS.
Source bytes/mode/mtime оригиналов и собственных копий сохранены. Наблюдение
подтверждает исправление AnyRef, но большая композиция остаётся непринятой.
