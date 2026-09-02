# Project foundation

## T01 — Доменная модель `Project`

**Статус:** `DONE`
**Зависит от:** B01

Создать минимальные locale-independent типы: `Project`, `ProjectType`
(`configuration`, `extension`, `processing`, `report`) и `SourceFormat`
(`designer-xml`). Зафиксировать поля root/source/configuration; VCS и workflow пока
не реализовывать и не подменять пустыми abstractions.

**Готово, когда:** типы валидируют основные инварианты путей, имеют unit tests и не
зависят от CLI/localization.

**Результат:** добавлены locale-independent `Project`, `ProjectConfiguration`,
`ProjectType` и `SourceFormat`. `Project` принимает абсолютные пути без `..` и
гарантирует, что source находится внутри root; существование директорий намеренно
оставлено discovery-слою T03. Нарушения представлены структурированным
`ProjectPathError`, основные инварианты покрыты unit-тестами.

## T02 — Схема и загрузка `eska.toml`

**Статус:** `DONE`
**Зависит от:** T01

Добавить TOML parsing, компактные defaults и validation для `[project]`: type,
source, source format. Не стабилизировать заранее секции VCS/format и не хранить
пользовательскую locale в project config.

**Готово, когда:** валидный config загружается в модель T01; malformed TOML,
неизвестные enum values и некорректные пути представлены структурированными
ошибками; defaults и serialization покрыты тестами.

**Результат:** добавлена строгая схема `[project]`, чтение файла, parsing,
validation и компактная serialization. Обязательное поле `type` принимает
`configuration`, `extension`, `processing` или `report`; defaults — `source =
"src"` и `source_format = "designer-xml"`. Source остаётся относительным путём
внутри проекта, абсолютные/пустые пути и `..` отклоняются. Malformed TOML, I/O,
неизвестные enum values, поля и нарушения путей представлены вариантами
`ProjectConfigError`; locale и ещё не реализованные секции не принимаются.

## T03 — Project discovery и полная validation

**Статус:** `NEXT`
**Зависит от:** T02

Искать ближайший `eska.toml` вверх от текущей или переданной директории, считать
его каталог project root, разрешать source относительно root и проверять, что это
существующая директория. Добавить локализованный CLI presentation ошибок.

**Готово, когда:** discovery работает из root и глубоких вложенных каталогов;
отсутствие/ошибки config имеют корректные exit codes и тексты на обоих языках.

**Не входит во весь этап:** создание проектов, Git, XML object parsing, build, fmt.
