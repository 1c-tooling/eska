# Текущие вложенные формы в сессии анализа

`eska-analysis`, IDE-сервер и checker используют отдельный canonical slot каждой
объявленной вложенной формы. Полный native descriptor path связывает slot,
descriptor из YAML владельца и именно этой формы dialog. Одинаковые короткие
имена у разных объектов не пересекаются. `ModuleMetadata.form` получает текущую
модель через существующий resolver ядра; XML описания не создаётся.

## Независимые зависимости

При открытии регистрируются pending slots и declared XML resources без чтения
payload. Вложенные формы используют тот же resource controller, saved/overlay/
close, версии документов, cancellation, bounded reader и source-byte квоту,
что [общие формы и права ролей](analyzer-forms.md).

Пара ревизий YAML и собственного dialog определяет текущую проекцию формы.
Правка dialog не пересоздаёт главный объект, sibling forms или формы других
владельцев. Их Arc и memo контекста BSL сохраняются. Изменение YAML владельца
перепроверяет все его формы. Недоступный или некорректный текущий YAML очищает
зависимые slots; данные с диска и старая модель не становятся fallback.

Неверный XML остаётся готовым raw input с hash/byte reservation, но очищает
только slot своей формы с `analysis_form_dialog_invalid` и resource location.
Ошибки чтения, encoding, квоты и cancellation также очищают этот slot.
Главный объект и исправные siblings могут оставаться готовыми. Saved events
инвалидируют только объявленные inputs и сохраняют authority открытых overlays.
Close читает текущий файл с диска. Состав сессии остаётся неизменным до reopen.

Name/UUID/FormType берутся из текущего вложенного YAML descriptor; элементы,
attributes/columns и event/command handlers — из текущего dialog через native
parser. Правка handler меняет `UnusedLocalMethod` при неизменном BSL-тексте.
Отсутствующий/binary dialog даёт `analysis_form_dialog_undeclared` по YAML
владельца. Его данные или ordinary handlers не угадываются.

## Tree, IDE и checker

Экспериментальный Rust API добавляет `OwnerModel.forms: Vec<FormModel>`.
Каждая запись содержит `native_path`, `dialog`, ревизию своих зависимостей и
`ModelState`. `modeled_descriptors` учитывает готовый главный descriptor и
готовые вложенные формы. Другие nested descriptors сохраняют явную неполноту.
Полнота главных моделей, всех descriptors и semantic profile различается.

Protocol 0.1 добавляет `experimental.eska.nestedFormModels=true`. Ответ состояния
владельца с вложенными формами содержит `forms`; у других владельцев поле
отсутствует. Элемент имеет `native_path`, `dialog`, строковую `revision`,
`model` и `model_issue`. Это ревизия последней проекции формы; текущий общий
snapshot по-прежнему проверяется отдельно. Resource notification сохраняет
`input.kind=form_dialog`, enclosing `owner` и raw origin/hash/readiness.
Lazy metadata read готовит declared dialogs только выбранного владельца.
Semantic preparation использует все pending declared inputs общей сессии.

JSON checker schema 1 добавляет `forms` только к metadata issue с отказавшими
вложенными формами. Каждый отказ содержит full `native_path`, code, location
и optional range. Resource failure не получает диапазон YAML. Adapter failure
сохраняет captured range текущего YAML и появляется в его pull diagnostics.
Human output показывает отдельные coded отказы на RU/EN. Готовый главный объект
с отказавшей формой остаётся неполным по descriptors; exit codes прежние.
XML pull diagnostics не добавлены. Source schemas и Role JSON сохраняются.

## Проверки

Четыре новых unit-теста проверяют полные identity, native equality, независимые
slots, Arc/memo главного объекта и siblings, handler-driven BSL diagnostics,
owner YAML failures и captured adapter ranges, saved invalidation, encoding,
cancellation, stale versions/clock, overlay authority и общую квоту.
Два IDE process-теста проверяют lifecycle/watcher и saved IDE/checker parity;
один checker-тест — отдельные nested failures в human/JSON RU/EN.
Всего tools — 298 passed, root — 906 passed/14 прежних ignored.
Все 22 tool/15 root gates прошли: feature matrix, strict Clippy/Rustdoc,
release binaries и три модельных примера.

Read-only пример общей сессии сверяет полные nested модели с native reader.
Он читает YAML каждого владельца один раз, затем освобождает его source inputs;
это не полный resident semantic batch большой конфигурации и не бюджет RSS.

```bash
cargo run --manifest-path tools/analysis-core/Cargo.toml --release --locked --example project_nested_forms -- /path/to/eska/source /path/to/native/source
```

Все 740/7 781/9 665 nested XML-моделей C1/C2/C3 совпали целиком с native reader:
identity/type, элементы, attributes/columns и event/command handlers.
Всего — 18 186 XML-моделей; одна binary форма C2 явно недоступна.
Сохранились 4 638/49 896/59 651 source/native/control hashes.
Текущий резерв одного владельца достигал 0,88/8,95/8,88 МБ и освобождался до
нуля между владельцами; полный heap процесса этим не измерялся.
Регрессия всех 978 общих форм и 1 641 ролей также дала full native equality.

Шесть stdio-сценариев на owned partial копиях C1/C2/C3 проверили independent
leaf/main/sibling state, verbatim non-BMP/CRLF, invalid/refresh/watch/close,
delete/recreate и равенство machine output RU/EN. Полный declared состав
сохранён, несвязанные payloads намеренно отсутствуют. Это протокольные сценарии;
настоящий OS/client watcher ими не подтверждается.

Checker C1 с профилем `semantic` и единственным правилом `SelfAssign` прочитал
все 740 nested XML dialogs и снял 242 непокрытых owner issues:
1 202 → 960. Остальные issues, 26 findings в 1 975 модулях, настройки и JSON
RU/EN сохранились. Дополнительно удерживаются 16 248 034 source bytes;
общий резерв — 204 302 277 из 268 435 456 bytes. Все 7 829 source hashes
сохранились. Exit 2 остаётся из-за неполноты других descriptors.

Версии/dependencies и graph 206/214 прежние; 495 upstream hashes проверены.
Платформа 1С не запускалась: этот компонент работает с текущими source inputs
и моделями. Binary dialogs, остальные модели, общий memory budget и остальные
условия EF-18 ещё в работе. EF-18 — `IN_PROGRESS`, EF-12 не начата.

Результаты — в [машинном отчёте](analyzer-nested-forms-results.json).
