# Адреса файлов в сообщениях платформы

`build_core::execute_prepared_with_locations` выполняет тот же защищённый
конвейер, что `execute_prepared_with_manifest`. Callback получает stage,
stream, исходные bytes строки и optional `&MappedFile` из private prepared map.
Существующие трёхаргументные callbacks сохраняют signatures и прежние bytes.
Artifact manifest schema и выбор исходников не меняются.

Адрес сопоставляется только на ImportSources и относительно фактически
скопированного snapshot root. Создание ИБ, сообщения о внешних файлах и
неизвестные адреса не получают origin из другого проекта или старого XML.
Callback вызывается и для строк неуспешного процесса; исходная ошибка запуска
и весь raw output сохраняют прежнее поведение.

`locate_output_file` — чистый helper сборочного слоя, без файлового I/O.
Caller передаёт уже проверенную карту и абсолютный root. Поддерживается
UTF-8 grammar `[WARN|ERROR|FATAL] File: <absolute path>, <message>`.
Сопоставление использует точный root и portable relative paths; alias/traversal,
отсутствующие файлы и неподдержанная форма не трактуются как знакомый адрес.
Comma внутри filename допустима, но несколько подходящих известных префиксов
дают отсутствие annotation, без выбора по догадке. Raw строка остаётся доступной.

Payload origin указывает на выбранный ресурс или BSL из рабочей копии/базы.
Metadata origin указывает на принятый YAML и имеющийся рабочий кандидат;
адрес файла сам по себе не доказывает, какое свойство вызвало сообщение.
Если consumer отдельно получил надёжный native byte span, он использует
[сопоставление координат](source-locations.md) с проверкой точных bytes.

Это экспериментальный Rust API. Строки/колонки из произвольного platform
output ещё не разбираются; неподдержанные языки/encodings не получают
выдуманную привязку. Публичный CLI и workspace format selection пока не подключены.

## Проверки

Прошли полный набор fmt/check/strict Clippy/test/Rustdoc/diff check и отдельные
source/build сборки: 324 unit + 525 integration в приложении, 132 unit в source
core и 147 unit в build core; пропущены соответственно 14, три и три opt-in теста.
Три новых теста проверяют grammar, Unicode/spaces, относительные separators,
foreign roots, alias/traversal, отсутствующий файл, encoding, относительный root
и неоднозначные comma filenames. Это адресные fixtures, не export proofs.

Независимый build consumer выполнил новый callback route на C1 и установленной
1С 8.3.27.2325. Все пять реальных предупреждений о ссылках справки получили
ожидаемые payload origins; callback проверил stage и точное совпадение записи
с private prepared map. CF содержит 200 417 603 байта, SHA-256 совпал с manifest.
Source context, map и snapshot id совпали с предыдущей принятой сборкой.
Это приёмка реальных file-address annotations; ошибка с line/column,
применение к БД, клиент и полный Designer CheckConfig здесь не проверены.
