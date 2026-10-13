# Точечные правки управляемых форм — EF-12, часть 7

`prepare_form_edit` готовит saved edit единого `form.yaml` и возвращает общий
`SourceEditPlan`. `prepare_form_edit_text` работает над точными editor bytes;
`FormEditPreview` предоставляет accepted path/pin, изменённый текст и no-op,
но не имеет метода публикации. CLI и IDE protocol этой частью не добавлены.

`FormEdit::Property` выбирает существующее свойство формы или UI element по
`Option<FormId>`, `FormPropertyName` и полному `FormPropertyValue`.
Для localized value можно изменить тексты принятых language keys и существующий
formatted flag. Bool сохраняет вид native slot. Добавление/удаление свойства,
языка, flag или изменение типа значения отклоняется.

`FormEdit::Layout` задаёт проверенный layout целиком; editor заменяет только
изменённые существующие roots/children collections. Definitions и properties
остаются в исходных байтах. Перемещение в существующий пустой ChildItems разрешено;
новые slots, изменение fixed bindings, потеря/дублирование ID, циклы и перенос
в другой data/namespace context не разрешаются.

## Принятая identity и публикация

Разрешение `Resource` берётся по metadata UUID формы из независимо принятой базы.
Native descriptor, owner path и dialog binding выбираются по pinned map 4.
Текущий YAML сохраняет состав descriptor частей, UUID/kind/Name владельца и body
schema/native stencils. Native запреты и inherited freeze проверяются также
перед публикацией. Короткое имя, путь из YAML и decimal UI ID не дают прав.

Рабочая форма проверяется до операции, включая no-op; editor не восстанавливает
недопустимые изменения молча. [Protected writer](managed-form-protected.md)
сохраняет свою независимую функцию выбора разрешённых значений при сборке.

Saved writer повторно проверяет baseline pin/storage и exact current bytes,
использует прежнюю атомарную замену и отказывает при конфликте, read-only или
небезопасном пути. Preview не читает source filesystem и не проверяет mutable
baseline storage; IDE consumer должен независимо проверить pin и document version.
Сравнение перед rename не является OS-level compare-and-swap с посторонним writer.

## Локальность YAML

CST адресует точные mapping keys UI ID/language и existing value slots, без
поиска текста. Каждый изменённый scalar имеет исходный byte span; patches
применяются с конца, не меняя соседние values/comments. Literal transport
сохраняется отдельно от соседней строки. Полный strict reader сравнивает
результат с ожидаемой моделью до получения saved plan.

Одинаковое логическое значение сохраняет все байты, spelling bool и mtime.
LF/CRLF и допустимые scalar styles сохраняются. Empty collection остаётся `[]`.
Комментарии существующих layout позиций остаются на своих позициях; удаление
annotated block entry и потеря комментария отклоняются. Смешанные LF/CRLF
в изменяемом поддереве и неподдержанная CST запись также вызывают отказ.
Общие ограничения YAML профиля, включая запрет BOM, остаются прежними.

## Граница части

[Часть 8](managed-form-migration.md) добавляет source/base/project/workspace
migration. Для завершения EF-12 нужен независимый платформенный цикл изменённой
формы. CLI defaults и analyzer/Explorer API пока прежние.
Generic UI create/delete/rename и визуальный редактор не добавлены.

## Проверки

Добавлены девять тестов. Они проверяют точность scalar patches, flow/literal
styles и LF/CRLF, комментарии и пустые коллекции, preview/no-op/mtime,
перемещение в существующий пустой slot и изменение только native dialog,
native policy/inherited freeze, current-byte conflict и independent baseline pin,
неподдержанные slots/languages/types/identity и невалидный layout.

[Результаты C1](managed-form-edit-results.json): 186 разрешённых форм прошли
exact no-op previews; 637 запрещённых форм получили отказ по native policy.
Проверены 166 property previews с 168 изменёнными scalars, 169 layout previews
и четыре saved publications. Strict protected export прошёл без exclusions;
из 8 769 native файлов изменились только 183 dialog XML. Принятый pin сохранён.

Обязательные fmt/check/strict Clippy/test и Rustdoc прошли: 653 unit,
570 integration, 24 прежних opt-in теста ignored. Check/Clippy/test/Rustdoc
SDK с all features также прошли, 359 тестов. C2/C3 и платформенная приёмка
изменённых форм этой проверкой не подтверждаются.
