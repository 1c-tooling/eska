# Единый form.yaml — EF-12, часть 5

Новый opt-in core API `import_designer_forms` создаёт обратную карту
[версии 4](source-manifest-v4.schema.json). Описание метаданных формы и body v2
хранятся в одном `form.yaml`: `document` с readable descriptor v2, пустой
`nested` и `form` с properties/elements/layout/native stencils.
[Схема](managed-form-source.schema.json),
[пример](managed-form-source.example.yaml).

`ObjectSource.form` и `FileRole::ManagedForm` расширяют экспериментальный Rust
API. Старые map 1/2/3, их схемы и defaults импорта сохраняются. CLI пока не
выбирает map 4; включение публичного CLI относится к EF-20 после приёмки модели.

## Авторитет и обратная карта

Native metadata descriptor имеет прежнюю роль `descriptor` с UUID. Для его
`Ext/Form.xml` введена роль `managed_form` с `source` того же YAML и полной
native `descriptor` identity. Совпадение короткого имени или decimal UI ID
не связывает разные формы. Имя native файла учитывает исходную запись регистра.

Только эта проверенная пара может указывать на один физический файл. В map 4
нельзя оставить native dialog объявленной формы вторым payload, подменить
владельца/descriptor/path или добавить незаявленную копию XML. Body разрешён
только самостоятельным Form/CommonForm с readable descriptor и body schema 2.
Отсутствие dialog в native input означает descriptor без `form`; пропущенный
body при существующей обратной связи — ошибка, а не пустая форма.

Модуль формы остаётся отдельным BSL-файлом. Остальные ресурсы/модули сохраняют
прежние bytes и mappings map 3; их внутренние ссылки не переписываются.

## Чтение и экспорт

`SourceReader` открывает только root и map. Отдельное чтение формы разбирает
один объявленный YAML; текущий несохранённый текст проходит те же identity,
composition и codec checks. `form_dialog_paths` в map 4 возвращает пути
form.yaml; прежние карты возвращают сохранённые native/relocated XML paths.
`read_form_dialog_text` генерирует native XML для прежних analysis consumers,
а `parse_form_source`/`parse_form_layout` ожидают полный YAML overlay для map 4.
Формат выбирается ролью map, без content sniffing или fallback на старые bytes.

Acceptance export создаёт оба native файла из этого YAML, проверяет полноту
дерева и повторный inventory. Повторная публикация возможна только при точном
совпадении ранее созданных native bytes. Body no-op сохраняет BOM/CRLF; metadata
writer сохраняет прежний договор нормализации descriptor, не byte-equal XML.

## Граница части 5

Ограничения ниже описывают часть 5. [Часть 6](managed-form-protected.md)
подключает protected writer и source map; migration остаётся незавершённой.

Protected writer для map 4 пока явно возвращает `source_schema_unsupported`.
Прежние migration APIs также отказывают map 4, чтобы не потерять тело формы.
Принятие source в baseline хранит его полные bytes, но не разрешает protected
сборку. Нет разрешения обходить поддержку или применять generic UI CRUD.

Следующее — protected selection по metadata UUID формы, точечные property/layout
edits и source map, source/base/workspace migration, затем независимый XML
контроль и реальный цикл CF через установленную платформу. Analyzer overlay
и Explorer будут подключаться к принятой модели в своих этапах.

## Проверка C1

[Результаты](managed-form-project-results.json) получены на отдельной копии
платформенной выгрузки C1; исходные файлы и demo пользователя не изменены.
Все 823 формы записаны в единые YAML и проверены по строгой combined schema.
Обратный экспорт без правок совпал с прежним readable export по всем 8 769
native файлам и пустым каталогам; XML dialog сохранил исходные BOM/CRLF и bytes.
В проекте осталось 7 947 физических файлов: 3 595 descriptors, 4 351 payload
и manifest; отдельные 823 dialog XML заменены body внутри YAML.

В текущих YAML изменены 14 010 текстов/флагов из 12 317 properties и выполнены
665 перестановок layout. Изменились только 799 native dialog XML; модули,
metadata descriptors и остальные ресурсы сохранили bytes. Снятие правок
восстановило properties/elements/layout, повторные экспорты прошли inventory
проверку. Все 823 saved/overlay модели совпали; exporter читает актуальный YAML.

Добавлены 9 project-тестов: обычный и повторный экспорт, saved/overlay, ошибочные
обратные связи и версии, отсутствие/подмена body, лишняя XML-копия, адресные
лимиты чтения, неподдержанный native профиль, одинаковые короткие имена форм,
регистр native пути и явный отказ незавершённых protected/migration consumers.
Полный обязательный Rust набор прошёл, включая strict Clippy, 630 unit и 570
integration tests; feature matrix, Rustdoc, SDK и adapter также прошли.
Всего выполнены 16 root и 19 consumer gates; 24 прежних opt-in теста остаются
ignored в default наборе. Это проверка project/codec, без платформенной приёмки
изменённых форм, C2/C3 и готовности публичного CLI/анализатора.
