# Проекция роли и её прав

`eska-bsl-adapter::role` собирает модель выбранного ядра из общего descriptor
роли и явно переданного `RoleData`. UUID и имя берутся из текущего YAML; адаптер
не читает диск и не создаёт основной XML descriptor.

## Два разных входа

В формате ESKA описание роли хранится в `Roles/<name>/object.yaml`, а ресурс
`Roles/<name>/Ext/Rights.xml` пока сохраняет native bytes. Это существующее
поведение импорта, а не новый XML fallback. Для сборки модели нужны оба входа:
валидное описание роли не доказывает, что её права загружены.

Адаптер принимает уже разобранные текущие права по значению. Пустой `RoleData`
допустим как явное решение caller для действительно отсутствующего ресурса.
Pending, unreadable и invalid объявленного ресурса нельзя заменять пустыми
правами. Пример `project_roles` читает только объявленный manifest ресурс и
возвращает ошибку разбора; мягкий fallback legacy loader не используется.
Контроль происхождения, версий и сохранности editor input относится к общей
сессии. Подключение этого lifecycle и canonical resolver ещё не выполнено.

`Role::with_data` сделан публичным в локальном `bsl-metadata` 0.2.85 для этого
текущего consumer. Он переносит исходную identity и переданный payload;
`Role::new`, который создаёт UUID, для источников не используется. Версии,
serialized shape, dependencies и protocol не менялись.

## Что представляет модель

Сохранены три флага, порядок ссылок на объекты и тексты условий RLS. Используется
существующий
[native rights reader](https://raw.githubusercontent.com/1c-tooling/bsl-analyzer/9021ba07471eee8a2614bed2105a2c8699f60780/crates/bsl-metadata/src/xml_parser/role.rs).
Он объединяет ссылки на реквизиты с уровнем объекта, сохраняет повторные записи,
пропускает `Configuration.*` и неизвестные типы, обрезает внешние пробелы RLS.
Отдельные значения прав `true/false`, шаблоны ограничений и полная модель доступа
в `RoleData` не представлены. Даже запись с `value=false` может оставаться
ссылкой графа. Это сведения для анализа связей, не проверка эффективных прав
пользователя и не выполнение RLS в 1С.

## Проверки

Четыре unit-теста проверяют равенство native модели, исходную identity, текущую
замену rights payload, явный absent input, неверный kind/UUID и неполные captures.
Файловый тест импортирует настоящую пару descriptor/resource, проверяет current
YAML, другую модель прав, invalid/close и сохранность bytes. Исходный descriptor
XML в проект не возвращается.

На C1/C2/C3 совпали все 212/676/753 модели роли и их 212/676/753 ресурса прав.
Сравнение включает соответственно 223 334/29 458/155 734 объектных ссылок и
66/4 055/6 495 условий RLS. Проверены hashes 850/2 706/3 014 YAML, resource,
native и control files; исходники не записывались. PATH был пустой, платформа
не запускалась. Детали и обязательные gates записаны в
[машинном отчёте](role-adapter-results.json).

Прошли все 6 adapter/5 kernel/19 SDK/15 root gates: fmt/check, strict
Clippy/Rustdoc, tests и feature matrix. Адаптер — 64 unit/7 file tests,
SDK — прежние 270 tests, kernel — 2626 passed/20 прежних ignored вместе с
Rustdoc examples, root — 895 passed/14 прежних ignored.

Сервер и checker пока не считают роли готовыми canonical inputs. EF-18 остаётся
`IN_PROGRESS`: зависимые resources, остальные модели, общая память, edits и
Explorer ещё в работе.
