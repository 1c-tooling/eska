# Самостоятельные descriptors ESKA

Вторая часть EF-24 добавляет самостоятельные файлы для каждого независимо
сохранённого native descriptor. Читаемая оболочка свойств, составные типы,
редактор формы EF-12 и structural edits ещё не реализованы.

## Расположение и версии

| Native descriptor | Авторитетный YAML |
|---|---|
| `Configuration.xml` | `configuration.yaml` |
| `Catalogs/Products.xml` | `Catalogs/Products/object.yaml` |
| `Catalogs/Products/Forms/List.xml` | `Catalogs/Products/Forms/List/form.yaml` |
| `Catalogs/Products/Commands/Print.xml` | `Catalogs/Products/Commands/Print/command.yaml` |
| `Catalogs/Products/Templates/Label.xml` | `Catalogs/Products/Templates/Label/template.yaml` |
| `Subsystems/Sales/Subsystems/Reports.xml` | `Subsystems/Sales/Subsystems/Reports/object.yaml` |
| `CommonForms/Dialog.xml` | `CommonForms/Dialog/form.yaml` |
| `CommonCommands/Run.xml` | `CommonCommands/Run/command.yaml` |
| `CommonTemplates/Label.xml` | `CommonTemplates/Label/template.yaml` |

Самостоятельность определяется объявленным native descriptor, а не размером
или сканированием каталога. Inline-реквизиты, табличные части, колонки и inline
команды остаются внутри владельца. Они не извлекаются искусственно. Порядок
объявлений остаётся в `Section::Children`, включая ссылки на отдельные файлы.

`source.json.schema_version = 2` обозначает новую обратную карту. XML-профиль,
правила преобразования и `Descriptor.schema_version` остаются прежними: профиль
`designer-2.20-configuration`, rules 1, descriptor 1. Оболочка `ObjectSource`
сохраняет `document` и пустой `nested`; второго descriptor той же формы у
родителя нет. Схемы: [обратная карта 2](source-manifest-v2.schema.json) и
[отдельный источник](object-source-v2.schema.json). Межфайловые ограничения
дополнительно проверяет Rust validator.

Старый `source.json` версии 1 и семейные YAML читаются без изменения байтов.
Открытие reader, analyzer и проекта не запускает миграцию. Новые API общего
ядра — `import_designer_separated` и `creation::import_separated_project`.
Прежние `import_designer`, `creation::import_project` и публичная команда
`eska-project import` пока сохраняют семейное расположение.

## Состав и обратная карта

В YAML хранится единственный текущий состав и значения. `source.json` обязателен:
он сохраняет native файлы/каталоги, UUID, исходные byte fingerprints и привязку
к физическому YAML. Поле `descriptor_contexts` содержит для каждого descriptor
`path`, `metadata_kind`, `name`, `parent`. `parent` — native путь родителя,
разрешаемый к UUID через ту же карту; у конфигурации он равен `null`.
Имя и путь остаются навигацией, не заменяют native UUID.

Эти контексты — происхождение и адреса будущих analysis slots. Они не являются
вторым редактируемым составом. Reader сверяет объявления прочитанного владельца
с обратным контекстом. Полный export/accept дополнительно сверяет весь текущий
состав, родительские связи и UUID. Ручная правка карты не разрешает новые
структурные операции; protected export использует independently pinned base.

Карта версии 2 записывается компактным JSON. Это сохраняет размер служебного
файла для крупных корпусов; предел чтения остаётся 64 MiB. Хеши native input
сохраняют происхождение, не выдают изменённый YAML за исходные XML-байты.
Текущие source hashes и рабочие индексы вычисляются потребителем.

BSL, XML dialog, шаблоны, картинки, HTML/XDTO и supplier payloads в этой части
сохраняют native paths и исходные байты. Удаление универсального `Ext` и явная
карта физических путей ресурсов — следующая часть EF-24. Managed form content
пока остаётся одним XML-ресурсом рядом с отдельным descriptor.

## Reader, редактор и анализатор

Открытие читает только manifest и configuration YAML. Остальные физические
владельцы получают самостоятельные pending slots; модули, ресурсы и accepted
base не читаются. Отсутствующий сосед не блокирует адресное чтение.
Успех отдельного snapshot не означает полной целостности проекта.

`DeclaredOwner.parent` передаёт logical parent для семантической проекции.
Например, собственный `form.yaml` даёт прежний ID `catalog:Products/form:List`,
а nested subsystem сохраняет всю цепочку. Parent UUID в объявлениях остаётся
родным. Координаты и source quota относятся к реально прочитанному файлу.

