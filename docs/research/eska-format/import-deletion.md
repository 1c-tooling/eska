# Удаление отдельного объекта при импорте

EF-25 расширяет проверяемую карту состава удалением объявленного owner и его
файлов. Общий ancestor, working и incoming по-прежнему сравниваются по UUID/slot.
Правка удаляемого файла даёт `deleted_modified`; исходники не публикуются.
Независимые правки оставшихся объектов сохраняются.

Проверка новой карты допускает только прежние UUID, классы и parentage оставшихся
узлов. Пропавший inline UUID допускается только внутри целиком удалённого
объявленного поддерева. Удаление каждого узла требует unrestricted состояния и
разрешённых прав исходной independently pinned базы. Freeze/нативный запрет
потомка блокирует удаление владельца. Новый UUID и смена владельца остаются отказами.

Ссылки удаляются из native stencil только после проверки прав родителя и порядка
смещённых siblings. Замороженный оставшийся sibling не получает право на смену
порядка от удаления соседа. Старые working, ancestry и support bytes сохраняет
существующий project journal; approved pin не меняется.

Protected prepare повторяет проверку после публикации. Source map schema 6 хранит
только реально выбранные native файлы; удалённый файл не становится восстановленным
из базы. Lookup исходных blobs сохраняется для оставшихся UUID/slots.

Проверка зависимостей учитывает UUID удалённого owner, modeled inline/service UUID,
квалифицированное имя и BSL identifier общего модуля. Неинтерпретируемые resources
остаются консервативным отказом. Ссылки не переписываются автоматически.

Два unit сценария проверяют успешное удаление, сохранённую local version,
strict prepare и no-op repeat; отрицательные случаи — local module edit,
frozen owner, frozen inline descendant и surviving handler. Четыре собственные
копии с отдельным объектом для удаления прошли import/build/dump: итоговый состав
13/107/8/4 файлов. Metadata XML совпал семантически (CDF configVersion игнорируется),
opaque bytes совпали, BSL отличается ожидаемым local comment. Repeat, settings и
прежний pin проверены.

Все 22 Rust gates прошли: root 803 unit/600 integration; SDK/adapter/minimal
build проверки и Rustdoc. Workspace MCP недоступен; четыре изменённых модуля
проверены bsl-analyzer-app CLI: complete, failed files 0, SyntaxError 0. Остальные
style/context diagnostics не объявляются полной семантической приёмкой.
[Обезличенные результаты](import-deletion-results.json). EF-25 остаётся IN_PROGRESS; добавление, native CLI и IDE refresh
ещё не реализованы.

Производственные конфигурации и выполнение в клиенте в этой части не проверены.
