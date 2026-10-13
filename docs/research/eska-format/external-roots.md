# Явные внешние корни — EF-14

Source API `import_designer_queries` принимает ExternalDataProcessor и
ExternalReport в полной readable структуре с формами и отдельными `.sdbl`.
Это map 6: `source.json.external_root` содержит фактический native путь и
`kind = processing | report`. UUID и Name остаются в единственном корневом
дескрипторе. Семантические MetadataKind DataProcessor/Report общие с внутренними
объектами; внешний native тег задаёт отдельный descriptor profile:
`designer-2.20-external-data-processor` / `designer-2.20-external-report`.
Вложенные external теги не принимаются.

В authoring используются `processing.yaml` / `report.yaml`, `ObjectModule.bsl`,
`Forms/.../form.yaml`, `Templates/.../template.yaml` и каталоги `Queries` рядом
с владельцами. Имя исходного XML и каталога native ресурсов может отличаться
от Name и содержать точки/Unicode. Оно сохраняется в обратной карте; корневой
native каталог не становится каталогом редактируемых исходников.
Прямая `Ext/ObjectModule.bsl` и обёрнутая `<stem>/Ext/ObjectModule.bsl`
читаются по проверенной карте. Необъявленные файлы не активируют метаданные.

Reader читает карту и корень, откладывая остальные YAML/BSL/ресурсы до запроса.
Логические родители форм/макетов относятся к внешнему владельцу, native UUID
и координаты YAML сохраняются. DefaultForm поддерживает точные внешние ссылки.
Фактический profile корня должен совпасть с map при открытии и редакторском
overlay. Прежние карты 1–5 и family/readable импорт CF/CFE не меняют схему;
старые layouts явно отказывают внешнему корню до создания результата.

Принятая база внешнего объекта имеет envelope 2 с external root/UUID вместо
configuration_uuid; CF/CFE сохраняют envelope 1. Pinned bytes проверяются полностью.
Freeze наследуется от корня к модулям, формам и запросам. Native source map 5
содержит external binding и обычные origins форм/запросов; их source paths
проверяются относительно внешнего root, а не произвольных полей DTO.
Строгая подготовка отказывает исключённым правкам, нестрогая восстанавливает
принятые bytes и сохраняет working changes.

Проверены оба вида, Unicode/dotted alias, оба расположения ObjectModule,
формы/СКД/динамические списки, повторный экспорт и freeze, неверные roots/maps.
Восьмифайловая настоящая обработка из playground импортирована и прошла
полную protected подготовку без изменения исходного ObjectModule.
[Машинный результат](external-root-results.json) фиксирует границы и проверки.

Project settings и protected build также принимают processing/report с
проверкой фактического native root. Адаптер T84 читает объявленный ObjectModule
без принятой базы; миграция и bump существующего модуля требуют pin через
`--accepted-base-sha256` или `ESKA_ACCEPTED_BASE_SHA256` и разрешения принятого
владельца. Preview уже проверяет права. Публикация повторно проверяет base,
root/manifest/module snapshots; номер остаётся только в BSL, BOM/CRLF сохраняются.
Приняты тесты обоих профилей/схем/языков, freeze, stale previews и сборки с
контрольным runner; они не заменяют настоящий платформенный цикл.

Создание отсутствующего модуля ESKA и независимая платформенная приёмка EPF/ERF
с непустым отчётом остаются следующими частями EF-14. Импорт не генерирует Версия().
Семантическая приёмка анализатора относится к EF-18, общий CLI — EF-20/EF-25.
