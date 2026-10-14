# Добавление объявленного объекта при импорте

EF-25 расширяет проверяемую карту состава полными новыми владельцами и их
поддеревьями. Native вход предоставляет descriptor, UUID, служебные identities,
модули и ресурсы; импорт не придумывает свойства и defaults. Three-way выбор
сохраняет независимые локальные values и файлы прежних владельцев. Пересекающиеся
правки новых файлов остаются конфликтами.

Новый UUID получает права только после проверки исходной принятой базы: ближайший
принятый родитель должен разрешать Resource изменение, а прежняя native policy
должна определять новый UUID как собственный unrestricted объект. Freeze родителя
наследуется. Вставка перед прежним sibling дополнительно проверяет его Reorder;
добавление после замороженного sibling не меняет его порядок. Новые inline UUID
допускаются только внутри полного нового объявленного поддерева.

Сохранившиеся UUID сохраняют классы и parentage. Новый owner не может занять
прежний native descriptor path с другим UUID или повторно использовать прежнюю
моделируемую object/service identity. Необъявленные и authority paths не получают право на
добавление от соседнего нового объекта. Смена правил поставщика требует отдельного
update-base. Повторная защищённая подготовка проверяет те же условия; independently
pinned store, его bytes и settings не меняются. Прежние объекты, включая удалённые,
остаются в исходной policy, чтобы их identities нельзя было использовать заново.

Source map schema 7 добавляет `created_metadata` и `created_managed_form` с
единственным working `input`. Новому файлу не назначается фиктивный accepted
origin. Модули, plain queries и opaque payloads имеют Working authority;
координаты metadata definitions и form properties также указывают рабочий YAML.
Для `.sdbl` координата вычисляется без загрузки owner текста. Для нового YAML
caller передаёт точные working bytes в `OriginTexts.working`; accepted text
не используется. Maps 1–6 и сериализация прежних origins сохранены, корень проекта
по-прежнему требует настоящий accepted input. Prepared artifact envelope прежний.

Четыре новых unit сценария проверяют локальный модуль/версию, working origins,
strict prepare и repeat; freeze родителя и сдвигаемого sibling; новую форму с
plain queries и двоичный XML-named template; новый nested descriptor в общем
legacy family YAML. Прежняя проверка смены UUID на том же пути сохраняется.

Все 22 Rust gates прошли: root 807 unit/600 integration, all-features также
21 build/50 project process tests; SDK 185/158/19, adapter 70 + 12,
минимальный build/core/CLI и Rustdoc. 24 opt-in root tests пропущены.
Четыре собственные native пары прошли import/build/dump на 1С 8.3.27.2325:
17/111/14/8 файлов. Состав и XML semantics совпали с independently unpacked
incoming, кроме служебных CDF configVersion; opaque bytes равны, BSL отличается
только ожидаемым local comment и отделяющими переводами строк. Settings,
прежний pin и no-op repeat проверены. CF/CFE добавляют common module, EPF —
форму с модулем, ERF — отдельный СКД template с plain queries.
[Обезличенные результаты](import-addition-results.json).

Workspace MCP недоступен. Установленный bsl-analyzer-app 0.3.2 проверил семь
новых/изменённых модулей: complete, failed files 0, SyntaxError 0. Остальные
style/context diagnostics: 0/5/51/2. В собственных проектах опыта нет
bsl-analyzer.toml, использованы defaults. Это выбранная syntax check,
не полная семантика конфигураций или выполнение в клиенте.

Подмена UUID, перенос принятого объекта к другому владельцу, непринятые inline
изменения вместе с новой картой и смена поддержки остаются явными отказами.
Native CLI lifecycle и обновление IDE после импорта ещё входят в незавершённую
EF-25. Эта часть не объявляет MVP готовым.
