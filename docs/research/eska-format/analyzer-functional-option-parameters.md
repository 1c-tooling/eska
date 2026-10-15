# Текущие параметры функциональных опций

`FunctionalOptionsParameter` получает отдельную canonical модель и current slot.
`Use` сохраняет ссылки на справочники и измерения регистров сведений, исходные
UUID, имя и ownership. Адаптер использует общий reader квалифицированных ссылок;
неизвестный target, чужой namespace, дубли или неполное собственное объявление
не становятся готовой моделью.

## Native результат

Настоящий платформенный вызов `GetInterfaceFunctionalOptionParameters()` /
`ПолучитьПараметрыФункциональныхОпцийИнтерфейса()` возвращает открытую `Structure`.
Её completion предлагает возможные имена текущих параметров. Тип подсказки
приходит из текущего `Use`: ссылка справочника, тип измерения или их union.
Пустой либо недоступный target оставляет unknown. Объявление параметра не
доказывает, что он установлен в текущем интерфейсе во время выполнения.

Сохраняется native политика изменяемой структуры: типы возможных полей видны
в completion, но не превращаются в жёстко доказанный тип последующего чтения
поля. Это предотвращает ложный `TypeMismatch` после изменения структуры.
Hover настоящего global и signature help используют native callable identity,
нулевую арность и доступный контекст выполнения. Локальная функция с тем же
именем и вызов из другого тела не разрешают подготовку входов.

Холодный запрос готовит только семейство параметров и владельцев его `Use`
через обычный controller. Опции, их storage и скрытые sibling roots этого
разрешения не получают. Несохранённые CF/CFE descriptors, watcher, invalid,
repair, close и restart/replay используют прежний root-qualified lifecycle;
invalid current input не заменяется сохранённым descriptor.

Заимствованный параметр требует совпадающих имени и UUID оригинала. В этом
профиле `Use` может отсутствовать либо совпадать с оригиналом как множество
targets. Добавление, удаление или замена targets в заимствованном параметре
пока отказываются, включая явный пустой `Use` вместо непустого оригинала.
Getter формы, проверка setter и вычисление фактических runtime values не входят
в этот срез.

Analyzer и Explorer теперь сохраняют штатное необязательное поле completion
`detail`: прежние native подсказки типов доходят до установленного редактора.
Поле не даёт прав на commands, дополнительные edits или публикацию исходников.
BSL checker при `CompositionInputIncomplete` возвращает существующее состояние
`analysis_composition_incomplete` и exit 2; прежде этот путь ошибочно становился
внутренней ошибкой с exit 3. JSON остаётся одинаковым на RU/EN.

Экспериментальный Rust API расширен: `MetadataListing`, `CanonicalMetadataInputs`
и `CanonicalMetadata` содержат шестнадцатое семейство, `ConfigsDatabase` — его
per-file resolver. Все прямые потребители обновлены; compatibility layer нет.
Нового XML loader, parser, dependency или crate нет. Source/config schemas,
команды CLI, версии и закреплённая ревизия ядра сохранены.

[Описание платформенного параметра](https://v8.1c.ru/platforma/parametr-funkcionalnoy-opcii/).

## Проверки

На итоговых 1656 Rust/Cargo/locale inputs выполнены шесть корневых gates:
fmt, check, strict Clippy, default/all-features tests и rustdoc с `-D warnings`.
Корневые тесты: 1511 default / 1632 all-features, 24 ignored в каждом профиле.
Одиннадцать SDK gates прошли заново после исправления передачи `detail`:
237 default / 551 all-features tests, strict Clippy/rustdoc, debug/release bins.
Десять kernel/adapter gates прошли для этого среза до исправления `detail`;
они переиспользованы по совпадению всех 569 их Rust/Cargo inputs. Kernel —
4429 passed / 25 ignored; adapter — 116, без failed. Dependencies не обновлялись.

SDK/native тесты проверяют current Use, union/unknown, отсутствие жёсткого
scalar field inference, shadowing и границы тел; CF/CFE visibility, UUID,
неподдержанное изменение Use, hidden inputs, dirty/invalid/repair/watch/close.
Checker проверен на обоих языках для complete и incomplete результата.

Explorer: все 366 Node tests прошли без skipped. Новый VSIX содержит 55
production JS modules, совпадающих с текущей компиляцией; изменён только
`analyzer-language.js`, test helpers в пакет не включены. Установленный клиент
проверил native completion detail и global hover, изменение типа без правки
BSL, invalid retraction, repair и restart/replay в generated CF/configured-CFE.
Прежняя query/metadata матрица и native-origin CF также прошли; native-origin
CFE сохранила явный semantic refusal и доступный syntax. Все 27 дочерних
процессов завершились. 72/61/151 файлов и native-CFE sources сохранились.

Большая native-origin CF дала 3/3 готовых объявления параметров. Один ограниченный
запрос explicit-base/selected-CFE затем остановился на неподдержанной `Sequence`:
`analysis_composition_incomplete`, cold 15.04 s, наблюдаемый peak RSS 391139328
bytes. Warm/update циклы после отказа не запускались. Post-cold memory telemetry
вернула `stale_revision`; общий source/heap budget этим прогоном не подтверждён.
Оба дочерних процесса завершились с exit 0. Инвентари байтов, mode и mtime
исходников сохранились. Совместимость этой пары CF/CFE по оригиналам и UUID
не подтверждена; её нельзя считать успешной приёмкой большой композиции.

Native Save, физический ввод и новый запуск платформы 1С здесь не проверялись.
[Машинные результаты](analyzer-functional-option-parameters-results.json).

## Что остаётся до MVP

- EF-18: оставшиеся semantic metadata families, включая следующий отказ
  `Sequence`, полное покрытие общих реквизитов, inference/editing и общий
  memory budget на подтверждённой большой паре CF/CFE.
- EF-27: полная семантика параметров, выражений и consumers СКД/DynamicList,
  ресурсов, итогов, связей, настроек, временных таблиц и объединений запросов.
- EF-11: общее editable opening, редактирование свойств и guarded Save;
  обычный Save пока проходит вне `SourceEditPlan`.
- EF-21: обычная установка, обновление, удаление и восстановление комплекта.
- EF-16: сквозная приёмка установленного рабочего места и платформенного цикла
  CF/CFE/EPF/ERF, включая большие проекты и память.

Текущий срез завершает параметры опций в описанном профиле; EF-18 и весь MVP
остаются `IN_PROGRESS`. Следующее семейство в этой задаче не реализуется.
