# Локальный синтаксис BSL четырёх типов

`eska-check --profile syntax` и `eska-analyzer --profile syntax` принимают
CF/CFE/EPF/ERF ESKA sources. CF использует прежний canonical configuration scope;
для трёх остальных типов явно выбирается LocalSyntax. Нового сервера, XML
fallback или фиктивной Configuration для внешнего объекта нет.

## Scope и lifecycle

`AnalysisScope::{CanonicalConfiguration, LocalSyntax}` имеет явный constructor
`from_reader_with_scope`; прежние constructors сохраняют canonical admission.
LocalSyntax регистрирует declared YAML/resources/BSL через прежнюю сессию,
FileId, revisions, byte budgets, Documents, saved invalidation и import guard.
Canonical slots/config roots остаются пустыми, modeled descriptors — ноль.
Готовые source bytes имеют `ModelState::SourceOnly`, а complete semantic model
не объявляется.

Для BSL доступны parse diagnostics и document symbols. Semantic requests
возвращают `analysis_context_required` до lazy reads. В LSP не объявляются
completion/hover/definition/references/signatureHelp/rename/editPlans для LocalSyntax;
прямые вызовы тоже отказывают. Metadata read/sync и текущие source documents
сохраняются. CF capabilities и semantic guards остаются прежними.

Dirty overlays остаются у клиента и имеют прежний version/snapshot protocol.
Watch сохранённого файла не подменяет открытый unsaved текст. Close возвращает
saved authority. Live import освобождает старый host с последним опубликованным
snapshot, включая отказ позднего ответа и неподтверждённой document version.

## Machine interface и полнота

Report добавляет `analysis_scope`, initialize —
`experimental.eska.analysisScope`. Diagnostic context содержит
`eska.analysis_scope`, document symbol context — `analysis_scope`.
Новый model state сериализуется как `source_only`.

`syntax_complete = true` означает проверку всех объявленных читаемых BSL модулей.
LocalSyntax сохраняет `metadata_models_complete = false`, status incomplete и
exit 2, даже если parse findings отсутствуют. Найденные ParseError остаются
в files/findings. Неподдержанный semantic profile даёт прежний exit 3;
он не подменяется syntax молча.

В первоначальной матрице CF Queries syntax был complete, но Language и Template
давали metadata_projection_unsupported и общий exit 2. Последующая
[структурная проверка](structural-metadata-coverage.md) закрывает эти descriptor
gaps без DCS/query модели. Полноценные CFE/base и external contexts, а также
`.sdbl` context analysis ещё нужны для MVP.

## Проверки

11 новых tests (4 core, 5 framed analyzer, 2 checker) и 3 прежних semantic refusal
cases прошли адресно; strict SDK Clippy, nofeatures и scoped fmt прошли.
Проверены all4/RU/EN, checker/LSP SHA/code/byte/UTF-16 parity, BOM/CRLF/Cyrillic,
open/change/watch/close, stale versions, binary/budget/deadline/cancellation,
import retirement и отсутствие canonical metadata loading/resolution.

Восемь framed runtime cases на четырёх настоящих native-origin проектах
подтвердили caller-owned dirty text, repair/symbols, saved invalidation и
checker/LSP ParseError codes/byte ranges. Независимый BSL Analyzer CLI 0.3.2
завершил все четыре source дерева без failed files: коды ParseError и UTF-8 byte
диапазоны совпали с SDK. Analyzer не загружал metadata context из ESKA YAML и
использовал built-in interface facts без HBK; прочие diagnostics не являются
проверкой семантической эквивалентности.

[Runtime результаты](analyzer-local-syntax-results.json) сохраняют scope,
полноту и обе проверки без локальных путей. Native build/execution и установленный
IDE-client этой синтаксической частью не принимаются. Все 22 Rust gates прошли: root 860 unit / 632 all-feature integration,
SDK 191 unit / 166 analyzer / 21 checker tests; 24 opt-in cases остаются ignored.
