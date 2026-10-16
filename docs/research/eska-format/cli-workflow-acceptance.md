# Приёмка файлового CLI-цикла ESKA

Итоговая сверка EF-20 с [формальным DoD](../../roadmap/15-eska-format-experiments.md#ef-20-сквозной-cli-newinit-импорт-и-сборка).
Проверенный профиль: Linux, 1С 8.3.27.2325, CF/CFE/EPF/ERF, существующий Queries
layout 5/6. Эта сводка переиспользует сохранённые receipts и текущие тесты;
новые платформенные или продуктовые прогоны для неё не выполнялись.

## Существующие native receipts

| Срез | Принятое покрытие | Результаты |
|---|---|---|
| Main new | 8 случаев: пустое создание и создание из артефакта для каждого из четырёх типов; main build и независимый unpack | [Контракт](shell-project-creation.md), [receipt](shell-project-creation-results.json) |
| Main init | 4 native-origin проекта: существующие Git/user bytes, source/base/published history и local BSL сохранены; build/dump совпали с контролем | [Контракт](project-initialization.md), [receipt](project-initialization-results.json) |
| Main import | 4 случая: preview, exact-plan write, confirmed no-op repeat, build и независимый dump под прежними pins | [Контракт](shell-source-import.md), [receipt](shell-source-import-results.json) |
| Main build | 4 native случая: protected source, local BSL, прежние settings/pin и independent dump | [Контракт](shell-build-delegation.md), [receipt](shell-build-delegation-results.json) |
| Installed workspace | 20 случаев: all4 addition/deletion/rename/Complete/RollBack; 63 Save, 24 orphan drafts, изоляция четырёх соседних members | [Контракт](installed-native-workspace.md), [receipt](installed-native-workspace-results.json) |

Workspace receipt проверяет выбранного member и клиентскую координацию; он не
выдаётся за 20 новых native build/dump циклов. Standalone и workspace proofs
дополняют друг друга. Полное произведение типов, локалей, режимов и отказов
этими результатами не заявляется и формальным DoD отдельно не требуется.
`status: partial` в старом build receipt относится к одиночному facade-срезу;
поздние new/init/import и workspace proofs перечислены отдельно. Это история
покрытия того среза, а не незакрытый контракт общей текущей сверки.

## Сверка DoD

| Условие | Проверенное доказательство |
|---|---|
| Пустое создание и создание из артефакта каждого типа | Восемь actual циклов [main new](shell-project-creation.md); прямой `new` проверяет inspect/prepare под исходным pin в [project_tool/new.rs](../../../tests/project_tool/new.rs) |
| Init каждого типа без потери существующих данных | Четыре actual [main init](project-initialization.md); all4/RU/EN и точные source/base/ancestor bytes в [init_eska.rs](../../../tests/cli/init_eska.rs) |
| Saved edits, повторный импорт, сборка и контрольная выгрузка | All4 [shell import receipt](shell-source-import-results.json) сохраняет local BSL, settings/base/history, выполняет unchanged repeat и independent native dump |
| Standalone и выбранный workspace member | Native standalone receipts выше; [workspace 20 cases](installed-native-workspace.md), new inheritance в [new_eska.rs](../../../tests/cli/new_eska.rs) и init inheritance/override в [workspace.rs](../../../tests/cli/init_eska/workspace.rs) |
| RU/EN, human/JSON и независимость machine output от locale | All4 [init_eska.rs](../../../tests/cli/init_eska.rs) сравнивает RU/EN JSON и human output; [actual build delegation](../../../tests/cli/build/delegation/actual.rs) проверяет оба языка и формата |
| No-TTY и явные обязательные choices | `preflight_refuses_incompatible_flags_and_nonterminal_policy` в [new_eska.rs](../../../tests/cli/new_eska.rs); `shell_preflight_precedes_capabilities` в [selection.rs](../../../tests/cli/build/delegation/selection.rs): отказ до companion/publication |
| Явная платформа, наследование и override | Actual [new](shell-project-creation.md) использует заданную версию; [workspace receipt](installed-native-workspace-results.json) подтверждает inheritance без CLI override; [build_tool.rs](../../../tests/build_tool.rs) сохраняет settings при explicit override |
| Отмена без частичного успешного результата | SIGINT native export в [source_import.rs](../../../tests/build_tool/source_import.rs) сохраняет source без receipt/guard; SIGTERM unpack в [unpack.rs](../../../tests/build_tool/unpack.rs) сохраняет input без output |
| Занятое назначение | [project_tool/new.rs](../../../tests/project_tool/new.rs) проверяет `source_destination_exists` и неизменный sentinel; [unpack.rs](../../../tests/build_tool/unpack.rs) отказывает для existing file/directory/symlink до платформы |
| Несовместимая версия инструмента | [New probe](../../../tests/cli/new_eska/guarded.rs) не создаёт member/staging; [build handshakes](../../../tests/cli/build/delegation/handshakes.rs) проверяют version/protocol/tool/profile и отсутствие запуска build |
| Direct/facade arguments, streams, exit codes и ограничения | [Build delegation](../../../tests/cli/build/delegation.rs) проверяет точные streams/argv, exit 23 и signal status 143; [main import](shell-source-import.md) сохраняет child streams/exit; [new](shell-project-creation.md) и [init](project-initialization.md) проверяют typed receipt перед собственной compound finalization |
| Сбой не объявляет частичный проект или artifact успешным | [New guarded cases](../../../tests/cli/new_eska/guarded.rs), [init guarded cases](../../../tests/cli/init_eska/guarded.rs); `platform_diagnostics_keep_origins_and_failed_build_preserves_previous_pair` в [build_tool.rs](../../../tests/build_tool.rs) сохраняет прежние artifact/manifest |
| Восстановление описано и проверено | [Core journal/recovery](source-import-recovery.md), [actual standalone recovery](installed-native-recovery.md) и [selected workspace recovery](installed-native-workspace.md): independently retained transaction, обе actions и unchanged repeat |
| Один редактируемый source; XML поколений производный | [Protected facade build](shell-build-delegation.md) использует отдельное поколение; [native receipt](shell-build-delegation-results.json) сохраняет saved source, accepted bytes, pins и settings для all4 |
| Clean checkout сохраняет source, обратную карту и принятую базу | [Initial ancestry transport](initial-import-ancestor.md) и настоящий standalone/member `clone --no-local` в [transport/tests/ancestry.rs](../../../src/project_core/transport/tests/ancestry.rs) сравнивают source/base/raw history и protected XML |
| Неверные pin/bytes дают явный отказ | [Project CLI refusals](../../../tests/project_tool.rs) проверяют `source_baseline_untrusted` без XML fallback; [baseline tests](../../../tests/project/source_format/baseline.rs) проверяют missing/corrupt blob, manifest и header |
| Transport не отложен до EF-19 | [Scoped transport и clean clone](initial-import-ancestor.md) уже существуют независимо от будущего VCS |
| Совместные check/IDE относятся к следующим этапам | Это явная граница [DoD EF-20](../../roadmap/15-eska-format-experiments.md#ef-20-сквозной-cli-newinit-импорт-и-сборка); [shell check](shell-check-delegation.md) уже подключён, полнота анализа и установленный workflow принимаются отдельно |

Связанный контракт миграции также имеет доказательство: [Queries migration](query-project-migration.md)
проверяет all4 history-preserving копии, mixed workspace и native переходы
Readable CF/CFE в Queries. Layout change с несовместимой raw history даёт явный
`source_migration_ancestry_unsupported` до output; история не пересчитывается молча.

## Актуальность проверок и границы

Последний [root receipt](borrowed-native-reimport-results.json) подтверждает
fmt/check/strict Clippy, 1543 default и 1664 all-features tests: failed нет,
в каждом полном прогоне 24 ранее ignored. Из 1694 frozen Rust inputs после
предыдущей root/SDK-приёмки изменён только borrowed test-файл; production прежний.
Эти root gates выполнены ранее; текущая документальная сверка их не повторяет.
SDK/client gates и native receipts переиспользованы в пределах их записанных
профилей, а не объявлены новыми прогонами.

Источник, independently supplied approval pin и отдельно подтверждённый ancestor
не подменяют друг друга. Creation receipt возвращает candidate approval reference;
это не неявное принятие поддержки; reimport сохраняет прежний independent pin.
При clone проверены exact authorities,
при build/import сохранены working bytes и прежняя база — [initial ancestor](initial-import-ancestor.md).

Physical terminal Ctrl+C родителя и Windows execution не приняты; проверенные
Unix signal cases перечислены выше. Aggregate ESKA build остаётся явным отказом;
workspace приёмка относится к выбранному member — [build contract](shell-build-delegation.md).
Новое borrowed CFE представление проверено core-тестом, без заявления runtime
applicability к реальной базе — [граница borrowed reimport](borrowed-native-reimport.md).

## Итог

EF-20 — `DONE` по формальному DoD в описанном проверенном профиле:
подтверждённых непокрытых контрактов эта сверка не выявила.
Общая семантика EF-18/EF-27, полная IDE-приёмка EF-11,
обычная поставка EF-21 и совместная приёмка EF-16/MVP остаются отдельными этапами —
[действующая очередь](../../roadmap/README.md#ближайший-порядок-выполнения).
