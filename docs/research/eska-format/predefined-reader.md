# Объявленные предопределённые данные

`SourceReader` связывает preserved `Ext/Predefined.xml` с объявленным главным
дескриптором справочника, плана счетов, видов расчёта или характеристик.
`predefined_paths` перечисляет эти ресурсы, `predefined_path(owner)` и
`predefined_owner(resource)` дают прямую и обратную связь с YAML владельца.
Регистрация использует проверенный состав, не читает payload владельца или
сам ресурс. Значения перечислений остаются частью их собственного descriptor.

Файл без объявленного владельца или у другого вида метаданных не становится
входом. Отсутствующая декларация отличается от удалённого объявленного файла:
путь последнего сохраняется и bounded read возвращает `source_file_missing`.
ASCII spelling variants сохраняют точное написание manifest. Общие проверки
пути, filesystem aliases, UTF-8 и byte limit действуют и здесь.

`read_predefined_text(path, max_bytes)` возвращает точные текущие байты текста,
включая BOM/CRLF. Reader не разбирает XML и не выдумывает элементы. Некорректный
XML остаётся доступным текстом, ошибки кодировки и размера возвращаются отдельно.
Source schema, публичные CLI, протокол и зависимости не изменились.

## Проверка на конфигурациях

Пример читает текущие ресурсы через общий reader и независимо сравнивает их
байты с native файлами; YAML владельцев для этого не загружается.

```bash
cargo run --no-default-features --release --locked --example project_predefined_reader -- /path/to/eska/source /path/to/native/source
```

Все 24/114/167 ресурсов C1/C2/C3 совпали с native файлами: всего 305,
85 982/2 347 143/2 987 399 bytes. Сохранились все 99/459/671
source/native/control hashes. Проверка не генерировала XML и не запускала
платформу; процесс работал с пустым `PATH`.

Четыре новых root tests проверяют supported/absent/stray owners, unread YAML,
verbatim текст, размер, кодировку, удаление, spelling variants и symlink.
Отдельный SDK guard проверяет первоначальную границу: структурная регистрация
reader ещё не активирует editor input, не меняет clock/quota при его отклонении
и не включает disk fallback.

22 tool и 15 root gates прошли, включая strict Clippy, Rustdoc, минимальные
features и release binaries/examples. Root — 910 passed/14 прежних ignored;
SDK — 299 passed. Checker C1 на обоих языках сохранил полный прежний JSON:
1 975 модулей, 26 findings, 960 metadata issues, exit 2 и source резерв
204 302 277 bytes. Все 7 829 source hashes сохранились. Runtime graph — 206/214;
495 original upstream hashes проверены. Результаты — в
[машинном отчёте](predefined-reader-results.json).

## Следующий компонент

Current resource producer и преобразование предопределённых элементов в модель
объекта пока не подключены. Для них нужен явный отказ при неверном XML:
сломанный ресурс не должен превращаться в корректный пустой список.
Saved/overlay/close, watch/invalidation, общая квота и адресная инвалидация
семантического потребителя будут проверены при подключении producer.
Другие модели, общий memory budget и остальные условия EF-18 ещё в работе;
EF-18 — `IN_PROGRESS`, EF-12 и следующие этапы не начаты.
