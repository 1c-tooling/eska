# Repository и workflow policy

## T07 — Repository layer

**Статус:** `PLANNED`  
**Зависит от:** T03

Создать единый infrastructure layer. Через `gix` реализовать discovery, HEAD,
branch/refs, worktree/index status, changed paths, базовую историю и merge-base по
мере реальной необходимости. System Git оставить fallback для network/mutating,
credentials, signing и LFS операций. Не размазывать `Command::new("git")` по CLI.

**Готово, когда:** реальные временные repositories покрывают detached HEAD,
unborn branch, dirty state и простой commit graph; core возвращает структурированные
данные, а не Git human output.

## T08 — Workflow policy model

**Статус:** `PLANNED`  
**Зависит от:** T02, T07

Модель определяет base branch, working branch policy, task naming, sync strategy,
integration target, publish и finish behavior. `custom` переопределяет preset через
config, а не через новый Rust enum на каждую компанию.

## T09 — Trunk preset

**Статус:** `PLANNED`  
**Зависит от:** T08

Short-lived task branch от `main`, rebase на `origin/main`, publish task branch,
integration через MR. Direct-trunk оставить отложенным вариантом.

## T10 — Git Flow preset

**Статус:** `PLANNED`  
**Зависит от:** T08

`feature/*` от `develop`; заложить policy slots для будущих `release/*` и
`hotfix/*` от `main`, не реализуя весь release flow заранее.

## T11 — GitHub Flow preset

**Статус:** `PLANNED`  
**Зависит от:** T08

Short-lived branch от `main`, публикация ветки и PR/MR integration policy.

**Готовность workflow:** одинаковые входные данные дают детерминированный plan;
presets и custom overrides покрыты table-driven tests, пока без выполнения команд
повседневного UX.

