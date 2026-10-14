# CLI повторного нативного импорта

EF-25 подключает существующие native staging, project plan и guarded journal
к самостоятельным инструментам. `eska-build import` распаковывает CF/CFE/EPF/ERF
и передаёт сравнение, права и публикацию project core. `eska-project` сохраняет
явно указанный предок и восстанавливает прерванный импорт без платформы или Git.
Общий `eska import` и координация IDE ещё не подключены.

## Предок и preview

Для первого импорта требуется подтверждённый общий снимок исходников ESKA.
Его нельзя получать из уже изменённой рабочей копии по умолчанию:

```sh
eska-project capture-ancestor pristine/src confirmed-ancestor --format json
```

Команда создаёт исключительно новый store и возвращает `data.ancestor.sha256`.
Caller отдельно подтверждает, что это общий предок. Ключ не принимает поддержку.
Начальные new/init будут сохранять эту связь через EF-20; текущая команда не
угадывает историю существующего проекта.

```sh
eska-build --project-dir project import incoming.cf \
  --accepted-base-sha256 <approved-pin> \
  --ancestor confirmed-ancestor --ancestor-sha256 <ancestor-pin> \
  --platform-version 8.3.27.2325 --format json
```

Без `--write` исходники и project history не публикуются. Собственный staging
распаковки удаляется после операции. Preview содержит тип/member, обе authority,
входящий/resulting ancestor, working/output revisions, exact file changes,
конфликты, `can_apply`, `unchanged` и `transaction.sha256` плана. Raw platform
streams остаются отдельными JSON byte events. Human output перечисляет пути
и конфликты на RU/EN. Ключи, категории и JSON envelope 1 независимы от locale.

`--project <name>` выбирает ровно один workspace member через общий selector.
Платформа берётся из resolved settings либо из явного override; ibcmd/архитектура/
Distrobox используют прежний machine layer. Пути input, ancestor и receipt
относительны cwd; `--project-dir` задаёт discovery, не меняет cwd.

## Применение и повтор

```sh
eska-build --project-dir project import incoming.cf \
  --accepted-base-sha256 <approved-pin> \
  --ancestor confirmed-ancestor --ancestor-sha256 <ancestor-pin> \
  --platform-version 8.3.27.2325 \
  --expected-plan-sha256 <preview-transaction-pin> \
  --write --receipt operation-recovery.json --format json
```

`--expected-plan-sha256` необязателен; если задан, заново проверенный план обязан
совпасть с preview до записи. Применение всегда повторно проверяет inputs.
Не разрешённые конфликты и новые правила поставщика не выбираются автоматически.
Неверные type/UUID/member, ancestry/support pin или receipt destination отказывают.
Freeze и поддержка остаются прежними; XML из staging не заменяет рабочий source.

Для write требуется явно выбранный новый recovery файл вне source, принятой
базы, ancestry store и project journal. Ключ публикуется атомарно/exclusive до
появления guard и source moves. Файл содержит transaction, approval, ancestor,
incoming/resulting ancestry и revisions. Существующие files/symlinks не затираются.
При отказе после этой границы сохранённый ключ используется для recovery;
JSON failure также содержит preview/reference и путь к receipt.

После применения caller отдельно сохраняет `data.resulting_ancestor.sha256`.
Следующий вызов передаёт его через `--ancestor-sha256` без `--ancestor`: store
открывается из подтверждённой истории выбранного проекта. Повтор одинакового
native input возвращает `publication = unchanged` без нового journal, source
или recovery файла. `transaction` такого preview является digest плана,
не ссылкой на новый опубликованный journal. Initial no-op всё же сохраняет
первую подтверждённую ancestry, как требует core.

## Восстановление

```sh
eska-project --project-dir project recover-import \
  --receipt-sha256 <retained-transaction-pin> --action complete --format json
```

`--action roll-back` возвращает прежние source/state. Root указывается точно;
для member это его каталог, не workspace root. Команда намеренно не требует
успешного discovery, заблокированного guard. Core проверяет digest, settings,
accepted bytes и все source/journal trees перед записью. Чужие или изменённые
данные блокируют обе actions. Повтор завершённой action — no-op.

## Проверки и границы

Process tests проверяют четыре native profiles, обе локали, preview/write,
точный plan digest, сохранённые local YAML annotations, оригинальный pin,
подтверждённый repeat, отдельный member, конфликты/freeze, wrong type/UUID,
platform failure, receipt collisions/unsafe paths и SIGINT native export.
Offline процессы проверяют capture и обе recovery actions настоящего прерванного
journal после удаления первоначального incoming store. Исправлена выявленная
регрессия merge: annotation-only working edit больше не заменяет весь mapping;
incoming scalar выбирается адресно, working comments сохраняются.

Все 22 Rust gates прошли: root 808 unit/600 integration, all-features также
27 build/52 project process tests, SDK 185/158/19, adapter 70 + 12, minimal
build/core/CLI и Rustdoc. 24 opt-in root случая пропущены. Семь выбранных
BSL модулей проверены CLI fallback 0.3.2: complete, failed files 0, SyntaxError 0;
остальные style/context findings 0/5/51/2. Workspace MCP недоступен,
bsl-analyzer.toml в собственных проектах опыта отсутствует; использованы defaults.

Четыре настоящих inputs прошли CLI capture, preview, write с exact plan pin,
confirmed repeat без нового receipt, build и независимый dump на 1С 8.3.27.2325.
17/111/14/8 файлов равны по составу/XML semantics при исключении служебных
CDF configVersion; opaque bytes совпали, BSL отличается только ожидаемым
local comment и разделяющими переводами строк. Прежний pin, local BSL и
settings сохранены. [Обезличенные результаты](native-reimport-cli-results.json).
Проверена Linux; исполнение новых сценариев клиентом и production проекты
этой частью не принимаются.

IDE refresh, защита несохранённых вкладок при замене состава, общий new/init/import
и окончательная приёмка EF-25/EF-16 ещё нужны. Эта часть не объявляет MVP готовым.
