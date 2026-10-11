# Объявленные ресурсы прав

Общий `SourceReader` регистрирует права роли при открытии manifest и composition,
не читая owner YAML или ресурс. Только `preserved` запись, привязанная к
объявленному descriptor роли, получает эту identity. Произвольный файл,
одноимённый ресурс другого вида и undeclared роль не становятся входами анализа.
ASCII case variants native пути учитываются; возвращается точный путь manifest.

`role_rights_paths` перечисляет эти входы, `role_rights_path` находит ресурс по
owner, `role_rights_owner` возвращает обратную связь. `None` означает отсутствие
объявленного ресурса. Если запись есть, удаление или порча файла не меняет эту
identity и не означает пустые права.

`read_role_rights_text` читает текущие UTF-8 bytes с прежними проверками путей,
symlinks и лимита документа. BOM/CRLF сохраняются. Код
`source_role_rights_undeclared` отличает неизвестный ресурс от
`source_file_missing`, `source_encoding_unsupported`, `source_entry_unsupported`
и `source_document_too_large`. XML syntax и permission semantics этот слой не
проверяет. Manifest/configuration changes по-прежнему требуют reopening.

[Пример адаптера роли](role-adapter.md) теперь получает identity и bounded bytes
через общий reader. Собственный поиск файла в manifest и прямое чтение рабочего
ресурса из примера удалены; native XML нужен только для независимого сравнения.

Четыре новых unit-теста проверяют регистрационный контракт, missing owner,
undeclared ресурсы, absent ресурс, текущие BOM/CRLF/invalid XML bytes, byte limit,
encoding, deletion, case variants и отказ symlink. Все 899 root tests прошли
(14 прежних ignored); minimal core — 169/3, project-cli — 186/3, build-cli — 205/3.
Адаптер — 64 unit/7 file, SDK — прежние 270 tests. Все 6 adapter/19 SDK/15 root
gates прошли, включая strict Clippy/Rustdoc и feature matrix; native workspace
в этой части не изменён.

Все 1 641 модели C1/C2/C3 повторно сравниваются с native reader через новый путь;
замеры и проверки сохранности исходников приведены в
[машинном отчёте](role-rights-reader-results.json).
Версии, dependencies, формат manifest, XML export и protocol не менялись.
Это структурный ввод для EF-18. Current rights lifecycle, canonical role resolver,
остальные модели, общий memory budget, edits и Explorer остаются в работе.
