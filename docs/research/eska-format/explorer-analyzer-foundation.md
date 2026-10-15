# Explorer через общий анализатор: первая установленная приёмка

Backend `analyzer` подключён рядом с прежним `designer-xml`. Один
`eska-analyzer` обслуживает дерево, текущие свойства YAML и языковые запросы
выбранного ESKA-проекта через [общий navigation/policy контракт](analyzer-navigation.md),
прежние owner `read`/`locate` и независимый accepted-base pin для permissions.
Без pin разрешение неизвестно. В этом срезе открытие из дерева всегда защищённое,
свойства доступны только для чтения.

Document journal сохраняет полный текущий текст YAML/BSL до подтверждения
backend. Restart завершает прежний child и объявляет readiness после replay.
Import guard отзывает результаты прежнего сеанса; после завершения импорта
открывается новый сеанс с сохранёнными черновиками. Поздние ответы проверяются
по сеансу, версии документа и текущему источнику. Координаты dirty editor
не применяются к защищённой вкладке сохранённого файла.

В VSIX включены отдельные grammars/language ids BSL, YAML и SDBL. Язык назначается
объявленным файлам до загрузки модели; глобальные ассоциации чужих `.bsl`
не меняются. Клиент регистрирует согласованные symbols, hover, definition,
completion, references и signature readers. Commands, дополнительные edits
и планы изменений из completion не исполняются.

## Проверки

Установленный VSIX `0.0.11` прошёл actual extension-host acceptance в отдельном
профиле VSCodium на собственных CF/CFE/EPF/ERF. В runtime PATH находились только
`eska-analyzer` и необходимые launcher helpers; project tool использовался
до запуска для подготовки проектов. Пользовательский профиль не изменялся.
Бинарники были скопированы перед запуском и сохранялись между restart cases.

Для всех четырёх типов прошли дерево, текущие свойства, protected opening/write
refusal, один symbols provider, диагностика unsaved BSL, dirty replay после
restart, import retirement/recovery и сохранность source bytes. Для CF/EPF/ERF
прошёл own-context completion. Для EPF/ERF проверены ranged adapter findings
в current YAML, `metadata_input_unavailable` для malformed YAML без старых
findings и восстановление после repair. Все 15 запущенных analyzer children
завершились. Это автоматические проверки установленного расширения через
настоящие editor commands/providers; ручная проверка всего UI не заявляется.

`npm run check`, Node suite 207/207 без skipped, последующие manifest/diff
checks прошли. VSIX содержит 1306 entries и прошёл `check-vsix.py`.
TextMate 9.3.2/Oniguruma 1.7.0 из установленного редактора загрузили три
grammars и прошли 12 lexical cases. SDK checkpoint прошёл 417 тестов
закоммиченного поведения и семь независимых query-helper tests; полный
основной crate, строгие SDK Clippy/Rustdoc прошли отдельно.
[Обезличенный отчёт](explorer-analyzer-foundation-results.json) разделяет
подтверждённые сценарии и ограничения.

## Граница первой приёмки

Следующая [приёмка текущей навигации](explorer-current-navigation.md) подключила
search/reveal, current labels и lazy support display. Ограничения ниже относятся
к checkpoint первой приёмки.

EF-11 остаётся `IN_PROGRESS`. Search/reveal, прежний порядок групп, текущие
имена физических owners, полный support display и editable opening по
проверенным permissions ещё нужны. `.sdbl` document sync/analysis, CF/CFE
composition, auto-discovery и local bundle EF-21 принимаются отдельно.
CFE semantics без owning base, property editing, общий RSS limit,
cold/warm benchmark, theme/member matrix и весь цикл MVP этой проверкой
не подтверждены. EF-16 не закрыта.