CST property edits и source map выбирают отдельный YAML через accepted native
UUID. Правка формы не переписывает YAML объекта. Freeze наследуется через
прежнюю native ownership tree, независимо от границ физических файлов.

В SDK отдельная форма получает один canonical slot и собственную dependency
на XML dialog. Она не дублируется как nested slot родителя. Invalid/overlay
этой формы очищает её текущую модель; siblings и parent сохраняют независимые
значения. Nested owners не добавляются в глобальные resolver collections.
Модули продолжают использовать native identities; XML fallback не добавлен.
Покрытие других BSL-моделей и EF-18 этим изменением не расширено.

## План миграции исходников

`prepare_descriptor_layout(source)` сначала проверяет весь источник: состав,
UUID, обратную карту, ресурсы и точный filesystem inventory. План возвращает
sorted `LayoutChange` с before/after bytes и SHA-256, `updated_text`, manifest,
input/output revisions. До применения исходники остаются неизменными.

Семейные descriptors переносятся через CST. Main subtree, кавычки и комментарии
сохраняются; новые поддеревья проверяются полным равенством исходной модели.
Потеря/дублирование комментария или изменение значения отклоняет план с
`source_layout_trivia_unsupported`. Это явный отказ для неподдержанного
расположения trivia, без сериализации с потерями. Singleton YAML переносится
без изменения оформления. Уже отдельный источник даёт пустой план и сохраняет
все байты, включая вручную оформленные YAML.

`plan.publish(new_directory)` повторно проверяет весь вход и пишет только новый
каталог. Точно совпадающий завершённый результат возвращает `unchanged`.
Чужой или изменённый результат не заменяется. Для восстановления после остановки
процесса нужен matching receipt с обеими revisions и точное подмножество
плановых файлов/каталогов. Только собственный staging-файл можно отбросить;
правки уже опубликованных файлов приводят к отказу.

Отдельные файлы устанавливаются через hard link из приватного staging без
замены существующего имени. `source.json` публикуется последним, reader отказывает
пока присутствует receipt. Проверены прерывание процесса между записями и
прерванный staging; гарантия восстановления после потери питания не заявлена.
Файловая система должна поддерживать hard links.

Этот API мигрирует каталог исходников. Он не преобразует project manifest,
CI pin или accepted base и не принимает текущие локальные изменения за новый
эталон поддержки. Полный перенос существующего проекта с отдельно мигрированной
trusted base ещё нужен. Свежий `import_separated_project` создаёт новый проект
из supplied native input и собственную candidate base обычным проверенным путём.

## Проверки

Unit-тесты покрывают отдельные descriptors, inline-границы, deep ancestry,
семантическое равенство с XML в `Summary`/`All`, no-op/repeat, exact-byte preview,
stale input, overlap, recovery, foreign/edited output, координаты source map,
freeze и сохранение YAML в block/flow LF/CRLF. SDK проверяет реальные form-module
bindings и диагностику на независимых YAML/dialog overlays.

Opt-in `real_separated_descriptor_acceptance` готовит C1 из read-only входа
playground: все 3 595 native descriptors и 8 330 объектов совпали в `All`.
Одна permitted form edit изменила только собственный `form.yaml` и один native
descriptor; strict preparation не дала исключений. Все 3 595 YAML и обратная
карта прошли опубликованные JSON schemas без сетевой загрузки ссылок.

Восемь новых unit-тестов и opt-in checks дополняют полный прогон: 394 unit и
537 integration, 17 opt-in пропущены обычным запуском. Все 15 root gates,
включая minimal/project-cli/build-cli, и 14 SDK gates прошли; SDK all-features —
355 тестов. Opt-in миграция старого C1 сохранила 3 595 descriptors, 8 330 объектов
и все байты обратного XML; исходный семейный каталог не изменился. Повторное
применение — `unchanged`, следующий план пуст.

Две независимые версии C1 собраны в CF и повторно выгружены на установленной
1С 8.3.27.2325 через `eska-build`/`eska`. Сравнение 8 769 файлов и 8 064 каталогов
совпало по всем байтам, кроме прежних `configVersion` в `ConfigDumpInfo.xml`.
Платформенный цикл сохранил рабочие исходники, accepted base, исходный XML и
версию конфигурации; контрольные суммы CF совпали с build manifests.
[Обезличенные результаты](descriptor-layout-results.json) отделяют эти проверки
от оставшейся работы. Эта часть не завершает EF-24, CFE/EPF/ERF или EF-12.
