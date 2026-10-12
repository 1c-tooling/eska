# Удаление значений перечисления

Пятнадцатая часть EF-24 разрешает удаление существующих inline `EnumValue`
из YAML принятого перечисления. Общий protected selector используется
`eska-project prepare` и `eska-build build`; новые команды/параметры не нужны.
Поддержаны descriptor schemas 1/2 и layouts 1/2/3.

Автор удаляет целый элемент из `children` либо оставляет явный пустой массив.
Descriptor, обязательная обратная карта и native service slot сохраняются;
ручная правка `source.json`, hashes и CI pin не нужна. Отсутствующий owner-файл
или свойство `children` не означают разрешение удалить всё перечисление.

## Граница разрешения

Владелец подтверждается UUID/class принятого native descriptor. Допускается
строгий уникальный ordered subset прежнего состава: новые UUID, смена владельца,
смешанные create/delete и перестановки оставшихся членов не выбираются.
Удаляются только inline значения с проверенным профилем Name/Synonym/Comment,
без дополнительных attributes/namespaces, children, generated types или opaque
данных. Внешний descriptor формы остаётся объявленным. Preserved child не даёт
интерпретировать неизвестную семантику коллекции.

Проверяются `Reorder` владельца, `Delete` каждого удаляемого значения и `Reorder`
каждого сдвигаемого соседа. Stationary freeze допускает удаление последующих
членов; удаление самого frozen значения либо сдвиг его позиции запрещены.
Экспериментальный `AcceptedBase::permission(uuid, Delete)` теперь допускает
принятый EnumValue с подходящей native поддержкой/freeze. Это необходимое
условие; фактический descriptor и профиль дополнительно проверяет selector.
Остальные классы не получили разрешения удаления.

При запрете вся редукция коллекции восстанавливается, независимые допустимые
свойства выбираются по UUID. Для каждого отсутствующего значения записывается
одна `{"kind":"delete"}` диагностика с действительным запрещённым владельцем/
элементом либо UUID удаляемого члена для неподдержанного профиля. Strict
отказывает до публикации; явный `--allow-exclusions` восстанавливает состав.

## Зависимости

Typed-ссылки проверяются по итоговому объявленному дереву после восстановления
запрещённых правок. Например, остающийся Subsystem.Content на удалённый EnumValue
даёт `source_metadata_reference_missing`. Разрешённая правка ссылки в том же
выбранном составе допускается; запрещённая правка не скрывает зависимость.

После выбора metadata и payload origins дополнительный guard проверяет
preserved XML-секции/свойства/children, transport attributes и native XML
ресурсы. Ищутся упоминания принятого UUID и полного
`Enum.<owner>.EnumValue.<value>` после XML entity/CDATA decoding и `to_lowercase`.
Совпадение в тексте или attribute даёт `source_structural_reference_preserved`
с native path и UUID. Guard консервативен: текстовое упоминание может не быть
ссылкой, но неизвестная inverse семантика не угадывается и не переписывается.
Comments/PI не имеют авторитета; namespace scopes сохраняются при разборе
фрагмента. Неразбираемый XML payload даёт `source_structural_xml_unverified`.
Оба отказа происходят до публикации и не обходятся `--allow-exclusions`.

Проверяются выбранные байты: запрещённый ресурс восстановлен из независимо
pinned storage и сохраняет свою зависимость. Разрешённое исправление XML
ресурса проверяется с рабочим fingerprint. Без реального удаления guard не
читает дополнительные payload. ConfigDumpInfo — производный service index,
а не авторитет состава, он исключён из проверки XML-зависимостей.

Проверка бинарных/non-XML ресурсов, BSL/SDBL-обращений и вычисляемых ссылок
этой частью не заявляется. Автоматическое переписывание BSL и UI находятся
вне EF-24; применение к данным рабочей ИБ требует отдельной приёмки.

## Чтение и карта

Saved/unsaved reader отражает текущий состав в `All`/`Summary`; удалённый UUID
не возвращается как ghost member. Source map сохраняет прежние DTO и native
пути; координаты разрешённого изменённого свойства выжившего элемента находятся
по UUID рабочего YAML после сокращения списка. Удалённый узел не имеет native
координаты в выбранном результате.

Schemas/rules/tool protocol и wire kind `delete` прежние. Рабочие файлы,
manifest, accepted storage и independently supplied pin не меняются.
Новая CST-команда удаления и принятие текущего состава в базу не добавлены.

## Проверки

Добавлены 12 core, один supplier integration и два CLI tests: обе оболочки,
ordered/empty composition, профиль/идентичности, policy/freeze, независимые
правки, decoded preserved references, transport attributes, выбранные ресурсы,
typed dependencies, source locations, saved/unsaved reader и обе локали.
Прошли 16 root/14 SDK/5 adapter gates и оба release build: 478 unit + 557
integration, 24 opt-in пропущены; minimal project CLI — 285 unit + 21 integration,
SDK — 359, adapter — 70 + 12. Полный набор включает fmt/check/strict Clippy,
default/all-features tests, Rustdoc и minimal core/CLI boundaries.

Три удаления C1 в двух YAML, включая значение перед внешней формой, совпали
с независимым native контролем по 8 778 файлам/8 077 каталогам. Stationary freeze
сохранён; отдельная попытка удалить frozen значение отказала, а восстановление
сохранило независимое допустимое удаление. Самостоятельный release CLI работал
без платформы, Git и оболочки в PATH; обе локали дали одинаковый JSON.
Существующая реальная DesignTimeRef к другому удаляемому значению отказала
в обоих режимах и локалях, без generation или изменения authoring bytes.
Все 3 595 descriptors/8 327 объектов совпали с контролем в saved/unsaved
`All`/`Summary`.

Контроль и ESKA собраны в CF и повторно выгружены на 1С 8.3.27.2325:
8 769 файлов/8 064 каталога равны кроме 7 011 `configVersion`. Checksums
артефактов проверены; версия проекта, исходники/база/settings и оригинальный
native input сохранены. [Обезличенные результаты](enum-value-deletion-results.json).
Права/профиль удаления не объявляют готовыми остальные операции.

EF-24 остаётся `IN_PROGRESS`: rename, остальные structural profiles, составные
домены и workspace migration ещё нужны. Формы/запросы и CFE/EPF/ERF принимаются
следующими задачами очереди MVP.
