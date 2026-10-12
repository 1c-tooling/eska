# Карта фактически собранных файлов

[Именованные свойства EF-24](readable-layout.md) используют source map schema 2
с `source_schema_version = 2`. Legacy descriptors сохраняют значение 1;
map schema 1 не принимает descriptor schema 2. Типизированные диапазоны новой
оболочки указывают direct value, preserved XML — native fragment. Правила
fingerprints и выбора accepted/working сохраняются.

`PreparedSource::source_map` возвращает карту полного native inventory:
descriptor XML, BSL, ресурсы, политика поддержки, supplier CF, файлы вне
объявленного состава и пустые каталоги. Пути относительные; абсолютных путей
машины в карте нет. Карта описывает выбранные входы конкретного поколения,
а не текущую рабочую копию после сборки.

`SourceMap` — сериализуемые данные. Десериализация карты не создаёт
`PreparedSource` и не разрешает экспорт. Private proof и проверки фактических
байтов остаются на [границе подготовленной сборки](prepared-inputs.md).

## Происхождение

Для byte-preserved payload записываются `authority` (`working` или `accepted`),
относительный input path, размер, SHA-256, признак BSL-модуля и UUID владельца,
если он определён. Authority берётся из защищённого selector: восстановленный
или запрещённый модуль относится к принятой базе даже при наличии рабочего файла.
Байты payload и input обязаны совпадать.

Map schema 1 сохраняет равенство native/input paths для исходных layouts 1/2.
[Layout 3](resource-layout.md) использует map schema 2: native `MappedFile.path`
и физический `origin.input.path` различаются, fingerprints/offsets совпадают.
`source_schema_version` описывает descriptor envelope и остаётся 1; это не
версия reverse map из `source.json`. Prepared artifact schema остаётся 2.

Для metadata XML записываются UUID, принятый YAML input, имеющийся рабочий YAML
input и `uses_working_values`. Принятый состав и native bindings сохраняются;
флаг отмечает отличие выбранных разрешённых значений или
[порядка существующих детей](child-order.md) от базы. Это составное
происхождение: флаг не объявляет весь рабочий YAML собранным или разрешённым.
Координаты свойств после перестановки разрешаются по UUID рабочего объекта.

Карта содержит версии map/source/rules, профиль, generation, рабочую ревизию,
base pin и native revision. Файлы и каталоги упорядочены по native path.
`validate` проверяет схему, fingerprints, полный inventory, родительские
каталоги, коллизии путей и descriptor UUID, форму origins и root configuration.
Это проверка внутренней согласованности данных, а не аутентификация поставщика.

`checksum` вычисляет SHA-256 компактного JSON после проверки DTO. Порядок полей
задан Rust-схемой, порядок массивов — native paths; checksum не зависит от
локали и местоположения checkout. Внешний JSON сначала десериализуется
в `SourceMap`, затем проверяется и сериализуется этим же методом.

## Manifest и границы

Prepared artifact manifest schema 2 теперь дополнительно содержит
`source.source_map` и `source.protected.source_map_checksum` с алгоритмом
`sha256`. Обычный Designer artifact manifest schema 1 не меняется.
Карта берётся из подготовленного входа после проверки частного native snapshot,
а не повторного чтения рабочей копии. Снимок manifest хранит одну копию DTO;
публикация JSON заимствует её.

Это карта файлов. [Координаты YAML/BSL](source-locations.md) вычисляются по запросу
из byte-verified inputs; [адреса сообщений платформы](platform-locations.md)
сопоставляются с этой картой в callback сборщика.
Для составного metadata XML нельзя переносить XML line number на YAML.

## Проверки

Полный набор fmt/check/strict Clippy/test/Rustdoc/diff check прошёл:
314 unit и 525 integration в приложении, 125 unit в минимальном source core,
137 unit в build core. Пропущены соответственно 14, три и три opt-in теста.
Тесты проверяют рабочие правки, восстановление отсутствующего модуля,
запрещённый модуль с существующим рабочим кандидатом, round trip JSON/checksum,
подмену origins, потерю файла/родительского каталога и повторный UUID.

Независимый consumer собрал C1 на 1С 8.3.27.2325 с этой картой. CF содержит
200 417 347 байт; SHA-256 CF и карты независимо пересчитаны и совпали с manifest.
Все 8 778 native файлов и 8 077 каталогов сопоставлены с поколением;
проверены размеры и SHA-256 каждого native файла и всех authoring origins.
Учтены 3 595 metadata descriptors, 5 183 payloads, включая 1 979 BSL-модулей.
В этом неизменённом входе payloads выбраны из рабочей копии; разрешённые и
восстановленные варианты проверяются отдельными тестами.
Пять предупреждений 1С относятся к ссылкам справки. Проверка не включает
применение к БД, клиент или полный Designer CheckConfig.
