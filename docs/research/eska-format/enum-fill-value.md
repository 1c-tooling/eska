# Ссылка на EnumValue в FillValue — EF-24, часть 18

Именованное значение заполнения реквизита принимает ссылку
`Enum.<enum>.EnumValue.<value>` как одно редактируемое значение. Общий core
используется importer, protected selector, reader, CST editor и SDK; дополнительных
CLI-команд или XML/YAML-копий свойства нет.

## Принятый профиль

Владельцы: `Attribute`, `CommonAttribute`, `Dimension`, `Resource`. Нативный
`FillValue` содержит только текст и один namespace-resolved `xsi:type`, чей тип —
`DesignTimeRef` из `http://v8.1c.ru/8.3/xcf/readable`. Локальные declarations,
дополнительные attributes, children/comments и иной тип остаются preserved.
Префиксы не обязаны называться `xsi`/`xr`, но должны разрешаться по URI.

В family YAML `PropertyValue::EnumFillValue` содержит `reference` и native
prefixes. В readable YAML `properties.FillValue` — непосредственная строка
ссылки; её native slot содержит только `enum_fill.xsi_prefix/type_prefix`.
Значение встречается один раз. `PropertyName::FillValue`, новый вариант
`PropertyValue` и структуры `EnumFillValue/EnumFillValueLayout` расширяют
экспериментальный Rust API; версии envelope/manifest/обратной карты прежние.

`nil`, `<FillValue/>`, `EmptyRef`, UUID-пары, строки/числа/даты/boolean, ссылки
на predefined values и form payloads этой частью не типизируются. Они сохраняют
исходный XML и проверяются прежними правилами preserved-фрагментов. Смена типа
значения, создание отсутствующего binding и очистка до Nil/Empty не разрешены.

## Выбор и редактирование

Поле изменяется только по independently pinned native policy и freeze своего
принятого UUID. Helper prefixes восстанавливаются независимо от разрешённой
правки ссылки; их изменение остаётся exclusion. Strict не публикует поколение,
явный `--allow-exclusions` сохраняет независимые разрешённые правки.
Точечный editor требует прежний binding и сохраняет quotes/comments/EOL;
равное значение — побайтный no-op. Миграция family → readable переносит
аннотации ссылки и prefixes по исходным координатам.

Target index проверяет существование EnumValue в окончательном объявленном
составе. Lookup допускает изменение регистра по `to_lowercase`; родной target
spelling не нормализуется. Старое имя после rename/delete отказывает до
публикации. Разрешённое исправление FillValue в той же операции принимается;
frozen или supplier-locked исправление восстанавливает прежнюю зависимость.
Opaque XML/resources продолжают проверяться общим structural guard.

Существование значения не доказывает совместимость с Type конкретного поля,
inverse semantics или runtime обращениями. Эти ограничения проверяются
платформенной сборкой; глобальное переписывание BSL/форм не заявляется.

При открытии и миграции старого проекта preserved FillValue не становится
typed автоматически. Классификация закреплённого source/base сохраняется;
новый профиль используется при явном импорте native input.

## Чтение и проверки

Saved/unsaved All/Summary сохраняют native `DesignTimeRef` qualifiers и typed
text семантику. Source map показывает точную рабочую строку ссылки, включая
legacy `value.reference`; служебные координаты не становятся editable values.

Core tests проверяют оба envelopes, четыре native owners, QName aliases,
unsupported profiles, reference/domain/namespace validation, point edits/no-op,
freeze, tampered helpers, окончательный rename/delete, case-only rename,
reader/source map, аннотации и сохранение старой preserved классификации.
Supplier integration и отдельные CLI processes проверяют mixed policy,
RU/EN, strict/exclusions, выходные коды и отсутствие публикации при отказе.

Прошли 16 root/14 SDK/5 adapter gates и оба release build: 505 unit + 563
integration, 24 opt-in пропущены; minimal project — 312 unit + 26 CLI tests,
SDK — 359, adapter — 70 + 12. Добавлены 12 core, один supplier integration
и два CLI tests.

Инвентаризация C1 нашла 2 598 metadata FillValue, из них шесть именованных
EnumValue references в шести descriptors (Attribute/Resource). Два Attribute
разрешают изменение по native policy. Они изменены в двух YAML с сохранением
прежних helpers; независимый native контроль совпал по 8 778 файлам/8 077
каталогам. Frozen field refusal/recovery, отсутствующая цель и иной класс
проверены в отдельных CLI processes RU/EN, strict/exclusions, без публикации
невалидного результата. Все 3 595 descriptors/8 330 объектов совпали с контролем
в saved/unsaved All/Summary; ресурсный профиль проверен чтением, его изменение
этим real probe не принималось.

Контроль и ESKA собраны в CF и выгружены на 1С 8.3.27.2325: 8 769 файлов/
8 064 каталога равны кроме 7 011 configVersion; checksums, source/base/native
inputs, settings, manifest, pin и версия сохранены. Клиент, рабочая ИБ,
BSL-runtime и новые CFE/EPF/ERF здесь не проверялись.
[Обезличенные результаты](enum-fill-value-results.json).

EF-24 — `IN_PROGRESS`; остальные compound/structural profiles и workspace
migration ещё нужны.
