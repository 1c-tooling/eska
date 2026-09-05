# Сквозные правила реализации

Эти требования применяются к каждой задаче и не должны дублироваться во всех
файлах этапов.

## Архитектура

- Core не зависит от `clap`, TTY и локализованных строк.
- CLI отвечает за parsing, presentation, exit codes и выбор human/JSON output.
- Ошибки core структурированы; их человекочитаемое представление локализуется на
  presentation layer.
- Не добавлять traits, generics, plugins, caches и dependencies до появления
  реального потребителя.
- Designer XML — единственный source format до отдельной задачи.
- Machine-facing значения (команды, options, config/JSON keys, enum serialization,
  error codes) всегда английские и не зависят от locale.
- Для команд, полезных CI/IDE, проектировать стабильный `--format human|json`.

## Безопасность и производительность

- Любая потенциально разрушающая операция имеет preflight, понятный preview и
  явный `--force`, если безопасной альтернативы нет.
- Не использовать `reset --hard` как универсальный механизм.
- Не привязывать `status`, `diff` и `save` к полному parse конфигурации.
- Сначала рассматривать changed paths и инкрементальную обработку; cache добавлять
  только по измеренной необходимости.
- Для каждой VCS-операции сначала проверяется возможность сохранить контракт и
  safety guarantees через текущую закреплённую версию `gix`.
- System Git допустим только как capability fallback через один infrastructure
  layer: для отсутствующей высокоуровневой orchestration worktree/index,
  hooks/editor/signing, LFS или конкретно неподдержанного transport/credential
  сценария. Fallback не является безусловным повтором после любой ошибки `gix`.
- Не использовать shell parsing и не разбирать локализованный human output Git;
  оставшиеся system Git-вызовы перечислять и обосновывать в решениях задачи.

## Definition of Done

Для каждой implementation task обязательно:

1. сверить задачу с фактической архитектурой;
2. реализовать только заявленный scope;
3. добавить human strings одновременно в `ru-RU` и `en-US`;
4. покрыть locale-independent core unit-тестами;
5. покрыть CLI exit code/stdout/stderr integration-тестами;
6. для JSON проверить стабильную схему и независимость от locale;
7. для Git использовать маленькие настоящие временные repositories, когда это
   надёжнее mock;
8. выполнить `cargo fmt --check`, `cargo check`,
   `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`;
9. вручную проверить затронутые CLI-сценарии на обоих языках;
10. удалить ставшие ненужными код и dependencies;
11. обновить статусы и решения в этом трекере;
12. не менять версию crate вручную — ей управляет release automation.
