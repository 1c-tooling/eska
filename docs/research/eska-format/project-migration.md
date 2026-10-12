# Миграция проекта через CLI, двенадцатая часть EF-24

`eska-project migrate new-project` готовит читаемую копию standalone-проекта.
По умолчанию команда только показывает план. `--write` публикует новый каталог;
исходный проект, его база и внешний pin CI не меняются.

```sh
eska-project --project-dir old-project migrate new-project \
  --accepted-base-sha256 "$APPROVED_OLD_BASE_SHA256" --format json
eska-project --project-dir old-project migrate new-project \
  --accepted-base-sha256 "$APPROVED_OLD_BASE_SHA256" --write --format json
eska-project --project-dir new-project prepare \
  --accepted-base-sha256 "$APPROVED_NEW_BASE_SHA256" --format json
```

Destination разрешается относительно текущего каталога процесса. Как и у
`import`, его родитель должен существовать. Для нового prepare нужно отдельно
одобрить и передать возвращённый `data.baseline`; old pin не разрешает новую базу.
Команда принимает Designer-derived ESKA configuration, а не XML/CF напрямую.

## Два независимых источника

Общий `prepare_readable_project` связывает прежние [планы рабочих исходников](readable-layout.md)
и [закреплённой базы](accepted-layout.md). Текущие YAML/BSL и аннотации сохраняются
в рабочих исходниках. Новый accepted storage строится исключительно из blobs
самостоятельно проверенной old base; рабочие правки туда не принимаются.
Проверяются общий native origin и configuration UUID, прежние native rules,
supplier evidence и inherited freeze. Unknown semantics и неподдержанные
аннотации по-прежнему приводят к отказу, preserved values не типизируются заново.

Новый проект использует layout 3/descriptor 2 и прежние относительные source/base
paths. `eska.toml`, существующие `.gitattributes` и `.gitignore` копируются
побайтно, включая комментарии и настройки. Остальные файлы проекта, `.git`,
артефакты и generations не копируются. Отсутствие transport files тоже входит
в ожидаемый снимок; effective Git policy эта операция не подтверждает.

Первый orchestration поддерживает standalone scope. Member workspace отклоняется
с `source_migration_scope_unsupported`: точное копирование его TOML потеряло бы
унаследованные настройки. Автоматическое переключение исходного проекта,
workspace migration и изменение CI approval не реализованы.

## Публикация и восстановление

Project receipt `.eska-project-layout.json` связывает оба полных before/after
inventory и hashes настроек. Новое дерево принадлежит операции; чужие файлы,
symlinks, изменённые outputs, совпадающие по регистру пути и overlap отказывают.
До записи любой из двух частей проверяются обе destinations. Они используют
прежний exclusive publisher с собственными receipts; ready markers публикуются
последними, файлы устанавливаются без замены существующих имён.

`eska.toml` публикуется после обеих частей. Пока project receipt существует,
`EskaSource` отказывает с `source_migration_incomplete`, в том числе если manifest
уже появился. Inspect/prepare и сборщик через этот вход не используют незавершённый
проект. Повтор той же команды из исходного проекта возобновляет точное matching
output; завершённый неизменённый результат возвращает `unchanged` без записи.
Вмешательство в оригинальные source/base/settings блокирует старый план.
Незавершённый собственный output сохраняется для возобновления.

Уже читаемая структура даёт layout no-op с прежним pin и оформлением; новый
destination всё равно получает полную копию. Новая миграция автоматически
пересчитывает source map и fingerprints. Гарантия транзакции при потере питания
не заявлена; файловая система должна поддерживать hard links.

## Machine interface

Tool envelope остаётся schema 1; команда — `migrate`. Данные содержат абсолютные
`root`/`destination`, target layout/schema, `previous_baseline`/`baseline`,
`layout_changed`, `written`, `publication`, `source_changes`, `accepted_changes`
и `copied_project_files`. Preview возвращает `publication = null`;
publication — `created | resumed | unchanged`. Каждый change содержит physical
path и before/after fingerprints; отсутствующая сторона означает добавление/удаление.
`ci_pin_updated` и `effective_git_policy_verified` всегда false.
Capabilities добавляет команду, `migration_layouts = ["readable"]` и
`migration_scopes = ["standalone"]`. JSON одинаков для RU/EN; human/help/errors
есть в обеих локалях. Отсутствующий pin не создаёт destination.

## Проверки

Прошли 15 root/14 SDK/5 adapter gates: fmt/check/strict Clippy/test/Rustdoc,
minimal core и отдельные CLI. Root — 443 unit + 552 integration, 24 opt-in
пропущены; minimal project CLI — 250 unit + 16 integration; SDK — 359,
adapter — 70 + 12. Семь core tests проверяют working/base separation, freeze,
exact settings, native parity, no-op/repeat, interruption, stale inputs,
foreign outputs и aliases; четыре CLI tests проверяют обе локали, preview/write,
pin/refusals, workspace refusal и блокировку incomplete project.

На полной C1 отдельный release CLI при PATH без платформы, Git и оболочки
перенёс 3 595 descriptors. Preview обеих локалей не создал destination;
publication/repeat сохранили рабочую правку Synonym и inherited freeze, новый pin
не принял эту правку в базу. Prepare обеих локалей совпал с независимым native
контролем по всем 8 778 файлам/8 077 каталогам, без exclusions. Все 3 595 YAML
прошли readable schema; old/new source/base, settings и исходный XML сохранены.

На 1С 8.3.27.2325 контроль и мигрированная копия собраны в CF и повторно
выгружены через ESKA. Все 8 769 файлов/8 064 каталога равны, кроме 7 011
`ConfigDumpInfo.configVersion`; artifact checksums, source map schema 2 и
независимый new pin проверены. Версия конфигурации не менялась.
[Обезличенные результаты](project-migration-results.json).

EF-24 остаётся `IN_PROGRESS`: structural create/delete/rename, прочие составные
домены и workspace migration ещё нужны. C2/C3, новые CFE/EPF/ERF, запуск клиента
и обновление рабочей ИБ здесь не принимались. Следующие этапы не запускаются.
