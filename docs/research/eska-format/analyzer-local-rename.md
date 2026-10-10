# Планы переименования локальных BSL-символов

`eska-analyzer` обслуживает стандартные `textDocument/prepareRename` и
`textDocument/rename` для параметров, явно объявленных и неявных локальных
переменных одного метода открытого BSL-модуля. Native binding и поиск
использований принадлежат тому же закреплённому ядру; сервер возвращает
версионированный план, а клиент решает, когда применить его через didChange.
Сервер не записывает BSL и не вызывает workspace/applyEdit.

## Допуск и права

Core создаёт `LocalRenameInput` только из current overlay без ошибок native
parser. Закрытый модуль, module variables, методы, top-level implicit locals,
platform/member symbols и неподдержанный binding не получают план.
`ReferenceScope::FileLocal` дополняется проверкой принадлежности методу:
приватный метод или модульная переменная также могут иметь file-local scope.
Revision, hash, editor version и текущий anchor проверяются повторно перед
авторизацией; retained input не допускает другой снимок или текст другого host.

Оба запроса требуют независимо закреплённый accepted pin и поддержку клиентом
`workspace.workspaceEdit.documentChanges=true`. Общий `Authority` используется
также [планами свойств YAML](analyzer-property-plans.md). Выбор capability не
читает базу; каждый preview проверяет полную принятую базу с общим контролем
отмены и времени. Адресная verification и кеш полномочий не добавлены.

`AcceptedBase::module_permission()` определяет UUID по принятым paths и составу,
а не по рабочему YAML. Двоичный, отсутствующий в базе или неизвестный модуль
отклоняется. Вложенный inline CommandModule принадлежит UUID команды, даже если
описание хранится в YAML справочника. Этот же resolver используется защищённым
экспортом: собственная заморозка команды больше не теряется из-за выбора UUID
родителя. Supplier restrictions и freeze остаются частью существующей policy.

Неполный общий проект не запрещает подтверждённую локальную область метода,
но coverage сохраняется отдельно: `semantic_complete=false` не повышается.
План требует ровно один scanned file без files_capped/hits_capped. Все native
occurrences должны принадлежать текущему документу, совпадать с hash/именем,
иметь корректные UTF-8 границы, не пересекаться и содержать выбранный anchor.

## Протокол и конфликт имён

