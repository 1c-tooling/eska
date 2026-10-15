# Текущие функциональные опции

`FunctionalOption` получает отдельную canonical модель и listing. Общий
адаптер читает UUID, имя, `Location`, `PrivilegedGetMode`, `Content` и ownership
из текущего `ParsedObject`. Новый фиктивный MDO, XML sidecar и второй parser
не добавляются.

Тип опции определяется хранилищем: константой, реквизитом справочника или
ресурсом регистра сведений. Он не обязан быть булевым.
[Описание 1С](https://v8.1c.ru/platforma/funktsionalnaya-opciya/) и
[руководство 8.3.27](https://1c-dn.com/library/tutorials/1c_enterprise_developer_guide_8_3_27/?sphrase_id=1678862)
подтверждают это разделение. Руководство также описывает `NULL` при обращении
к реквизиту, недоступному для группы справочника. Поэтому такой storage
сохраняет объединение объявленного типа с `NULL`: использование реквизита
и вид переданного во время выполнения значения пока не доказываются.

## Native потребители

Настоящий `ПолучитьФункциональнуюОпцию` / `GetFunctionalOption` с литеральным
именем получает текущий тип выбранного хранилища через native inference.
Сохраняются platform ID, параметры, arity, execution environments и call
candidates. Hover результата и глобального вызова, signature help и checker
используют ту же привязку; SDK не создаёт отдельные карточки типов.

Пустое хранилище или недоступный тип не превращаются в `Boolean` и не
получают прежний успешный тип. Нелитеральный аргумент остаётся с обычным
контекстно-независимым native результатом.

Cold preparation находит только native вызовы настоящей platform global
в текущем теле процедуры/функции. Она готовит названную опцию и её точный
storage owner через прежний input controller. Комментарий, другое тело,
динамическое имя или одноимённая пользовательская функция не разрешают
чтение опции. Исправлен обнаруженный при этом native приоритет: объявленная
функция модуля получает вызов раньше реестра встроенных функций и сохраняет
собственные candidates.

## Current input и композиция

Own declaration явно задаёт `Location`, `PrivilegedGetMode` и `Content`.
QName-aware список `Content` читает native элементы `xr:Object`; общий reader
сохраняет отдельный entry kind для каждой reference-list property.
Заимствованный объект может опустить их. Отсутствующий `Location` наследуется
только после проверки имени и исходного UUID; явно пустой отличается от
отсутствующего и не скрывает несовместимость с базовым хранилищем.

Допустимый вклад добавляет `Content`, сохраняя базовое хранилище. Неверный
UUID, иной storage, invalid или unavailable видимый вклад дают существующий
`analysis_composition_incomplete`. Отдельный syntax route остаётся доступен.
Dirty authority, watch, repair, close и restart/replay принадлежат прежнему
controller, включая foreign base. Правка storage меняет тип результата
без изменения BSL input; сохранённые bytes не подставляются вместо draft.

## API и границы

Экспериментальные `CanonicalMetadata`, `MetadataListingData`, listing input
и native resolver API расширены отдельной family без compatibility layer.
CLI/config/JSON schemas, версии, зависимости и состав девяти локальных
пакетов kernel прежние.

Этот срез выводит тип результата `GetFunctionalOption`, но не вычисляет
значение опции, права или видимость интерфейса. `Content` сохраняется, а не
выключает анализ BSL. Параметры функциональных опций, form/interface getters,
правильность runtime набора параметров и кэширование значений ещё нужны.
Classic XML Configuration не получает второй XML loader.
Первый срез проверял hover локального результата с явным `Var`.
[Native positional card неявного local](analyzer-implicit-local-hover.md)
подключена отдельным исправлением и прошла те же current CF/CFE сценарии.
EF-18 и MVP остаются в работе.


## Проверки

Native/adapter/SDK прошли 21 gate: fmt, check, strict Clippy/rustdoc, default/all
SDK tests и debug/release binaries. Kernel — 4426 passed / 25 ignored; adapter —
113; SDK — 232 default / 543 all-features, без ignored. После исправления
`Content` повторены все 16 adapter/SDK gates; пять kernel gates переиспользованы
по совпадению всех его Rust/Cargo inputs. Все 1645 inputs итогового прогона
сохранились. Шесть корневых gates прежнего CommonCommand-среза переиспользованы
по совпадению 749 inputs; корневой workspace не зависит от SDK/kernel.

Тесты проверили три storage вида и явный empty, current type, `NULL`, native
callee hover/signatures, cold named preparation, user shadowing, watch,
invalid/repair/close, UUID refusal, foreign base и отдельный syntax route.
Настоящий checker на RU/EN вернул одинаковый JSON с ожидаемым UnresolvedField
и exit 1 для reference storage. BOM/CRLF и исходные BSL bytes сохранились.

Explorer прошёл 361 Node test без skipped. Прежний VSIX содержит те же
55 production modules, без test helpers. Установленная матрица подтвердила
current result/global hover, dirty storage, invalid retraction, repair и
restart/replay для CF/configured-CFE, а также предыдущие query/metadata
consumers. Все 25 children завершились; 57/49/151 файлов и native-CFE sources
сохранились. Native Save, физический ввод и новый запуск 1С не проверялись.

Первый readonly прогон большой конфигурации показал 13 ready / 30 invalid:
`Content` использовал native `xr:Object`, а первоначальный адаптер ожидал
`Item`. Общий reader теперь принимает entry kind от семейства; другие lists
сохранили прежний строгий контракт. Новый fixture с непустым native Content
прошёл adapter, SDK и установленный клиент.

Повторный readonly прогон дал 43 ready, без parse issues. Следующий отказ
большой CF/CFE композиции — FunctionalOptionsParameter: cold 15.00 s, peak RSS
392417280 bytes. Исходники bytes/mode/mtime сохранились, оба процесса завершились.
Это семантический отказ, не приёмка производительности; совместимость большой
CF/CFE пары ещё не подтверждена.

[Машинные результаты](analyzer-functional-options-results.json).
