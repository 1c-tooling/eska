# Координаты выбранных исходников

`MappedFile::locate` сопоставляет byte span из конкретного native файла
с BSL/ресурсом или владельцем YAML. Это чистый Rust API: caller передаёт
native bytes и, для metadata, `OriginTexts` с принятым YAML и при необходимости
рабочим YAML. Метод не читает файловую систему, проект, Git или платформу.
После десериализации внешней карты caller сначала вызывает `SourceMap::validate`.

До разбора проверяются размер и SHA-256 native bytes. Принятый YAML проверяется
по своему origin; рабочий — когда выбранное значение требует его. Изменённые
байты дают `source_map_input_changed`, без чтения более нового файла или XML
fallback. Native XML использует существующие пределы размера/глубины и числа
узлов. Его root kind/UUID обязаны соответствовать принятому descriptor.

## Сопоставление

BSL и byte-preserved ресурсы возвращают ту же координату и выбранный
`working`/`accepted` input. Диапазон относится к записанному checksum;
consumer проверяет текущие дисковые/несохранённые байты перед показом координаты.
Метод не объявляет текущий рабочий модуль совпадающим с собранным.

Metadata XML читается по реальным объектам, properties и native UUID.
Координата структуры относится к принятому YAML. Для существующего typed
property сравниваются выбранное native значение и принятое значение:
отличающееся значение ведёт к рабочему YAML, неизменённое — к принятому.
Working sections/properties могут быть переставлены: адрес вычисляется по
реальным полям и identity, а не старым индексам. CST и strict decoder общие
с reader/editor; native typed values читает существующий XML parser.

`SourceLocation` содержит authority, полный `InputFile`, UUID и optional
диапазон. Диапазон metadata указывает на YAML mapping объекта, scalar текста,
mapping nil/empty/localized значения либо XML scalar opaque property.
Вложенная ошибка opaque XML указывает на весь сохранённый fragment;
XML column не переносится на YAML column. Диапазоны YAML — UTF-8 bytes;
для binary payloads это обычные byte offsets. Преобразование в line/column
или LSP encoding остаётся у потребителя.

Если разрешённое значение выбрано при восстановлении изменённого working UUID
или пути, а надёжной object coordinate нет, возвращается working file origin
без диапазона. Prolog/envelope XML также дают file-level accepted origin.
Неоднозначная identity отвергается. Это сведения о выбранном вводе,
не разрешение изменять или экспортировать его.

Карта manifest остаётся file-level schema 1: координаты вычисляются по запросу
из byte-verified texts, новая копия всего CST в manifest не сохраняется.
Автоматический разбор сообщений платформы и публичный CLI ещё не подключены.

## Проверки

Прошли полный набор fmt/check/strict Clippy/test/Rustdoc/diff check и отдельные
source/build сборки: 321 unit + 525 integration в приложении, 132 unit в source
core и 144 unit в build core; пропущены 14, три и три opt-in теста.

Семь новых тестов проверяют inline/external descriptors, opaque QName,
Unicode/CRLF/comments, переставленные поля, рабочие и принятые значения,
localized/nil, восстановленный UUID, BOM/CRLF BSL, изменённые bytes, отсутствующий
working text, неверные границы, UTF-8 boundary, foreign root kind и XML depth limit.

Независимый consumer с `default-features = false` проверил реальное поколение
C1 из принятой сборки 1С. Совпали координаты native `Name` в принятом YAML
всех 3 595 descriptors и byte coordinates всех 5 183 payloads, включая
1 979 BSL-модулей. Размер/SHA-256 native bytes и запрошенных YAML проверял mapper;
source-map checksum проверен отдельно. Это файловая приёмка координат,
не повторный запуск платформы или проверка клиента.
