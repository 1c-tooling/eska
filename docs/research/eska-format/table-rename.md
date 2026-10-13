# Переименование табличных частей, двадцать седьмая часть EF-24

Один `Name` принятой inline `TabularSection` у `Catalog`/`Document` задаёт
имя таблицы и обеих native declarations. UUID таблицы, колонок и четыре
service UUID сохраняются; пути descriptors/resources и порядок не меняются.
Новая human строка, CLI option, dependency или версия схемы не добавлены.

## Профиль и права

Переименование использует общий [известный native профиль](table-deletion.md):
typed InternalInfo, Name/Synonym/Comment, известные ToolTip/FillChecking/Use/
LineNumberLength, проверенный StandardAttribute LineNumber и inline Attribute
колонки. Непринятые секции, свойства, namespaces, собственные файлы или opaque
service declarations исключают rename; независимые разрешённые свойства
выбираются обычным образом. Старые preserved YAML не преобразуются при открытии.

Нужны Reorder реального принятого родителя и Rename самой таблицы. Native
supplier rules и inherited ESKA freeze проверяются отдельно. Неизменённые колонки
и stationary siblings сохраняют собственные права, но не запрещают чужой Name.
Service UUID не становятся владельцами метаданных. Рабочая подмена UUID,
kind/parentage или служебных слотов не расширяет authority. Rust API Rename для
TabularSection — необходимое условие; фактический parent/profile проверяет selector.

Имя проверяется после всех выбранных правок. Корневые Attribute и TabularSection
делят case-insensitive namespace: пустое имя, неверный identifier и коллизия
отказывают до generation в обоих режимах. Одновременный обмен свободных от
зависимостей имён разрешён. Strict отказывает по заблокированному rename;
`--allow-exclusions` восстанавливает Name/derived names, сохраняя независимые правки.

## Зависимости

Общий guard проверяет выбранные metadata/payload XML и BSL. Старое имя таблицы
инвалидируется с границами identifier; оба старых generated type names также
инвалидируются. UUID таблицы, её колонок и service UUID остаются действующими.
Case-only rename сохраняет прежние case-insensitive references и не запускает
новую проверку payload XML без других инвалидаций.

Type index отвергает отсутствующую цель. Дополнительная проверка typed Type
не позволяет старому table/row имени молча привязаться к другой таблице с новым
UUID, даже если та повторно использует освободившееся имя. Разрешённая правка
альтернативы на уникальное новое имя прежней таблицы проходит. Обмен имён с
оставшимися старыми typed references отклоняется консервативно: автоматическое
сопоставление/переписывание зависимостей не реализовано.

Разрешённая правка XML/BSL может убрать старое упоминание. Заблокированный ресурс
участвует принятыми байтами, поэтому его рабочая правка не скрывает зависимость.
Malformed выбранный XML отказывает до публикации. Поиск имён консервативный,
может находить чужие одноимённые поля; он не является BSL-анализом и не проверяет
computed references, двоичные payload или неизвестные языки ресурсов.

## Источники

Native Name и два атрибута `GeneratedType.name` адресуются к одному точному
Working Name scalar по UUID таблицы. Service UUID spans сохраняют Accepted
origin таблицы. Отказанный rename указывает на принятое Name; working service
edits не становятся accepted authority. Проверяются exact input revision/bytes,
перестановка/удаление соседнего поля и одинаковые saved/overlay All/Summary.

## Проверки

Девять core, один supplier integration и два CLI tests покрывают оба YAML
envelopes, parent/table/column policy, recovery, unknown profiles, коллизии,
name swap/reuse, typed row dependency, selected XML/BSL и source origins.
CLI проверяет exit/stdout/stderr и одинаковый JSON RU/EN.

Прошли 16 root/14 SDK/5 adapter gates и две release-сборки. Default tests:
592 unit + 570 integration, 24 opt-in пропущены; minimal core — 391,
minimal project — 399 unit + 44 CLI; SDK — 359, adapter — 70 + 12.

C1 проверена в собственной копии прежнего native dump. Два Name изменены
адресными CST edits в двух YAML. Independent control изменяет только native
Name и четыре generated name attributes. Все 8 769 файлов/8 064 каталога
совпали после предусмотренных [export contract](export.md) нормализаций:
final EOL у 3 595 descriptors и empty ChildObjects spelling у 106 коллекций.
Других исключений нет; raw platform metadata XML byte no-op не заявляется.

Freeze одной таблицы отказал с её UUID до generation; recovery сохранил её
Name и независимое переименование второй. RU/EN совпали, повтор использовал
поколение. Source/base/input/settings/manifest/pin и original C1 сохранены;
отдельный source-only release CLI работал без platform/Git/facade в PATH.
Все 3 595 YAML/пример прошли локальные schemas. Семантика 3 595 descriptors/
8 346 объектов равна control в saved/overlay All/Summary. Шесть Working spans
Name/derived names и восемь Accepted service spans подтвердили exact origins.

Независимые control/ESKA CF собраны и повторно выгружены на 1С 8.3.27.2325.
Все 8 769 файлов/8 064 каталога совпали, кроме 7 011 configVersion. Все 116
таблиц/464 service UUID и колонки сохранены; изменились только выбранные Name/
derived names. Порядок, остальные свойства/типы, inputs и версия не изменились;
manifest checksums верны. Клиент/рабочая ИБ, C2/C3, CFE/EPF/ERF не проверялись.

[Обезличенные результаты](table-rename-results.json).
EF-24 остаётся `IN_PROGRESS`; эта часть не объявляет готовыми формы или остальные
непринятые structural/compound profiles.
