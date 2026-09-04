# Deferred и требующие спецификации возможности

Эти пункты присутствуют в продуктовой спецификации, но не входят в ближайшие 41
задач либо намеренно отложены. Перед реализацией каждый превращается в обычную
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
| Реализация самостоятельной команды `eska test` | DEFERRED | Отдельная задача после спецификации T23; поставить перед T33, если backend выбран |
| EDT / `1cedtcli` source format | DEFERRED | После зрелой Designer XML модели |
| Standalone GUI | DEFERRED | После VS Code и стабильного protocol |
| Другие GUI/IDE frontends | DEFERRED | После T41 |
| Plugin system | DEFERRED | Только при двух реальных implementations/use cases |
| Сторонние translation packs | DEFERRED | После реальной потребности |

Для всех сложных multi-state операций следует оценивать единообразный `--dry-run`.
