# Native slots и обратимый query codec — EF-23, часть 1

Общий `project_core::source_format` предоставляет `NativeQueryDocument`,
`NativeQuery` и `QuerySlot`. Это экспериментальный pure Rust API без выбора
путей, файловой publication, supplier authority или анализа языка запроса.
Проектные schemas и CLI defaults этой частью не меняются.

## Привязка к native владельцу

| Native профиль | Извлекаемый slot | Identity в пределах ресурса |
|---|---|---|
| DataCompositionSchema, namespace `http://v8.1c.ru/8.1/data-composition-system/schema` | Прямой `query` у `dataSet`/union `item` с qualified type `DataSetQuery` | Полная цепочка `name` наборов, включая parent unions |
| Form logform 2.20 | `Attributes/Attribute/Settings/QueryText`, qualified `DynamicList`, `ManualQuery = true` | Native ID корневого Attribute; он независим от UI/Command ID |

`DataSetUnion` обходится итеративно; `DataSetObject` без запроса не создаёт slot.
Одинаковые имена в разных unions допустимы, case collisions siblings отказаны.
Namespace qualified type разрешается через реальный scope узла; знакомое имя
prefix не даёт право извлечения. Двойные direct query/name/settings slots и
неизвестные dataset types вызывают отказ. Изменение имени набора меняет native
binding; автоматическое следование старого файла за rename этим API не заявлено.

Автоматический DynamicList с `ManualQuery = false` без QueryText не создаёт
запрос. Неоднозначный ID, отсутствующий manual query text или несогласованный
flag/text shape отказывают. Другие одноимённые XML properties не считаются
запросом динамического списка. Metadata UUID/путь ресурса добавляет проектный
слой; offsets и короткие names этого pure codec не дают project authority.

## Текст и обратная запись

Native gaps не содержат извлечённые query values. У каждого slot один decoded
text и проверенный inverse frame: точные opening/closing tags, existing empty
shape, encoding и native EOL. Отдельная mutable копия текста в XML не хранится.

При promotion frame обязан восстановить native bytes. Plain XML text,
наблюдавшиеся quote/apostrophe/CR entities и canonical CDATA поддержаны;
CDATA termination и actual CR имеют явные inverse sequences. Uniform native
LF/CRLF/CR отделены от logical EOL текста: literal XML transport нормализуется
XML parser, actual CR сохраняется character reference. Остальные bytes,
namespaces, comments вне query и неизвестные XML sections остаются прежними.

`materialize` требует ровно все declared bindings. Missing/extra values и XML
control characters отказывают; скрытого восстановления текста нет. Результат
повторно разбирается и сверяется по каждому scoped binding/value. Комментарии
языка запроса, пакет, Unicode, quotes и параметры не форматируются. XML comments/
PI внутри самого query, mixed native EOL и неподтверждённые lexical shapes
отказывают до получения separately editable value.

Функции exact opening QName/end теперь общие с managed form codec; его прежние
ошибки и поведение сохранены. XML character validation также общая.

## Проверки

[Результаты C1](native-query-codec-results.json) получены из свежей платформенной
выгрузки предыдущей [приёмки формы](managed-form-acceptance.md), в отдельном
playground run. Прочитаны 823 формы и 51 схема СКД. Извлечены 27 запросов СКД
(четыре union members) и 123 запроса списков: 24 schemas и 102 forms содержат
запросы, всего 126 ресурсов. Все native no-op совпали. Во всех 150 запросах
изменён текст, inverse/reparse сохранили точные значения и прочие native bytes;
исходные файлы не изменились. Это codec probe, новый CF/execution цикл запросов
не выполнялся.

Восемь тестов покрывают native scopes/namespace aliases, union collisions,
automatic/manual lists и UI ID overlap, пакеты/comments/XML entities/logical
EOL, native LF/CRLF/CR, CDATA/empty slots, missing/extra bindings, invalid XML
characters и unverified profiles/trivia. Обязательные fmt/check/strict Clippy/
test и Rustdoc прошли: 669 unit/570 integration, 24 прежних opt-in ignored.
SDK all features check/Clippy/test/Rustdoc также прошли, 359 тестов.

Далее — версия source references, пути `.sdbl`, общий reader/import/export,
protected source/base selection, source map и миграция, затем независимый
CF/build/dump и выполнение выбранного отчёта/списка. EF-23 остаётся `IN_PROGRESS`;
анализ языка и зависимостей — EF-27.
