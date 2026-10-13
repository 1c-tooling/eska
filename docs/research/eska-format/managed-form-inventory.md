# Native ID и ссылки управляемых форм — EF-12, часть 1

Прочитан текущий C1 после CF dump 1С 8.3.27.2325; form payloads не менялись
при EF-24. Найдено 823 XML формы с root `{http://v8.1c.ru/8.3/xcf/logform}Form`
и `version="2.20"`. Исходный raw корпус содержит 825; два отсутствующих в
платформенном dump payload не входят в эту матрицу. Это инвентаризация, typed
YAML/import/writer и новая платформенная приёмка формы здесь ещё не реализованы.

[Численные результаты](managed-form-inventory.json).
Воспроизводимый [скрипт](scripts/inventory_forms.py) читает native каталог и
выводит обезличенный JSON без имён формы/элементов, UUID и путей. Native bytes
повторно проверяются после чтения. Синтетические namespace/column/reference
fixtures проверены в собственном каталоге playground; C2/C3 не исследованы.

## Области идентичности

| Область | Наблюдение | Правило для модели |
|---|---|---|
| UI | 52 502 ID: 18 571 в ChildItems, 33 931 в фиксированных slots | Один UI namespace формы включает обычные и специальные элементы |
| Form Attributes | 4 971 root declarations; повторов ID/имён нет | Отдельный namespace; не metadata UUID и не UI ID |
| Form Commands | 2 303 declarations; повторов ID/имён нет | Отдельный namespace; ссылки Form.Command.* разрешаются здесь |
| Data columns | 2 028 declarations в 322 непосредственных контейнерах | ID/имя проверяются в конкретном native контейнере; не общим списком формы/Attribute |
| AutoCommandBar | 823 root элемента с ID -1 | Специальный корневой slot; unsigned-only ID запрещает корректные native формы |

ID UI/Attribute совпадают в 689 формах, UI/Command — в 445,
Attribute/Command — в 552. Между разными column containers ID повторяются
в 65 формах и внутри рекурсивного состава 11 Attributes; внутри каждого
непосредственного контейнера повторов ID/имён нет.

Глобальная уникальность UI **имён** не подтверждается: семь повторов относятся
к fixed additions/context menu/tooltips. Два AutoCommandBar имеют пустое имя.
Поэтому stable UI identity — native ID/роль; fixed names нельзя автоматически
считать единственным ключом или исправлять при импорте.

## Композиция UI

Встречаются 25 классов regular ChildItems и шесть fixed classes; два класса
additions встречаются в обеих ролях. Main elements: CommandBar, Button,
UsualGroup, Table, поля/декорации, Pages/Page, ColumnGroup, Popup/ButtonGroup,
SearchStringAddition/SearchControlAddition и specialised document fields.
Полная count/membership matrix находится в JSON, без вывода семантики из suffix.

Fixed slots включают AutoCommandBar формы/таблицы, ContextMenu, ExtendedTooltip,
табличные SearchStringAddition/ViewStatusAddition/SearchControlAddition.
У additions есть собственные menu/tooltip ID. Их ChildItems остаются ordered
layout командных элементов; сами fixed edges не становятся свободными moves.

Для EF-12 нужны отдельные определения elements и layout, где обычный child
имеет одного родителя, а fixed slot сохраняет actual owner/роль. Проверять
уникальность UI ID, достижимость, циклы, dangling references и nesting matrix.
Pages принимает Page; Table/ColumnGroup, командные containers и обычные группы
имеют разные child domains. Наблюдавшийся parent/child pair — evidence конкретного
профиля; общий XML node не становится пользовательским синтаксисом.

## Ссылки и непринятые домены

Все 1 822 AdditionSource.Item разрешаются к именованной regular Table.
Все 2 804 Form.Command.* в CommandName разрешаются к локальным Commands.
Есть ещё 616 Form.StandardCommand.*, 99 numeric и 875 других CommandName.
Число в CommandName/Command не доказано ссылкой на UI ID; так его не трактуем.

Из 6 713 DataPath: 6 557 имеют head принятого root Attribute, 58 — Items,
37 — indexed head, 36 — marked head, 25 — native identity path.
Наличие head не подтверждает поля metadata/type, runtime CurrentData или весь
диалект. Непринятый selector сохраняется целиком; автоматическое разбиение по
точке/подмена ID недопустимы. Data context Table сохраняется при moves, пока
нет правил переноса/изменения DataPath между такими областями.

Title содержит v8:item; в 1 700 случаях есть formatted=true/false, включая
семь пустых formatted Title. ToolTip тоже локализован. Visible/ReadOnly/Enabled
встречаются отдельными scalar leaves. Полная property/type/enum matrix и rules
изменения ещё нужны; Namespace/QName и отсутствие значения не заменяются defaults.
Commands/Attributes/Columns, Events, Parameters, CommandSet/CommandInterface,
DCS Settings и custom appearance имеют отдельные inverse rules. Они не становятся
редактируемыми из-за успешного parse их XML envelope.

## Следующие части EF-12

1. Общая typed identity/layout модель и валидатор, адресное чтение реального
   declared dialog через SourceReader; фиксированные slots/области ID учитываются.
2. Lossless import/writer и единый form.yaml вместе с metadata descriptor:
   единственные values, native bindings/unknown fragments отдельно, без второй
   editable XML копии. Модуль остаётся отдельным файлом, query slots — EF-23.
3. Protected selection, точечные CST edits/source map, миграция и no-op/repeat:
   права Form UUID, строгий отказ при неизвестной семантике/невалидном graph.
4. Общий SDK потребитель и реальная property/layout правка с независимым XML
   контролем, CF/build/dump и проверками native ID/references.

Это обязательная текущая очередь EF-12; статус — IN_PROGRESS.
Добавление/удаление/rename элементов и переходы между непринятыми data contexts
не получают разрешения из generic XML walker. Обычные бинарные формы остаются
ресурсами; визуальный редактор/merge-driver не входят в эту задачу.
