# Текущие общие формы в анализаторе

`eska-analysis`, IDE-сервер и checker получают модель общей формы из текущего
YAML и объявленного preserved `Ext/Form.xml`. Общий reader читает bytes,
сессия управляет их версиями/готовностью, адаптер объединяет declaration и
dialog, canonical slot передаёт результат в контекст BSL-модуля. XML описания
не создаётся; платформа, Git и другой установленный анализатор не запускаются.

## Общий контроллер ресурсов

Права ролей и dialogs используют один `MetadataResourceInput/State/Kind`.
`resource_paths`, `resource_owner` и `metadata_resource` возвращают объявленный
состав и текущий снимок. `load_resource_saved`, `apply_resource_overlay` и
`close_resource_overlay` используют общий clock, проверку версии, отмену,
bounded reader и source-byte квоту вместе с YAML/BSL. Это замена прежних
неопубликованных Rust-типов/методов прав; compatibility aliases не добавлены.

Открытие не читает payload. Только главный declared CommonForm регистрирует
dialog; stray resources, nested и binary формы не становятся входами.
Lazy metadata read готовит одну зависимость выбранного владельца. Пакетная
подготовка читает pending и повторяет cancelled saved inputs; открытый редактор
сохраняет authority даже после отказа по квоте или отмены. Saved invalidation
проверяет весь batch до изменений и не читает bytes. Close читает текущий диск.

Raw UTF-8 text хранится verbatim в `Arc<str>`, включая BOM/CRLF. Его `Ready`
означает доступность bytes, а не корректность native XML. Неверный dialog
сохраняет hash/резерв текста, но очищает модель с `analysis_form_dialog_invalid`
и точным resource path. Ошибка не получает выдуманного диапазона YAML.
Удаление, encoding/size/quota failure и cancellation тоже очищают slot.
Отсутствующий или binary dialog даёт `analysis_form_dialog_undeclared` по owner;
пустая managed форма или guessed ordinary handlers вместо него не создаются.

Name/UUID/FormType берутся из текущего YAML. Элементы, реквизиты/колонки и
event/command handlers — из текущего XML через закреплённый native parser.
Правка одной части перепроецирует её владельца, соседние slots сохраняются.
`ModuleMetadata.form` получает тот же Arc; native `UnusedLocalMethod` учитывает
смену handler без изменения BSL-текста. Неверный YAML также очищает модель.

## Обмен с IDE

ESKA protocol 0.1 добавляет capability `experimental.eska.formDialogSync=true`.
Full document open/change/save/close принимает declared dialog; уведомление
имеет `input.kind=form_dialog`, `owner` и прежние origin/hash/readiness поля.
Save сохраняет текущий editor text/version и clock; close восстанавливает диск.
Role JSON и действующий IDE protocol 1.x не меняются.

Saved watcher добавляет точный basename `Form.xml` из объявленного состава.
Необъявленные файлы остаются вне input boundary. Owner model failure видна
в metadata read и checker через прежний dependency-unavailable контракт.
Pull-диагностика самого XML не добавлена. Максимальный RPC frame остаётся
4 MiB вместе с JSON; saved input может читаться до 64 MiB в пределах общей квоты.

## Проверки

Шесть новых unit-тестов проверяют pending/absent/binary/stray, native parity,
handler-driven BSL semantics, module context, memo соседа, независимость YAML,
stale versions/clock, invalid XML/YAML, disk close, atomic invalidation, encoding,
cancellation и общую квоту двух типов ресурсов с YAML/BSL. Четыре process-теста
IDE и один checker-тест проверяют lifecycle, watcher, RU/EN, human/JSON и
совпадение saved batch/IDE findings и ресурсов. Всего — 290 passed;
все 21 tool/15 root gates прошли, root — 903 passed/14 прежних ignored.
Feature matrix включает минимальную библиотеку, checker, analyzer и все features;
strict Clippy/Rustdoc, release binaries и оба примера прошли.

Все 83/435/460 общих форм C1/C2/C3 совпали с полными native моделями;
706/3114/3348 source/native/control hashes сохранились. Регрессия всех 1 641 ролей
также дала native equality. Шесть stdio-сценариев на owned partial копиях
реальных корпусов проверили current/invalid/refresh/watch/close/delete/recreate,
non-BMP/CRLF и равенство machine output между локалями. Они сохраняют полный
declared состав, но не читают остальные отсутствующие payloads.

Checker C1 снял 83 CommonForm issues: 1 285 → 1 202. Прежние 26 findings в
1 975 модулях, настройки и остальные issues сохранились. Dialog добавил
2 108 532 source bytes к общей квоте. JSON RU/EN и 7 829 hashes совпали.
Exit остаётся 2 из-за неполного покрытия. Результаты сохранены в
[машинном отчёте](analyzer-forms-results.json).

Source schema, версии и graph 206/214 прежние; все 495 upstream hashes проверены.
Это приёмка current input и native model/module consumer. Nested forms, binary
dialogs, полная семантика и общий memory budget остаются в работе;
source-byte квота не ограничивает heap/RSS. EF-18 — `IN_PROGRESS`, EF-12 не начата.
