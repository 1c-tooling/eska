# Защищённый экспорт управляемых форм — EF-12, часть 6

Protected writer принимает [обратную карту 4 и единый form.yaml](managed-form-project.md).
Принятый metadata UUID формы определяет права `Resource` на body. Decimal UI ID,
короткое имя, путь и изменённый UUID рабочего YAML не становятся авторитетом.
Нативные запреты поставщика и freeze самой формы/её metadata предков сохраняются.

## Выбор содержимого

Selector читает каждый owner один раз. Metadata descriptor и body выбираются
из одной captured ревизии, затем общий writer создаёт оба native XML файла.
Модули и payload resources проходят прежний независимый выбор прав и байтов.

Для разрешённой формы выбираются значения существующих Title/ToolTip/bool slots,
тексты принятых language keys, существующий formatted flag и проверенный layout.
Class/name/ID definitions, native stencils, набор свойств/языков и schema остаются
из базы. Их подмена восстанавливается независимо от разрешённых значений.
Неверное разрешённое значение, цикл, потеря ID или недопустимое перемещение
останавливают generation до публикации. Generic UI create/delete/rename не добавлены.

Запрещённый body восстанавливается без интерпретации его значений как прав.
Отсутствующий body/owner также восстанавливается с явной диагностикой.
Строгий режим отказывает при exclusions; обычный создаёт поколение из выбранных
значений, не переписывая рабочий YAML. Повторная generation и private build proof
сохраняют прежние проверки полного native inventory и обеих input authorities.

## Source map 3

`MANAGED_FORM_SOURCE_MAP_VERSION = 3` добавляет origin `managed_form` с metadata
`uuid`, native `descriptor`, captured `accepted`/`working` inputs и независимым
`uses_working_values`. Descriptor origin остаётся `metadata`: оба native файла
используют тот же YAML и UUID, но один может содержать рабочие правки, а другой — нет.
`source_schema_version = 2` описывает descriptor envelope; reverse map остаётся 4.
Прежние source maps 1/2 и prepared artifact envelope 2 сохранены.

Проверка DTO связывает dialog с точным native descriptor и одинаковыми captured
inputs; старый map version не принимает новый origin. Десериализация карты не
создаёт `PreparedSource` и не разрешает экспорт.

Адресный mapper проверяет native/input fingerprints и UTF-8 границы. Он
восстанавливает выбранный XML через принятые stencils, затем разрешает текущие
language text/bool/formatted values и переставленные layout entries в Working.
Неизменённые значения и definitions относятся к Accepted. Новая позиция в
существующем пустом ChildItems не требует отсутствующего accepted span.
Координаты получаются из CST по full form identity, UI scope и language key,
без поиска строки в YAML. Metadata origin не интерпретирует неподдержанные
body edits; они не скрывают независимую разрешённую правку metadata property.

## Граница части

[Точечный editor API](managed-form-edit.md) добавлен в части 7.
Позднейшая [часть 8](managed-form-migration.md) добавляет source/base/project/workspace
migration; прежние targets сохраняют map 4 как byte no-op.
CLI import defaults не менялись. Этот этап не подтверждает платформенную
приёмку изменённых форм или готовность Explorer/analyzer overlay.

## Проверки

[Результаты C1](managed-form-protected-results.json) получены в отдельной копии
playground. Все 823 формы прошли protected no-op. Права разрешили 186 форм;
правки 2 720 значений и 169 перестановок изменили 183 native dialog XML.
Содержимое 637 запрещённых форм восстановлено; ещё три разрешённые формы
не содержали выбранных правок. Строгий режим отказал до публикации,
повторная generation совпала. Проверены все native/input fingerprints и
20 адресных Working координат; исходная native выгрузка не изменена.

Добавлены 10 тестов: no-op/repeat/DTO, разрешённые values и move в empty slot,
восстановление bindings с независимыми правками metadata, native policy и
inherited freeze, пропущенный body/owner, неверное значение/graph, подмена
связей origins, fingerprints, UTF-8 spans и независимые Attribute/UI scopes
с одинаковым decimal ID. Прежний test отказа protected map 4
заменён проверкой оставшейся границы migration. Обязательные fmt/check/strict
Clippy/test прошли: 644 unit, 570 integration; feature matrix, Rustdoc,
SDK и adapter также прошли. Всего 16 root и 19 consumer gates, 24 прежних
opt-in теста ignored в default наборе. После последнего теста обязательные
четыре проверки повторены успешно. C2/C3 и новый платформенный цикл
изменённых форм в эту проверку не входят.
