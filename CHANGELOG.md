# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.0](https://github.com/1c-tooling/eska/compare/v0.2.1...v0.3.0) - 2026-09-05

### Added

- *(vcs)* Добавлено переключение между задачами
- *(save)* Добавлен deterministic semantic draft
- *(save)* Добавлена передача draft в редактор
- *(diff)* Добавлена группировка semantic diff
- *(diff)* Добавлен режим semantic diff
- *(semantic)* Добавлены события semantic diff
- *(semantic)* Добавлен reusable ChangeSet pipeline
- *(xml)* Добавлена логическая модель объектов Designer
- *(history)* Email автора сделан кликабельным
- *(cli)* Добавлена команда history
- *(history)* Добавлена модель локальной истории
- *(vcs)* Переведён запуск задачи на gix-first слой
- *(project)* Добавлено клонирование проекта через gix

### Fixed

- *(workflow)* Разрешена настройка имён веток preset
- *(save)* Добавлена отмена неизменённого draft
- *(save)* Сохранена локаль редактора commit
- *(history)* Изменён порядок полей
- *(history)* Дата сделана человекочитаемой

### Other

- *(roadmap)* Начата реализация завершения задачи
- *(roadmap)* Завершено переключение задач
- *(roadmap)* Начата реализация переключения задач
- *(roadmap)* Сформирован практический CLI MVP
- Revert "fix(save): Добавлена отмена неизменённого draft"
- *(roadmap)* Завершён генератор сообщений
- *(roadmap)* Начата реализация генератора сообщений
- *(roadmap)* Завершён semantic diff
- *(roadmap)* Начата реализация semantic diff
- *(roadmap)* Завершён semantic ChangeSet
- *(roadmap)* Начата реализация semantic ChangeSet
- *(roadmap)* Завершена модель Designer XML
- *(roadmap)* Начата реализация модели Designer XML
- *(roadmap)* Завершена реализация истории
- *(roadmap)* Начата реализация истории
- *(roadmap)* Завершена gix-first миграция VCS
- *(roadmap)* Начата gix-first миграция VCS
- *(roadmap)* Перестроен backlog с приоритетом gix
- *(deps)* Обновлены зависимости Rust

## [0.2.1](https://github.com/1c-tooling/eska/compare/v0.1.1...v0.2.1) - 2026-09-04

### Added

- *(diff)* Сгруппирован вывод по состояниям
- *(save)* Добавлено сохранение изменений проекта
- *(diff)* Добавлено сравнение Git-ревизий
- *(diff)* Добавлено представление объектов метаданных
- *(cli)* Добавлена команда eska diff
- *(cli)* Добавлена команда eska start
- *(vcs)* Добавлен запуск задачи по workflow policy
- *(cli)* Добавлена команда eska status
- *(vcs)* Добавлен встроенный GitHub Flow preset
- *(vcs)* Добавлен встроенный Git Flow preset
- *(init)* Добавлено создание Git-файлов
- *(new)* Добавлены встроенные Git-шаблоны
- *(vcs)* Добавлен встроенный Trunk preset
- *(config)* Добавлены custom workflow policy и локализованная проверка
- *(vcs)* Добавлена модель workflow policy и план задачи
- *(vcs)* Добавлено состояние индекса и рабочих файлов
- *(vcs)* Добавлено чтение репозитория, ссылок и истории
- *(cli)* Добавлено безопасное подключение Designer XML проектов
- *(tui)* Добавлен клавиатурный выбор при создании проекта
- *(cli)* Добавлено безопасное создание проектов
- *(templates)* Добавлены встроенные шаблоны проектов
- *(project)* Добавлены поиск и проверка проекта
- *(config)* Добавлена загрузка eska.toml
- *(project)* Добавлена доменная модель проекта
- Добавлена локализация
- Очистка тестовых функций перед реализацией.
- *(fmt)* интеграция tree-sitter-bsl и базовая архитектура форматтера
- добавил команду fmt, удалил init

### Fixed

- *(diff)* Учтены служебные файлы метаданных
- *(start)* Разделена обработка отсутствующего и недоступного remote
- *(vcs)* Уточнено определение обновления базовой ветки
- удален IDE из gitignore
- *(init)* Исправлено определение типа проекта
- *(tui)* Добавлена навигация в русской раскладке
- *(test)* Устранены коллизии временных каталогов
- *(cli)* Локализована строка использования в справке

### Other

- Включил публикацию на crates.io
- *(diff)* Описано оформление групп изменений
- *(save)* Завершена задача сохранения изменений
- *(diff)* Описано сравнение Git-ревизий
- *(diff)* Завершено распознавание Designer XML
- *(diff)* Описано представление объектов метаданных
- *(vcs)* Описана команда eska diff
- *(vcs)* Описана команда eska start
- *(structure)* Уточнено расположение файлов проекта
- *(cli)* Выровнен вывод eska status
- изменение CODEX.md
- *(structure)* Упорядочены модули CLI и проекта
- *(cli)* Изолированы проверки в настраиваемом playground
- Добавлена инструкция для Codex
- Roadmap и задачи
- cargo update
- обновить зависимости
- отключил публикацию на crates.io
- release v0.2.0
- перевод help на Русский язык
- изменил хуки
- обновил зависимости

## [0.1.0](https://github.com/1c-tooling/eska/releases/tag/v0.1.0) - 2025-10-30

### Added

- *(ci)* add release-plz GitHub Actions workflow
- initialize Rust project with hello world
- add lefthook configuration file

### Fixed

- *(ci)* изменить скрипт ci

### Other

- Initial commit
