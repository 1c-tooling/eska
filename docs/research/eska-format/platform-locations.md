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
UTF-8 grammar `[WARN|ERROR|FATAL] File: <absolute path>, <message>` и две
наблюдённые формы ошибки чтения XDTO:

```text
[ERROR] XDTO exception while reading file <absolute path>. Error converting XDTO data: <message>
[ERROR] An error occurred while importing the configuration from XML XDTO exception while reading file <absolute path>. Error converting XDTO data: <message>
```

Сопоставление использует точный root и portable relative paths; alias/traversal,
отсутствующие файлы и неподдержанная форма не трактуются как знакомый адрес.
Comma внутри filename допустима, но несколько подходящих известных префиксов
дают отсутствие annotation, без выбора по догадке. То же правило применяется
к разделителю XDTO. Raw строка остаётся доступной.

Payload origin указывает на выбранный ресурс или BSL из рабочей копии/базы.
Metadata origin указывает на принятый YAML и имеющийся рабочий кандидат;
адрес файла сам по себе не доказывает, какое свойство вызвало сообщение.
Если consumer отдельно получил надёжный native byte span, он использует
[сопоставление координат](source-locations.md) с проверкой точных bytes.

Это экспериментальный Rust API. Проверенные сообщения 1С содержат адрес файла
без строки/колонки. Они получают file-level origin; строка из текста значения
и XML column не выдаются за координату YAML. Неподдержанные формы,
языки/encodings сохраняются без выдуманной привязки. Публичный CLI и workspace
format selection пока не подключены.

## Проверки

Прошли полный набор fmt/check/strict Clippy/test/Rustdoc/diff check и отдельные
source/build сборки: 325 unit + 525 integration в приложении, 132 unit в source
core и 148 unit в build core; пропущены соответственно 14, три и три opt-in теста.
Четыре теста проверяют grammar, Unicode/spaces, относительные separators,
foreign roots, alias/traversal, отсутствующий файл, encoding, относительный root
и неоднозначные comma filenames, прямую и обёрнутую ошибку XDTO и отказ
незнакомой обёртке. Это адресные fixtures, не export proofs.

Независимый build consumer выполнил новый callback route на C1 и установленной
1С 8.3.27.2325. Все пять реальных предупреждений о ссылках справки получили
ожидаемые payload origins; callback проверил stage и точное совпадение записи
с private prepared map. CF содержит 200 417 603 байта, SHA-256 совпал с manifest.
Source context, map и snapshot id совпали с предыдущей принятой сборкой.
В отдельной собственной копии C1 разрешённый ресурс формы изменён сначала
до невалидного XML. Защищённая материализация отклонила его до запуска 1С.
Затем XML оставлен корректным, но числовой атрибут получил текстовое значение.
Установленная платформа отвергла ImportSources с двумя приведёнными сообщениями
XDTO. Повтор через новый callback проверил оба origins из prepared map;
CF и artifact manifest отсутствуют. База и исходный стенд не изменялись.

Это приёмка реальных file-address annotations при успехе и отказе. Ошибка
с line/column, применение к БД, клиент и полный Designer CheckConfig здесь
не проверены; подтверждённый вывод платформы координат не содержит.
