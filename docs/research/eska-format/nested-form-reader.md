# Объявленные вложенные формы и dialog

Общий `SourceReader` связывает формы с полным native descriptor path, UUID,
видом и YAML владельца. `form_paths` перечисляет главные CommonForm и вложенные
Form; `declared_form` возвращает `DeclaredForm`. Одинаковое короткое имя в разных
объектах остаётся разной identity. Регистрация использует manifest и layout
объявленных descriptors, не читает YAML владельца или dialog. Корректность
текущего YAML проверяется отдельно при чтении владельца.

`DeclaredForm.dialog` содержит точный preserved XML path либо `None`.
`form_dialog_paths` теперь перечисляет XML dialogs общих и вложенных форм;
`form_dialog_owner` возвращает enclosing YAML. Новый `form_for_dialog` даёт
полную identity формы. `form_dialog_path(owner)` по-прежнему адресует главный
CommonForm: обычный владелец может иметь несколько вложенных форм.
`DeclaredModule.native_descriptor` совпадает с identity формы для её body.

Stray files и resources без form descriptor не становятся входами. Binary
dialog не заменяется пустой формой. Объявленный, но удалённый XML сохраняет путь
и даёт `source_file_missing`. Bounded current read использует общие проверки
UTF-8, size, filesystem aliases и сохраняет BOM/CRLF. ASCII spelling variants
ресурса не меняют descriptor или YAML owner. Source schema прежняя.

## Независимая сверка модели

Read-only пример `project_nested_forms` группирует формы по владельцу, читает
его текущий YAML ровно один раз и передаёт отдельные descriptors и dialogs
существующему адаптеру. Одновременно удерживается один owner snapshot.
Native файловый reader — независимый reference; source XML не генерируется.

```bash
cargo run --manifest-path tools/bsl-adapter/Cargo.toml --release --locked --example project_nested_forms -- /path/to/eska/source /path/to/native/source
```

В C1 проверены все 740 вложенных форм, в C2 — 7 781 из 7 782, в C3 — все 9 665.
Все 18 186 моделей с XML dialog совпали целиком: identity/type, elements,
attributes/columns и event/command handlers. Одна форма C2 имеет binary dialog;
она отмечена как отсутствующий XML input и не сравнивается с inferred моделью.
Прочитаны 294/2 328/2 940 owners. Сохранились 4 638/49 896/59 651
source/native/control hashes. Лишний XML C1 не активировал форму.
Wall time сценария с hashing и native reference — примерно 5/119/81 секунд;
это не время resident-запроса или замер улучшения производительности.

## Граница сессии анализа

[Producer общих форм](analyzer-forms.md) явно выбирает только resource главного
CommonForm из расширенного reader. Вложенные dialogs ещё не регистрируются как
editor inputs и не публикуются в canonical slots; их BSL module context остаётся
`form=None` без disk fallback. Полнота вложенных моделей остаётся ложной.

Следующий компонент должен публиковать отдельный slot по full descriptor
identity, учитывать YAML владельца и именно этой формы resource, передавать
его в соответствующий BSL body и сохранять адресную инвалидацию соседа.
Reader и сверка адаптера сами по себе не означают готовность этого lifecycle.

## Проверки

Три новых root tests проверяют одинаковые имена/разные UUID, full identities,
module backlinks, main/nested/absent/binary/stray inputs, verbatim текущий текст,
size/encoding/deletion, unread owner и spelling variants. Отдельный SDK-тест
проверяет, что reader не активирует nested editor input через main producer,
не меняет clock/quota при отклонении и не включает native disk fallback.

Все 6 adapter/21 tool/15 root gates прошли: strict Clippy/Rustdoc, feature matrix
и release examples/binaries. Root — 906 passed/14 прежних ignored,
адаптер — 69 unit/8 file, tools — 291 passed. Шесть real stdio-сценариев C1/C2/C3
RU/EN сохранили common-form lifecycle и равенство machine output между локалями.
Checker C1 сохранил 26 findings в 1 975 модулях, 1 202 metadata issues, квоту и
exit 2. JSON RU/EN и 7 829 hashes совпали. Протокол, версии, graph 206/214 и
495 upstream hashes прежние. Результаты — в [машинном отчёте](nested-form-reader-results.json).

Платформа не запускалась: компонент читает source identity и модели.
Nested slots/resources, binary dialog, общий memory budget и прочие условия
EF-18 ещё в работе. EF-18 — `IN_PROGRESS`, EF-12 не начата.
