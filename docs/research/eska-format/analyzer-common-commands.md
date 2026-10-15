# Текущие параметры общих команд

`CommonCommand` имеет отдельную canonical модель и listing в закреплённом native
kernel. Адаптер передаёт UUID, имя, `CommandParameterType`, `ParameterUseMode`
и ownership из общего текущего `ParsedObject`. XML sidecar, directory scan,
второй parser и подстановка сохранённой версии не используются.

Платформа задаёт тип параметризованной команды в метаданных и выполняет её
алгоритм в модуле команды. Это подтверждает
[описание команды](https://v8.1c.ru/platforma/komanda/). Reference MCP подтвердил
отдельные `CommandParameterType`, `ExtendedConfigurationObject` и значения
`CommandParameterUseMode.Single/Multiple`.

## Native потребитель

Manifest связывает объявленный `CommandModule.bsl` с точным descriptor FileId.
Native `ModuleMetadata.common_command` получает текущие свойства того же
visibility chain. Заимствование требует исходного UUID и того же имени;
непереданные свойства наследуются, переданные заменяют предыдущие значения.
Invalid/pending вклад, неверный UUID и недоступная база не заменяются namesake
из другого root. Hidden sibling не участвует.

Native inference передаёт объявленный тип первому параметру процедуры
`ОбработкаКоманды` / `CommandProcessing` с двумя параметрами. Имя самого
параметра произвольно; helper, одноимённая функция и неправильная arity такого
типа не получают. `Multiple` даёт массив значений объявленного типа, включая
тип элемента при индексировании. Пустой TypeDescription остаётся Unknown;
отсутствие значения не превращается в предполагаемый тип.

Обычная команда обязана явно задать type/use; заимствованная может наследовать
эти свойства. Неизвестные enum/type, дубли и malformed explicit booleans дают
адресную ошибку адаптера. TypeDescription использует общий адаптер типов.
CommonCommand не становится MDO или StructureOnly.

## Текущие запросы SDK и клиента

Completion, hover и signature preparation готовят собственный descriptor до
поиска receiver, поскольку тип локального параметра команды зависит от YAML.
Семантические запросы и diagnostics проверяют ту же native композицию;
отказ передаётся как существующий root-qualified `analysis_composition_incomplete`.
Синтаксический запрос сохраняет отдельную доступность.

Native positional symbol card теперь передаёт известный выведенный тип
параметров и локальных переменных через существующее поле `return_type`.
Presentation выводит «Тип» / «Type» для значений и прежний заголовок результата
для функций/методов. Обе локали обновлены; wire schema прежняя.

Cold completion узнаёт один доказанный reference receiver через прежний native
inference и готовит только соответствующий descriptor. Это также действует
для элемента массива. Неоднозначный union не разрешается выбором одного arm;
полная transitive preparation этим не объявляется.

Правка типа/режима YAML меняет результат без изменения BSL input. Dirty input,
invalid/retraction, repair, watch, close и restart/replay используют прежний
current input controller. Native ranges и source hashes сохраняют координаты
BSL, включая BOM, CRLF и non-BMP символы.

## API и границы

Экспериментальные `CanonicalMetadata`, `MetadataListingData` и `ModuleMetadata`
получили отдельные Command variant/list/owner. Native `common_command_for_file`
возвращает текущего владельца либо FileId непроверенного вклада. Эти source API
изменены без compatibility layer. CLI/config/JSON schemas, версии пакетов и
dependencies не менялись; локальных пакетов ядра по-прежнему девять.

Принят только top-level CommonCommand. Inline object commands, второй параметр
контекста, interception/weaving inference и полный command interface остаются
отдельными границами. Настоящие native XML consumers вне canonical ESKA не
получили нового command XML loader. EF-18 и MVP этим срезом не закрываются.

## Проверки

Корневой gate: fmt, check, strict all-targets/all-features Clippy, default и
all-features tests, strict rustdoc. Тесты — 1511/1632 passed, по 24 ignored.
Все 749 корневых Rust/Cargo/config/locale inputs сохранились после последующего
уточнения только SDK; корневой Cargo workspace от native/SDK не зависит.

Отдельный frozen native/adapter/SDK gate: 21 команда с fmt, check, strict Clippy
и rustdoc, default/all SDK tests и debug/release binaries. Kernel — 4423 passed /
25 ignored; adapter — 106; SDK — 221 default / 528 all-features, без ignored.
1619 Rust/Cargo/config/locale inputs сохранились между началом и концом этого gate.

Framed hover RU/EN проверил current YAML, родной UTF-16 range, invalid/refusal,
repair и close. Checker single/multiple × известный/пустой тип × RU/EN проверил
UnresolvedField и exit 1/0 через тот же native inference, при пустом PATH.
Синтетический configured CFE проверил current base, hidden sibling, неверный UUID
и доступность синтаксиса при отказе семантики.

Explorer: 344 Node tests, без skipped. Установленный прежний VSIX и новый frozen
release analyzer проверили actual native hover provider, тип/режим параметра,
dirty base в configured CFE, invalid/repair и restart/replay. Прежняя consumer
матрица CF/configured-CFE/native-CF и syntax/refusal native-CFE также прошла.
21 дочерний процесс завершился; bytes всех четырёх проектов сохранились.
Native Save, физический ввод пользователя и новый запуск платформы 1С не проверялись.
Прежняя причина единичного пропуска import marker не установлена; успешный
текущий прогон не объявляет её исправленной.

Readonly адресная проверка большой native конфигурации дала ready для всех
58 CommonCommand без parse issues. Композиция остановилась на следующей family
FilterCriterion: cold 14.98 s, peak RSS 393396224 bytes. Исходники сохранились,
процессы завершились. Это отказ, не приёмка производительности; фактическая
совместимость выбранной большой CF/CFE пары по-прежнему не подтверждена.

[Машинные результаты](analyzer-common-commands-results.json) сохраняют разделение
gates, synthetic cases, установленного клиента и native readonly проверки.