Контракт определяется [LSP rename/prepareRename](https://raw.githubusercontent.com/microsoft/language-server-protocol/gh-pages/_specifications/lsp/3.18/language/rename.md).
Оба запроса принимают textDocument URI, UTF-16 position, необязательные
workDoneToken и `eska.snapshot`; rename требует newName. PartialResultToken
не принимается, progress/streaming не отправляются. При prepareSupport=true
provider имеет `{prepareProvider:true,workDoneProgress:false}`, иначе boolean
true. Повреждённые настройки запрещают capability и запросы.

`experimental.eska.localRename` объявляет scope `method-local-bsl`,
openDocumentsOnly, versioned и acceptedBaseRequired=true,
conflictCheck `current-module-names`, incompleteResults `error`,
maxOccurrences=10000 и maxNameBytes=1024. Без accepted pin или documentChanges
provider отсутствует, запрос получает rename_plans_disabled.

Prepare возвращает range текущего occurrence и placeholder; неподходящий
неименной token даёт null. Rename возвращает один `TextDocumentEdit` внутри
documentChanges с обязательной editor version и непересекающимися TextEdits.
Unversioned fallback отсутствует. Контекст `eska` сохраняет snapshot, source/hash,
document_version, profile `local-rename`, scope/scope_verified, coverage,
resources и типизированный permission с baseline. Только rename добавляет
reference_search_complete, число изменённых occurrences и unchanged.

NewName проверяется тем же native parser в declaration/read/assignment contexts,
без второй грамматики или списка reserved words. Непустой одиночный identifier
не может добавить аргумент, default, statement или комментарий. Лимит 1024 bytes
ограничивает размер preview, а не объявляет ограничение платформы 1С.
Проверка выполняется до открытия базы и повторяется в core.

Новое отличающееся имя отклоняется при совпадении без учёта регистра с любым
native name token текущего модуля. Это консервативная защита от capture:
она может отказать из-за имени в независимой области. Case-only rename сохраняет
binding; occurrences с уже точным newName не включаются. Настоящий no-op
возвращает пустой edits и unchanged=true. Комментарии, строки, текст запросов
и динамически исполняемый код не переписываются.

Девять languageMethods и десять cancellableMethods используют одну очередь,
worker, lifetime ID и RequestCancellation. Cancellation — `-32800`, deadline —
`-32000` без retriggerRequest; нулевой timeout отказывает до IO. Кооперативная
отмена не гарантирует задержку или RSS. 10000 ограничивает проекцию после native
collection, как в [references](analyzer-references.md); жёсткого CPU/heap/RSS
budget ещё нет. Native ограничение line comments при одиночном CR сохраняется;
bytes не нормализуются скрыто. Core API `LocalRenameInput/Target/Plan/Error`
экспериментальный, compatibility layer не создаётся.

## Проверки

Шесть новых core и десять process tests проверяют native scope трёх видов locals,
version/hash, UTF-16/BOM/non-BMP/LF/CRLF/CR, capture, case-only/no-op, native grammar
и размер имени, stale/pending/closed/invalid inputs, pinned policy и повреждённую
базу, capability negotiation, настройки, отмену и восстановление сессии в RU/EN.
Inline-command test подтверждает одинаковый запрет preview и защищённого
экспорта с восстановлением принятых bytes. Дополнительный root test проверяет
accepted ownership, unsupported payloads и невозможность сменить права рабочим UUID.

All-features: 106 unit/file, 116 analyzer и 11 checker process tests, всего 233.
Minimal — 84; checker — 86 unit/file и 11 process; analyzer — 106 unit/file
и 115 process. Все 19 tool gates прошли, включая strict Clippy, Rustdoc с warnings
denied в четырёх profiles и обе release-сборки. Все 15 root gates прошли:
895 passed / 14 прежних ignored; minimal — 165/3, project-cli — 182/3,
build-cli — 201/3. Diff проверен с учётом CRLF.

Parser получил прямого потребителя для newName, но его версия уже входила
транзитивно в граф. Normal graph обоих инструментов прежний: 206 имён/214 версий.
LSP runtime, Tokio, reqwest и Git transport не добавлены. Release analyzer —
50068776 bytes; checker — 46269608 bytes. Human strings не добавлены;
machine responses и traces совпадают в RU/EN.

[Обезличенные измерения](analyzer-local-rename-results.json) фиксируют 24 сценария
C1/C2/C3: small/medium настоящий CommonModule, оба prepareSupport и RU/EN.
Для каждого выбранного модуля создана собственная полная минимальная композиция
с настоящим владельцем/BSL и собственной принятой базой. Это не проверка policy
или памяти всей исходной конфигурации. 360 pulls дали 96 планов с 228 правками,
24 no-op, 24 stale refusals и 96 отказов закрытого документа/имени.
Настоящий параметр изменён только в собственном overlay; дополнительные locals
и одноимённый параметр другого метода добавлены только для проверки scope.
Client-applied планы повторно проверены синтаксически в той же сессии.

15 выбранных оригиналов и BSL bytes шести копий сохранены, временные проекты
удалены. Процессы завершились штатно при открытом stdin и пустом PATH.
Workspace MCP недоступен; установленный bsl-analyzer-app 0.3.0 дополнительно
проверил SyntaxError на шести собственных изменённых BSL-копиях, exit 0.
VmHWM — 51192–52808 KiB; query time — 0,0001574–0,0383632 секунды.
Эти значения относятся к минимальным композициям, не к полным конфигурациям.
Платформа 1С и настоящий IDE-клиент в этой части не проверялись.
Планы вне метода, полные модели/семантика, общий memory budget и Explorer
остаются в работе; EF-18 — `IN_PROGRESS`.
