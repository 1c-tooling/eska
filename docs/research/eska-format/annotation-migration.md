# Перенос аннотаций YAML, шестая часть EF-24

Явные `prepare_readable_layout` и `prepare_accepted_readable_layout` переводят
legacy descriptors с комментариями в [читаемую оболочку](readable-layout.md).
Открытие проекта ничего не мигрирует; layout 3, descriptor schema 2, rules/profile
1 и default CLI не меняются.

## Правила переноса

Аннотация привязана к конкретному CST field/entry. Координаты строятся из
проверенной native модели: UUID/владелец, имя typed-свойства, порядок children
и preserved sections. Совпадающий текст, XML-фрагмент или UUID в комментарии
не используется для поиска владельца.

Совместимые value subtrees копируются через CST: scalar quotes, literal XML,
localized sequence, namespaces/attributes, native fragments и уже readable
descriptors в mixed family. Новые wrappers и отступы формируются заново;
миграция не обещает прежние пустые строки или полный побайтовый стиль legacy
документа. Комментарии внутри сохранённого значения остаются там же. Остальные
комментарии на удалённых `kind/name/state/value` wrappers становятся отдельными
строками перед тем же новым свойством. Комментарии native sections и children
следуют их явным slots; file header/footer остаются на границах документа.
Текст каждого comment token и его количество сохраняются, включая повторы.

Проверяются весь rebuilt `ObjectSource` и все comment tokens. Reindentation не
может изменить строку, literal chomping, XML, localized order, UUID или состав.
При потере/дублировании аннотации либо неподдержанном CST shape возвращается
`source_layout_trivia_unsupported` до публикации. Модель служит общей проверкой
для reader, source map и защищённого экспорта; второго typed writer нет.

Смешанные LF/CRLF в legacy source отказываются до семейного splitter. Это
предотвращает неявную нормализацию ещё до проверки аннотаций. Однородные LF/CRLF
сохраняются. Уже readable input — exact no-op, включая его авторский transport.

Source и pinned accepted plans остаются независимыми. Accepted migration читает
только проверенные blobs, сохраняет policy/freeze и выдаёт новый явный pin.
Пользовательские правки не становятся trusted base. Existing publisher сохраняет
expected-byte preview, original fingerprints, новый owned output, ready marker
последним и repeat/recovery; гарантии fsync не добавляются.

## Приёмка

Прошли fmt/check/strict Clippy/test/Rustdoc, minimal/project-cli/build-cli:
413 unit + 542 integration, 21 opt-in пропущен обычным запуском. Отдельно прошли
14 SDK gates (359 all-features тестов) и пять gates адаптера BSL (70 unit +
10 integration). Последний default набор повторён после окончательной правки.
Один existing update CLI test получил `ETXTBSY`; isolated и полный повтор
прошли, сам тест и update-код не изменялись.

Проверены аннотации каждого fixture field, binding свойств/UUID, repeated tokens,
кавычки, literal XML, localized order, nested/mixed envelopes, LF/CRLF, no-op,
обе независимые миграции и отказ mixed transport с сохранением source/blobs.
Разрешённая правка после миграции сохраняет соседние байты и аннотации;
строгий protected export совпадает с независимой native-правкой.

В реальной C1 аннотирована одна detached legacy форма: четыре комментария и
CRLF. Миграция сохранила 3 595 descriptors/8 330 объектов, 33 320 решений по
правам и все native/payload bytes. Pin получен отдельно из прежнего trusted base.
После разрешённой правки одного Comment обе CF собраны и выгружены через ESKA
на 1С 8.3.27.2325: 8 769 файлов/8 064 каталога совпали, кроме 7 011
`ConfigDumpInfo.configVersion`. Checksums, версия, source/base и исходный XML
проверены; exclusions отсутствуют. [Обезличенные результаты](annotation-migration-results.json).

Клиентское выполнение, полный платформенный BSL-анализ и новые C2/C3,
CFE/EPF/ERF profiles этой частью не принимаются.

EF-24 остаётся `IN_PROGRESS`: составные типы/ссылки/коллекции, structural
create/delete/rename и публичная orchestration ещё нужны.
