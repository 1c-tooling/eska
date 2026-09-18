# Deferred и требующие спецификации возможности

Эти пункты присутствуют в продуктовой спецификации, но не входят в основную
очередь задач либо намеренно отложены. Перед реализацией каждый
превращается в обычную
задачу с зависимостями и Definition of Done.

| Возможность | Статус | Когда возвращаться |
|---|---|---|
| Custom/company templates | DEFERRED | После стабильных T04–T06 |
| Remote template registry | DEFERRED | После реального use case |
| Direct-trunk workflow | DEFERRED | После T09 и пользовательской проверки |
| Hotfix/release Git Flow | DEFERRED | После T10 и основного VCS UX |
| MR/PR provider integration | DEFERRED | После T38 и стабильного publish |
| Workspace per task / Git worktree + isolated infobase | DEFERRED | После T30–T31 |
| `hooks install` / `prepare-commit-msg` | DEFERRED | После T22 |
| AI refinement commit messages | DEFERRED | После deterministic T22 |
| `eska setup` onboarding | NEEDS-SPEC | После doctor + environments + build |
| Завершение задачи после squash/rebase | NEEDS-SPEC | Приоритет и контракт в [T53](12-current-functionality.md); уточнение до изменения T40 |
| Реализация самостоятельной команды `eska test` | DEFERRED | Отдельная задача после спецификации T23; поставить перед T33, если backend выбран |
| Структурное редактирование Designer XML: add/remove/rename | DEFERRED | [T74](13-metadata-and-ide.md), после стабильного read-only API и T73; требует спецификации согласованной записи файлов |
| Фоновый prefetch метаданных | DEFERRED | Отдельная оптимизация по замерам T68; не включает необходимую индексацию поиска T75 |
| EDT / `1cedtcli` source format | DEFERRED | После зрелой Designer XML модели |
| Standalone GUI | DEFERRED | После VS Code и стабильного protocol |
| Другие GUI/IDE frontends | DEFERRED | После T41 |
| Plugin system | DEFERRED | Только при двух реальных implementations/use cases |
| Сторонние translation packs | DEFERRED | После реальной потребности |

Для всех сложных multi-state операций следует оценивать единообразный `--dry-run`.

EDT, AI, MCP, собственный BSL LSP и визуальный редактор форм не входят в
первую metadata/IDE-поставку T60–T70, T75 и T76–T81. Наличие других идей в общем
backlog не расширяет её scope. Backend read-only protocol выполняется в T69–T70;
клиент реализуется в [T76–T81](14-vscode-extension.md), закрывающих T41.
Редактирование T71–T73 остаётся отдельным последующим этапом.
