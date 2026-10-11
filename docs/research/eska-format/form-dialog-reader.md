# Объявленные данные общих форм

Результаты первоначального reader общих форм. Последующее
[расширение на nested identities](nested-form-reader.md) добавило full descriptor
lookup и чтение вложенных dialogs; его API и проверки описаны отдельно.

`SourceReader` связывает preserved `Ext/Form.xml` только с объявленным главным
descriptor вида `CommonForm`. Регистрация использует validated manifest и
configuration composition, не читает YAML формы или dialog. Лишний native
файл, вложенная форма либо ресурс другого вида не становятся такими входами.
ASCII case variants сохраняют точный путь из manifest.

`form_dialog_paths` перечисляет входы, `form_dialog_path` находит dialog по
owner YAML, `form_dialog_owner` возвращает владельца ресурса. `None` означает
отсутствие объявленного XML dialog. Зарегистрированный, но удалённый файл
сохраняет identity и при чтении вызывает `source_file_missing`.

`read_form_dialog_text` читает текущие UTF-8 bytes с общими проверками пути,
каждого компонента файловой системы и лимита, переданного caller. BOM/CRLF
сохраняются; изменение файла видно без reopening. Неизвестный ресурс даёт
`source_form_dialog_undeclared`; encoding, missing, symlink/directory и size
failures имеют прежние коды. Malformed XML остаётся текущим текстом: syntax и
семантику формы проверяет consumer. Изменение manifest/configuration требует
reopening, как и для остальных входов reader.

Пример [`project_forms`](form-adapter.md) использует этот reader вместо
собственного manifest scan и прямого чтения рабочего ресурса. Native файловый
loader остаётся независимым сравнением. XML не экспортируется, платформа и Git
для проекции не нужны.

Четыре новых unit-теста проверили прямые/обратные связи, absent/undeclared
inputs, открытие с invalid owner YAML и missing dialog, verbatim BOM/CRLF,
текущие изменения, invalid XML, size, encoding, deletion, case variants и
symlink rejection. Все 903 root tests прошли, 14 прежних ignored; minimal —
173/3, project-cli — 190/3, build-cli — 209/3. Адаптер — 69 unit/8 file,
общая сессия — прежние 279 tests. Все 6 adapter/5 consumer/15 root gates прошли,
включая strict Clippy/Rustdoc и root feature matrix.

Все 83/435/460 объявленных общих форм C1/C2/C3 повторно совпали с native reader
по полной модели. Hashes 706/3114/3348 source/native/control files сохранились.
В C1 84 preserved ресурса, но активируются только 83 объявленные формы.
Результаты — в [машинном отчёте](form-dialog-reader-results.json).

Этот первоначальный компонент ограничен главными общими формами и XML dialog.
Их [current lifecycle и canonical input](analyzer-forms.md) затем подключены
к сессии. Nested reader описан отдельно выше; nested lifecycle, binary dialog
и общий memory budget ещё в работе. Source/protocol schemas, версии,
зависимости и native workspace не менялись. EF-18 остаётся `IN_PROGRESS`.
