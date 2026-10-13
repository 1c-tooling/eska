# Readable свойства managed form — EF-12, часть 4

[Body schema v2](managed-form-body-v2.schema.json) добавляет `properties` формы
и `elements.<ID>.properties`. Definitions теперь имеют тип `FormDefinition`;
readonly identity/layout model остаётся `FormElement`/`ManagedFormLayout`.
Это изменение экспериментального Rust API. [Пример](managed-form-body-v2.example.yaml)
создан самим codec; прежние body v1/schema/example продолжают читаться.
Новый импорт dialog codec создаёт v2, v1 не получает свойства автоматически.

| Свойство | Editable value | Принятые native slots |
|---|---|---|
| Title | `text.<language>` и optional bool `formatted` | Уникальный прямой logform Title, v8:item/lang/content, прежний состав языков/наличие formatted |
| ToolTip | `text.<language>` | Уникальный прямой logform ToolTip без formatted, прежние языки |
| Visible / ReadOnly / Enabled | YAML bool | Уникальный прямой leaf без attributes; native true/false |

Значение существует только в property map. Native stencils содержат XML оболочки,
language bindings и placeholder bool/formatted/text; прежний caption/flag не
сохраняется вторым editable XML значением. Root и UI properties имеют отдельные
scopes, даже если их текст или decimal ID реквизита совпадают.

## Обратимость и ограничения

Promoter принимает property только при byte-equal no-op её inverse writer.
Неизвестные attrs/shapes, foreign localized items, duplicate property names,
неканонические boolean lexemes и непринятый CDATA остаются целиком opaque.
Такое свойство не появляется в editable map; нормализации или raw fallback
для уже promoted value нет. Текст использует общий XML escaping с сохранением CR.

`validate_change` разрешает существующие property values/layout при прежних
native stencils, ID/names/classes/references и data/namespace contexts.
`validate_layout_change` дополнительно запрещает property changes.
Новый язык, удаление language/property, изменение вида bool/localized и
добавление/удаление formatted slot без inverse binding возвращают
`source_form_property_binding_invalid`.

Native empty content может развернуться при изменении текста; empty formatted
Title без items сохраняет пустой map и собственный flag. Новый property slot
не создаётся. Невалидные XML characters отвергаются до materialization.
Это codec validation, не permission поддержки; project file map, единый
form.yaml и protected selection/source map ещё не подключены.

## Проверки C1

Все 823 dialogs снова прошли byte-equal XML → source YAML → XML.
Promoted весь наблюдавшийся direct-property набор: 9 535 Title, 1 401 ToolTip,
455 Visible, 863 ReadOnly и 63 Enabled — всего 12 317 properties.
В 751 форме изменены 14 010 texts/flags, затем проверены inverse и повторный
parse с теми же values/definitions/layout. Input files сохранились.
Это массовый codec probe, новый CF/client/runtime цикл не выполнялся.
C2/C3 этим результатом не принимаются.

Восемь tests проверяют единственный value/no-op, bool/formatted/text edits,
Unicode/XML escaping, empty content, root/element scopes, missing binding/type/
language/flag отказ, unknown/duplicate shapes, недопустимые XML characters и
контракт v1. Оба synthetic examples проверены по своим JSON Schemas.
[Обезличенные результаты](managed-form-properties-results.json).

Полные fmt/check/strict Clippy/test прошли после последней правки. Дополнительно
прошли all-features/minimal/project/build CLI, Rustdoc, SDK/adapter feature gates.
Приложение: 621 unit/570 integration, 24 прежних opt-in ignored; CLI: 44/9;
SDK: 359; adapter: 70 unit/12 integration.

Следующее — соединить body v2 и metadata descriptor в одном form.yaml с
явной обратной картой, затем protected selection, точечные edits/source map,
миграция и реальный property/layout цикл через платформу.
