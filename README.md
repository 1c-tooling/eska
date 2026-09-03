[![CI](https://github.com/1c-tooling/eska/actions/workflows/release.yml/badge.svg)](https://github.com/1c-tooling/eska/actions/workflows/release.yml)
[![crates.io](https://img.shields.io/crates/v/eska.svg)](https://crates.io/crates/eska)
# eska

Текущий этап разработки и очередь задач: [development tracker](docs/roadmap/README.md).

## Проверка проекта

`eska` ищет ближайший `eska.toml` вверх от текущего каталога и проверяет настройки
и каталог исходников. Другую начальную директорию можно передать явно:

```bash
eska --project-dir /path/to/project
eska --lang ru --project-dir /path/to/project/src/CommonModules
```

Минимальный `eska.toml`:

```toml
[project]
type = "configuration"
```

`type` обязателен: `configuration`, `extension`, `processing` или `report`.
По умолчанию `source = "src"`, `source_format = "designer-xml"`; другие форматы
пока не поддерживаются. Source — непустой относительный путь без `..`, допустимо
`.`. Неизвестные поля и секции, включая пользовательскую locale, отклоняются.

Корень проекта — каталог найденного config. Source должен существовать и быть
директорией внутри этого корня, в том числе после разрешения символических
ссылок. Начальный каталог и source приводятся к физическим абсолютным путям;
поиск через ссылку идёт по родителям её целевого каталога. Повреждённый или
недоступный ближайший config не пропускается ради родительского проекта.

Коды завершения: `0` — проект корректен (stdout/stderr пусты), `1` — ошибка
поиска, конфигурации или исходников (локализованный текст в stderr), `2` — ошибка
аргументов CLI. `--help` и `--version` не требуют проекта. Diagnostics ошибок
аргументов пока формирует `clap` на английском.

Команда ничего не меняет и не проверяет XML-содержимое исходников. Создание
проектов, VCS и сборка пока не реализованы.
