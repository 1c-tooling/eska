# Текущие хранилища настроек

`SettingsStorage` получает отдельную canonical модель, восемнадцатое listing
family и current slot. Сохраняются настоящие UUID, имя и ownership; заимствование
требует совпадения имени и UUID оригинала на каждом шаге видимой CF/CFE цепочки.
Модель не превращает свойства оформления хранилища в runtime поля и не задаёт
схему сохранённого значения. Адаптер использует прежний descriptor parser.

## Native BSL и текущие тела

Настоящий `SettingsStorages` / `ХранилищаНастроек` предлагает только проверенные
текущие имена. Настроенный `SettingsStorageManager` сохраняет восемь методов
закреплённого каталога платформы, их IDs, параметры, арность и execution contexts.
`Load` возвращает native arbitrary type (`Any`); ключ не создаёт типизированную
схему значения. Native iteration контейнера сохраняет generic manager type,
не приписывая ему exports конкретного хранилища.

Модуль менеджера связан с владельцем через объявленный FileId. Читаются только
зарегистрированные `ManagerModule.bsl`/`ManagerModule.bin` в видимой цепочке.
Binary/unread вклад сохраняет барьер неполноты; одноимённый файл вне manifest
не участвует. Необязательное отсутствие модуля отличается от недоступного тела.

Completion объединяет native методы и текущие экспортные методы менеджера.
Hover/signature help используют общий call binding. После удаления известного
экспорта вызов получает `UnresolvedMethodCall`; native `Load` остаётся доступным.
Холодный запрос готовит выбранный manager body через общий input controller,
включая reaching aliases и возвращаемое пользовательским методом значение.
Соседние процедуры, аргументы и скрытые расширения не разрешают лишнее чтение.
Циклические aliases обрываются при повторном посещении узла с учётом его глубины.

Unsaved bodies, watcher, invalid metadata, repair, close и foreign restart/replay
используют прежние current inputs. Ошибка текущего descriptor или неверный
original UUID дают `analysis_composition_incomplete`, сохраняя отдельный syntax
route. Старый slot или XML sidecar не заменяют недоступный текущий ввод.

Экспериментальные Rust APIs расширены: `CanonicalMetadata`, constructors/inputs
listing, `ConfigsDatabase`, `ObjectResolver`, `ModuleMetadata.settings_storage`.
Все текущие потребители обновлены. CLI/config/source schemas, wire negotiation,
версии, dependencies и pin девяти native packages не менялись.

## Проверка самого модуля менеджера в 1С

На 1С 8.3.27.2325 проверены два маленьких native модуля менеджера:
`SettingsStorage` и `Catalog`. Для каждого выполнен один набор
`/CheckModules -Server -ExternalConnection -ThickClientOrdinaryApplication`:

| Исходник | Exit | Результат |
|---|---:|---|
| Обычный корректный код | 0 | Проверка пройдена |
| Заведомо отсутствующая переменная | 101 | Три undefined-variable findings |
| Обращение к `ThisObject` | 101 | Три undefined-variable findings |
| Исправленный код | 0 | Проверка пройдена |

Все восемь сохранённых CF повторно выгружены через `eska-build unpack`:
проверяемый BSL совпал побайтно. Основная и все временные unpack базы удалены,
собственных процессов платформы не осталось. `ibcmd config check` пропустил
контрольную отсутствующую переменную и поэтому не считается проверкой BSL.
Runtime `Load` не вызывался.

Положительный SDK/Explorer сценарий использует квалифицированное
`SettingsStorages.CurrentЯ.Load(...)`. Предопределённый `ThisObject` в этих двух
проверенных видах ManagerModule отсутствует; пользовательские bindings/globals
по-прежнему владеют своими именами. Реальные self свойства object/record-set/form
модулей сохраняются. Непрочитанная поверхность globals не становится ложным
`UnresolvedName`.

## Проверка ассоциации CF/CFE

Read-only аудит ранее использованной большой native пары проверил все 38
заимствованных non-root declarations: только одна связь имеет совпадающие
original UUID, kind, имя и parent. Для остальных 37 UUID оригинала отсутствует
в явно выбранной базе. Совпадение имени у 28 из них не доказывает заимствование.
Корневой adopted marker без original UUID учитывается отдельно.

Три другие ранее сохранённые пары также не доказали ни одной из 38 связей.
Прежние успешные сборки/выгрузки отдельных артефактов сохраняют своё значение,
но не подтверждают их совместимость как base/extension. Source bytes, mode,
mtime и inode всех 3681 проверенных файлов сохранены. Новых копий и баз аудит
не создавал. Повторный semantic success probe несовместимой пары не запускался.

## Проверки SDK, Explorer и native источников

Все 27 Rust gates выполнены на одном неизменном наборе 1682 Rust/Cargo/locale
inputs: шесть корневых, пять kernel, пять adapter и одиннадцать SDK. Прошли fmt,
check, strict Clippy, default/all-features tests, rustdoc с `-D warnings` и
debug/release binaries. Корень — 1511/1632 tests, по 24 ignored; kernel —
4432 passed / 25 ignored; adapter — 123; SDK — 255/573 без ignored.

Проверены native namespace RU/EN, метод `Load` с четырьмя параметрами, exports,
iteration, cold alias/user-return/parenthesized calls, циклические aliases,
current removal, invalid/repair/watch/close, binary barrier, hidden siblings и
неправильный original UUID. SDK и process checker дают одинаковый JSON RU/EN:
правильный вызов, удалённый export и недоступная композиция сохраняют разные
результаты. Отрицательные self тесты явно включают `UnresolvedName`, сохраняя
штатную проверку отсутствующего метода и корректный qualified control.

Explorer — 376/376 Node tests, без skipped. Установленный VSIX подтвердил native
manager completion, method hover/signatures, current exports и их отзыв,
invalid/repair/close, ошибочный CFE UUID и restart/import replay. Все 33 дочерних
процесса завершились; инвентари 96/85/133 файлов и standalone native CFE сохранены.
Все 55 production JS modules совпали с новой компиляцией; прежний VSIX 0.0.11
использован без изменений и test helpers. Native CF сохранила consumer-матрицу;
standalone native CFE явно отказала в semantic context, сохраняя syntax route.
Физический ввод и Native Save не проверялись.

Первый Node/root запуск в sandbox встретил ограничения дочерних процессов,
localhost и Git discovery. Контроль и полные финальные наборы вне sandbox прошли
на тех же исходниках; application code ради этих ограничений не менялся.
Предыдущие неуспешные logs сохранены отдельно от финальных результатов.

В большой native-origin CF адресно прочитаны все три SettingsStorage: 3/3 ready,
parse issues — 0. Процесс завершился с exit 0; наблюдаемый peak RSS — 60923904
bytes. Исходные bytes/mode/mtime сохранены. Новых source copies и баз 1С этот
probe не создавал. Адресное чтение не подтверждает общий heap/RSS budget.

[Машинные результаты](analyzer-settings-storages-results.json).

## Границы и остаток MVP

Хранилище не получает выдуманных runtime properties, configured value schema
или гарантий работы platform storage. Общий heap/RSS budget, полнота остальных
metadata families, EF-27, editable opening/guarded Save EF-11, обычная установка,
обновление и удаление EF-21, а также сквозная клиентская приёмка EF-16 остаются
в работе. MVP не завершён.
