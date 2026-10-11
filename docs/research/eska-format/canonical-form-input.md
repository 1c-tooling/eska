# Вход формы в ядро анализа

Локальный `ide-db` принимает `CanonicalMetadata::Form(Arc<Form>)` в существующем
`CanonicalMetadataInput`. `resolve_form` читает значение и source revision этого
slot, проверяет текущее имя без учёта регистра и возвращает общий Arc. Pending,
wrong kind или другое имя дают `None`; XML и прежняя модель не используются.

## Контекст BSL-модуля

`module_metadata_query` проверяет canonical режим до native form loader.
Владелец модуля определяется по его собственному зарегистрированному корню;
видимый чужой корень не может присвоить себе отдельный файл. Native путь
описания даёт стабильный FileId в SourceRoot, хотя ESKA не создаёт этот XML.
По FileId находится тот же mutable canonical slot, а `ModuleMetadata.form`
получает его текущую модель.

В canonical корне непредоставленная форма, включая ещё не подключённую nested
форму, остаётся `FormModule` с `form=None`. Данные preserved `Ext/Form.xml`
самостоятельно с диска больше не загружаются. Это закрывает неверсированный
обход input layer. В обычном Designer-корне без canonical slots прежний native
loader продолжает работать. Отдельный глобальный индекс имён форм не добавлен:
этот компонент обслуживает существующий потребитель контекста модуля.

Общая сессия умеет разрешать новый вариант slot, но producer dialog input ещё
не подключён. Поэтому CommonForm по-прежнему имеет явную неполноту проекции;
готовность YAML не означает наличие текущих данных формы в BSL-ядре.
[Адаптер](form-adapter.md) и [reader ресурсов](form-dialog-reader.md) готовы;
согласованный resource lifecycle и последующая публикация slot ещё в работе.

## Проверки

Три новых native-теста проверяют Unicode case, замену payload, source revision,
сохранение resolver/module memo соседа, pending и неправильную identity/kind,
подключение к BSL module context и недоступную nested форму. Файловый тест в
собственном playground создаёт настоящие native descriptor/dialog: current
slot перекрывает данные на диске, pending не получает disk fallback, а обычный
Designer-режим читает прежний dialog. Чужой canonical корень не перехватывает
standalone форму; исходные bytes сохраняются.

Все 5 native/20 tool gates прошли: strict Clippy/Rustdoc, feature matrix,
release analyzer/checker и пример роли. Kernel — 2630 passed/20 прежних ignored,
общая сессия — прежние 279 tests. Основная библиотека после reader не менялась;
её проверенные 903/14 и 15 gates сохранены отдельной ссылкой в отчёте.
Все 495 upstream hashes и runtime graph 206/214 прежние, версии не менялись.

Checker C1 сохранил 26 findings в 1 975 модулях, 1 285 metadata issues, прежнюю
квоту и exit 2 из-за неполного покрытия. JSON RU/EN совпал, все 7 829 hashes
исходников сохранились. Native inputs, gates и сохранность upstream provenance
записаны в [машинном отчёте](canonical-form-input-results.json). Это проверка
input/resolver и существующего module consumer, не платформенная приёмка UI.
Source/protocol schemas, версии и зависимости сохраняются; EF-18 остаётся
`IN_PROGRESS`, EF-12 не начата.
