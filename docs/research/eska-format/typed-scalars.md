# Типизированные скаляры, первая часть EF-24

Общий source core импортирует и экспортирует 32 типизированных свойства:
четыре прежних и 28 новых. Это расширение экспериментальной схемы 1, не
завершённая структура EF-24. Разделение descriptors, новая читаемая оболочка,
составные типы/ссылки/коллекции, операции с составом и миграция ещё не выполнены.

## Матрица

Каждое новое свойство принимается только в перечисленных классах. Совпадение
имени XML-тега или текста с bool/числом не определяет тип свойства.

| Класс владельца | Свойства | Домен |
|---|---|---|
| CommonModule | Global, ClientManagedApplication, ClientOrdinaryApplication, Server, ExternalConnection, ServerCall, Privileged | boolean |
| Catalog, ChartOfCharacteristicTypes | Hierarchical | boolean |
| Catalog, ChartOfCharacteristicTypes, Document | Autonumbering, CheckUnique | boolean |
| Catalog | LimitLevelCount | boolean |
| Attribute, CommonAttribute, Constant, Dimension, Resource | MultiLine, PasswordMode, MarkNegatives | boolean |
| Attribute, CommonAttribute, Dimension, Resource | FillFromFillingValue | boolean |
| Catalog, ChartOfCharacteristicTypes, ExchangePlan | CodeLength, DescriptionLength | integer |
| Catalog | LevelCount | integer |
| Document | NumberLength | integer |
| TabularSection | LineNumberLength | integer |
| Catalog | CodeType | String, Number |
| Document | NumberType | String, Number |
| Catalog, ChartOfCharacteristicTypes, ExchangePlan | CodeAllowedLength | Variable, Fixed |
| Document | NumberAllowedLength | Variable, Fixed |
| InformationRegister | InformationRegisterPeriodicity | Nonperiodical, Second, RecorderPosition, Day, Month, Quarter, Year |
| CommonModule | ReturnValuesReuse | DontUse, DuringSession, DuringRequest |
| Attribute, CommonAttribute, Constant, Dimension, Resource, TabularSection | FillChecking | DontCheck, ShowError |
| Attribute, CommonAttribute, Dimension, Resource | Indexing | DontIndex, Index, IndexWithAdditionalOrder |

`Name`, `Comment`, `Version` остаются строками; `Synonym` — упорядоченным набором
языков. `Version` не превращается в число: `x.y.z`, четыре компонента и ведущие
нули сохраняются. Создание/rename и новые правила bump сюда не входят.

## Значения и сохранность

`boolean` использует YAML boolean и точные native `true`/`false`. Другие
native написания (`1`, `False`, пробелы), дополнительные XML-атрибуты,
комментарии внутри элемента, неизвестный enum или неподдержанный владелец
сохраняются как preserved. При ручной записи typed-значения неверный тип,
владелец или enum отклоняются до публикации.

`integer` содержит строку ASCII-цифр. `ExactUnsigned` сохраняет ведущие нули
и любое число цифр; оно не проходит через float/u64. Числовой YAML-токен,
отрицательное/дробное значение, экспонента и знаки не принимаются. Проверка
верхней границы длины кода, совместимости флагов и прочих связей между
свойствами остаётся за платформой. Эта часть не заявляет полную семантическую
валидацию структуры.

Отсутствие свойства, `empty`, `nil` и явные `false`/`"0"` различаются. Writer
не выводит значения по умолчанию и не создаёт отсутствующие свойства. Импорт
применяет новую матрицу; открытие старого YAML сохраняет прежние preserved
значения. Они не становятся редактируемыми typed-свойствами при открытии или
сборке. Их перевод требует отдельной миграции EF-24.

Точечный CST editor использует те же домены и права принятой базы. Он меняет
только выбранное значение, сохраняет комментарии, кавычки строк, CRLF и
соседние байты. Равное значение не записывает файл. Числа и enum остаются
строками; bool заменяется настоящим boolean-токеном. Переходы скаляра/empty/nil
не расширяют разрешения на структурное редактирование.

Protected export применяет разрешённые правки существующих свойств и
восстанавливает запрещённые, включая заморозку владельца. Диагностика содержит
конкретный `property:<Name>`, а не общий resource. Source map указывает точный
скаляр текущего YAML; у старого preserved свойства сохраняется диапазон XML
в принятом YAML. Общая семантическая проекция возвращает прежний native text,
поэтому analyzer/checker не получают новую отдельную модель свойств.

## Проверки

Unit-проверки покрывают XML → YAML → XML, точность чисел, BOM/CRLF, неизвестные
envelopes и enum, разные владельцы и пустые состояния. Integration-проверки
покрывают свойства объекта/реквизита/табличной части, CST/no-op, native support,
заморозку владельца, защиту от raw YAML bypass и текущие координаты source map.
Старое preserved представление отдельно открывается и экспортируется без изменения
исходных байтов. Через прежний analyzer transport проверены boolean-планы в RU/EN:
точечная правка, no-op, запрет текстового значения и отсутствие записи на диск.

Основной набор — 386 unit- и 537 integration-тестов, 15 opt-in проверок
пропущены обычным запуском. Все 15 gates root и 14 gates SDK прошли:
fmt/check/strict Clippy/test/Rustdoc, отдельно minimal/project-cli/build-cli
и minimal/check-cli/analyzer-cli. SDK all-features — 354 теста без пропусков.
JSON Schema и её пример проверены, состав 32 имён совпадает с Rust.

Полная семантическая проекция новой записи C1 после правок совпала с независимым
XML в режимах `All` и `Summary`: 3 595 descriptors и 8 330 объектов. Старые
preserved-проекты C1/C2/C3 совпали с исходным XML в режиме `All`: соответственно
3 595 / 39 572 / 45 708 descriptors и 8 330 / 103 417 / 131 381 объектов.
Эта сверка читает прежние проекты, не переимпортирует и не собирает C2/C3.

Реальный opt-in `real_scalar_acceptance` на C1 импортирует всю конфигурацию,
самостоятельно выбирает разрешённые `MultiLine`, `CodeLength`, `FillChecking`
и готовит независимую контрольную правку XML. Изменились только два ожидаемых
native файла из 8 778; повторные планы не меняли bytes, strict export не дал
exclusions. Входной XML проверен по исходным hashes и пустым каталогам.

Оба маршрута собраны отдельным `eska-build` в CF на установленной платформе
8.3.27.2325 и независимо выгружены через `eska new --from`. В каждой выгрузке
8 769 файлов и 8 064 каталога. Полное сравнение
[`compare_native.py`](scripts/compare_native.py) подтвердило совпадение всех
байтов, кроме 7 011 `configVersion` в `ConfigDumpInfo.xml`; остальные байты
этого файла, ID и порядок сравниваются. Это прежняя узкая оговорка независимых
ИБ [EF-08](ef-08-results.md). Правила поддержки и оба CF поставщиков сохранены
байт в байт. Обе сборки сохранили версию проекта, исходники и accepted base;
контрольные суммы artifacts совпали с manifests.

Это приёмка скалярных правок обычного CF. Обновление ИБ, клиент, полная проверка
BSL и CFE/EPF/ERF этим запуском не проверялись. Новый layout, составные домены
и миграция остаются условиями завершения EF-24. Детальные команды, private пути,
hashes и логи хранятся в собственном каталоге соседнего playground;
[обезличенные результаты](typed-scalars-results.json) сохранены в репозитории.
