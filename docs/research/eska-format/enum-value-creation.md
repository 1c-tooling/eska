# Создание значений перечисления

Четырнадцатая часть EF-24 разрешает новые inline `EnumValue` в существующем
descriptor перечисления. Значения добавляются вручную в YAML; общий protected
selector используется `eska-project prepare` и `eska-build build` без новых
команд или параметров. Поддержаны обе descriptor schemas и layouts 1/2/3.

Для readable descriptor новый элемент `children` выглядит так:

```yaml
- kind: inline
  object:
    kind: EnumValue
    uuid: 60000000-0000-4000-8000-000000000001
    properties:
      Name: НовоеЗначение
      Synonym: {state: empty}
      Comment: {state: empty}
    children: []
    native:
      namespaces: []
      attributes: []
      sections:
      - kind: properties
        items: [Name, Synonym, Comment]
```

UUID должен быть новым в принятой конфигурации и единственным во всём выбранном
рабочем составе. Для каждого элемента нужна собственная identity. `Name` начинается
с Unicode letter или `_`, далее содержит letters/digits/`_`; имена новых и прежних
значений не должны совпадать после `to_lowercase`. Полные ограничения длины и
семантики имени платформы этой лексической проверкой не заявляются.

## Граница разрешения

Авторитет — принятый UUID и class владельца, его native descriptor и independently
supplied pin. `AcceptedBase::permission(owner, Create)` теперь разрешает создание
у принятого `Enum` с допустимой поддержкой/freeze. Это необходимое условие:
selector дополнительно проверяет сам descriptor и профиль нового элемента.
Чужой принятый UUID, изменение рабочего class/UUID владельца или статуса поддержки
не создают разрешений. Прямое `permission(new_uuid, operation)` остаётся
`unknown_object`: рабочий файл не принимает новый объект в базу.

Прежние члены сохраняются по identity и в прежнем относительном порядке.
Новые значения вставляются в группу `EnumValue` перед внешними декларациями,
включая формы; новая ссылка/новый descriptor этим механизмом не добавляются.
Вставка, сдвигающая прежний элемент, проверяет его `Reorder` permission.
Stationary freeze допускает добавление после замороженного значения; перемещение
его позиции запрещает всю эту вставку. Preserved child сохраняет весь принятый
порядок и состав, без предположений о неизвестной native семантике.

Профиль нового значения закрыт: `Name`, `Synonym`, `Comment` в native порядке,
без дополнительных attributes/namespaces, children, generated types или opaque
XML. Synonym принимает Empty/Localized, Comment — Empty/Text. Некорректный
разрешённый профиль/имя или коллизия отказывают до публикации. Остальные native
профили требуют своих правил и платформенной приёмки.

Запрещённая вставка получает `{"kind":"create"}` с действительным UUID
владельца или сдвигаемого запрещённого элемента. Strict не создаёт поколение;
явный `--allow-exclusions` исключает добавления, сохраняя независимые разрешённые
свойства. Создание вместе с удалением или перестановкой прежних членов этой
частью не поддержано. Рабочие YAML, manifest, accepted storage и CI pin не меняются.

## Чтение и source map

Saved/unsaved reader включает новый UUID и его свойства в `All`/`Summary`
непосредственно из YAML. Добавление inline значения не требует ручной правки
`source.json`, hashes или обязательной обратной карты путей.

Metadata origin сохраняет принятый input владельца; `uses_working_values`
отмечает также разрешённые новые inline members. Координаты нового объекта
и свойства относятся к Working. Для них mapper проверяет fingerprint рабочего
input и точное равенство рабочей модели нового узла с выбранным native узлом;
он не придумывает accepted span. Отсутствие/изменение рабочего текста даёт отказ.
Данные source map сами по себе не разрешают сборку.

Версии schemas/rules/tool protocol и прежние поля JSON сохранены. Публичный
экспериментальный permission API расширил существующий `Create`; отдельная
CST-команда создания и точечное редактирование ещё не принятых UUID не добавлены.
До отдельного принятия базы новое значение правится вручную как создаваемый узел.
Проверка BSL-обращений остаётся задачей анализатора, а клиентское поведение —
отдельной приёмкой; автоматического переписывания BSL нет.

## Проверки

Прошли 16 root gates: fmt/check/strict Clippy/default и all-features tests,
Rustdoc, minimal core/project-cli/build-cli boundaries и diff check.
Root — 466 unit + 556 integration, 24 opt-in пропущены; minimal project CLI —
273 unit + 19 integration. Добавлены 11 core, два integration и два CLI tests.
Покрыты обе оболочки, empty collection, глобальные UUID, имена/профиль,
поставщик/freeze, смешанные правки, source locations, сохранность и повторная
generation, saved/overlay reader и обе локали. Прошли 14 SDK gates (359 tests)
и пять gates адаптера BSL (70 unit + 12 integration), оба release build.

Реальная C1: три значения в двух YAML, в том числе перед отдельной формой,
побайтно совпали с независимым native контролем по 8 778 файлам/8 077 каталогам.
Проверены английское/кириллическое имя, Empty/Localized synonym, Comment,
stationary freeze и отдельный отказ при его сдвиге. Восстановление сохранило
допустимое добавление другого владельца, обе локали дали одинаковый JSON.
Рабочие/принятые/native inputs, manifest и pin сохранены, exclusions — 0
у разрешённого результата. Самостоятельный release CLI работал с PATH без
платформы, Git и оболочки, без машинной конфигурации.

Все 3 595 descriptors/8 333 объектов совпали с независимым контролем в saved/
unsaved `All`/`Summary`. Обе CF собраны и повторно выгружены на 1С 8.3.27.2325:
8 769 файлов/8 064 каталога равны кроме 7 011 `configVersion`. Проверены
manifest checksums; версия проекта, source/base/settings и оригинальные native
inputs сохранены. [Обезличенные результаты](enum-value-creation-results.json).

EF-24 остаётся `IN_PROGRESS`: другие structural profiles, delete/rename,
составные домены и workspace migration ещё нужны. Клиентское исполнение,
обновление рабочей ИБ и новые CFE/EPF/ERF этой частью не приняты.
