# Пределы жизни процесса анализатора

`eska-analyzer` отзывает текущее поколение host после достижения одного из
четырёх ресурсных порогов. Порог проверяется после опубликованного ответа:
принятый input сохраняет свой ACK, подтверждённый Save — свой receipt.
Следующий запрос не продолжает вычисления в отозванной сессии. Клиент завершает
и дожидается старого процесса, запускает новый и повторяет текущие черновики.

Это отдельный механизм рядом с [квотой текущих исходников](source-budget.md)
и [очисткой вычисленных кешей](analyzer-query-cache-lifecycle.md). Замена
текста освобождает прежний source reserve, но append-only native type table
и canonical names могут продолжать расти. Новая native database освобождает
свою type table; process-global canonical names живут до завершения процесса.
Поэтому одной замены базы недостаточно.

## Пороги и области счётчиков

| Option `eska-analyzer` | По умолчанию | Wire key | Область |
|---|---:|---|---|
| `--max-process-source-bytes` | `1073741824` (1 GiB) | `cumulative_ready_source_bytes` | Накопленный размер вновь принятых Ready inputs |
| `--max-process-input-publications` | `100000` | `accepted_publications` | Принятые публикации общего host |
| `--max-type-kernel-entries` | `1000000` | `type_kernel_entries` | Native type table общей database, включая sentinels |
| `--max-interner-norm-entries` | `1000000` | `interner_norm_entries` | Canonical folded names всего процесса |

Порог достигается при `counter >= limit`. Все четыре options принимают u64,
включая `0`; с нулевым пределом host отзывается на первой Ready boundary.
Process flags несовместимы с отдельным режимом `--capabilities`.

Экспериментальный Rust API `AnalysisSession::input_traffic()` возвращает
`SessionTraffic` с двумя первыми счётчиками. `cumulative_ready_source_bytes`
складывает полный UTF-8 размер каждого нового Ready YAML/BSL/resource/query
input с насыщением на `u64::MAX`. Это полный размер замены, включая идентичный
текст с новой версией, а не разница текущих резервов. Невалидная semantic model
не отменяет учёт её читаемого Ready input. Resource projection учитывает
новые bytes ресурса без повторного учёта неизменившегося owning YAML.

`accepted_publications` совпадает с общим `SessionRevision`. Принятый
unavailable input и release меняют revision и добавляют ноль Ready bytes.
Stale/unknown inputs и фактические no-op не меняют счётчики. Освобождение
текущего резерва не уменьшает накопленный трафик. Все admitted roots одной
composition используют общие ledger, database и publication clock.

Native sample возвращает точное число удерживаемых entries своей области,
не читая исходники и не вычисляя language query. Клоны database разделяют
type table. Canonical names дедуплицируются по folded spelling и переживают
замену database. Их sample использует существующую статистику interner и
обходит удерживаемые строки: стоимость O(retained strings). Два native counts
не образуют единого атомарного snapshot.

## Negotiation и wire

Прежний protocol `0.1` сохраняется. Клиент включает отдельный opt-in:

```json
{
  "initializationOptions": {
    "eska": {
      "protocolVersion": "0.1",
      "resourceRestart": true
    }
  }
}
```

Отсутствие `resourceRestart` и `false` отключают дополнительную notification,
но не enforcement настроенных пределов. Значение другого типа отклоняется
с `rpc_invalid_params` до открытия проекта. Initialize hello сообщает
`capabilities.experimental.eska.resourceRestart`; с opt-in и стандартными
порогами его значение:

```json
{
  "version": 1,
  "enabled": true,
  "scope": "process",
  "notification": "eska/restartRequired",
  "cause": "resource_limit",
  "checkBoundary": "after-published-frame",
  "overshoot": "one-operation",
  "processRssLimit": false,
  "limits": {
    "cumulative_ready_source_bytes": "1073741824",
    "accepted_publications": "100000",
    "type_kernel_entries": "1000000",
    "interner_norm_entries": "1000000"
  }
}
```

Каждый limit и counter — canonical decimal u64 string без округления JSON
numbers. `u64::MAX` передаётся как `"18446744073709551615"`.
После достижения порога согласованный клиент получает отдельный framed
`eska/restartRequired`. Условный пример с достигнутым publication limit:

```json
{
  "jsonrpc": "2.0",
  "method": "eska/restartRequired",
  "params": {
    "snapshot": {"session_id": "current-session-id", "revision": "100000"},
    "changes": [],
    "cause": "resource_limit",
    "resource_limit": {
      "schema_version": 1,
      "limits": {
        "cumulative_ready_source_bytes": "1073741824",
        "accepted_publications": "100000",
        "type_kernel_entries": "1000000",
        "interner_norm_entries": "1000000"
      },
      "counters": {
        "cumulative_ready_source_bytes": "450000000",
        "accepted_publications": "100000",
        "type_kernel_entries": "12000",
        "interner_norm_entries": "50000"
      },
      "exceeded": ["accepted_publications"],
      "checked_after": "published-frame",
      "overshoot": "one-operation",
      "process_rss_limit": false
    }
  }
}
```

`limits` совпадает с hello. `counters` содержит те же четыре ключа;
`exceeded` перечисляет все и только достигнутые пороги без дубликатов.
`snapshot` — последний успешно опубликованный snapshot этого процесса.
Resource retirement имеет отдельную причину и пустой `changes`; он не создаёт
фиктивных source-import events или receipt files.

Несогласованный клиент не получает дополнительного frame. Его следующая
проектная операция отклоняется обычным `project_restart_required`; error
details содержат тот же state с причиной и ресурсным отчётом. Для input
notification сохраняется `eska/inputRejected` с последним snapshot.
Стандартные `shutdown` и `exit` остаются доступны обоим клиентам.

## Граница публикации и восстановления

Serial worker проверяет пороги только в Ready phase, после `initialized`,
когда существует успешно отправленный snapshot. При нулевом пределе это
может быть snapshot уже отправленного initialize hello. Существующий import
restart имеет приоритет; отозванный host не генерирует второй resource restart.

Для обычного ответа сначала завершаются прежние cancellation/import fences,
затем выполняются bounded encoding, запись полного frame и acknowledgment
опубликованного snapshot. Только после этого sample может отозвать host
и отправить отдельную resource notification. Native query handles уже
уничтожены. Подтверждённый Save проходит прежний `Committed` путь и сохраняет
свой результат; ротация не разрешает автоматический повтор записи.

После retirement host очищает session, document state и прежние edit/read
authorities. Новые language requests и editor inputs не принимаются.
Global names освобождаются при завершении старого процесса, поэтому клиент
должен дождаться его выхода перед запуском следующего поколения. В новом
процессе нужны новый hello, проверка declarations/bindings и точный replay
текущего caller-owned journal с исходными text/version и новыми ACK.

Код Explorer запрашивает capability и отделяет resource retirement от import
recovery. Для неизменившихся target, admitted bindings и journal предусмотрен
один автоматический повтор: если новый процесс снова достигает лимита без
editor progress, соединение останавливается с `analyzerResourceLimit`, сохраняя
черновики. ACK/replay не считаются editor progress. Результаты клиентской
и установленной приёмки приведены [ниже](#приёмка).

## Границы гарантии

`one-operation` означает место проверки. Одна операция транспорта может
подготовить несколько inputs или выполнить большой native query и превысить
порог до публикации ответа. Sample после frame — обслуживание поколения;
он не меняет уже опубликованный результат по позднему deadline/cancellation.
До Ready phase эта policy не ограничивает попытки инициализации.

Число types/names не ограничивает размер отдельного TypeKind, строки,
immutable composition или временных аллокаций. Source traffic не является
оценкой heap. Allocator RSS и удержанные внешним SDK-потребителем `Arc`
не измеряются. `processRssLimit: false` и `process_rss_limit: false` явно
сохраняют эту границу: жёсткий RSS/OOM предел не заявлен.

Политика ротации принадлежит IDE host. `eska-check` использует ту же сессию,
профиль и source quota, выполняет один проход и завершает свой процесс;
новые process flags и IDE replay к checker не добавлены. Ledger SDK доступен
самостоятельному потребителю, но сам не завершает его процесс.
Сам wire-контракт не является приёмкой всей EF-18; итоговая сверка приведена ниже.

## Приёмка

[Машинный отчёт](analyzer-process-retirement-results.json) содержит финальные
gates и анонимные результаты. Все 20 Rust gates прошли на неизменных 2624
локальных входах: root/SDK/native fmt, check, strict Clippy, tests, SDK Rustdoc
с запрещёнными warnings и debug/release binaries. Root — 1543/1664 passed,
по 24 прежних ignored; SDK — 270/620 без ignored; native workspace — 6645
passed и 32 прежних ignored. Предварительные lint-замечания исправлены без
подавления. Explorer прошёл TypeScript check, упаковку VSIX и 454 Node tests
без skipped. Отдельный parser/lifecycle набор — 94 tests, включая поздний
malformed frame во время shutdown и сохранение подтверждённого Save без retry.

На прежнем большом CF обе release-сборки последовательно выполнили по 500
изменений поля и 512 completion requests для каждой локали. Cargo и installed
host в эти замеры не запускались; OS cache не сбрасывался.

| Сборка | Медиана update + completion, EN / RU | Peak observed RSS двух запусков |
|---|---:|---:|
| До process policy | 35,25 / 35,27 ms | 92,50 MiB |
| С process policy | 35,73 / 35,87 ms | 93,21 MiB |

Точные hashes, versions, появление нового поля, отсутствие прежнего и close
сохранены. Это наблюдение выбранных зависимостей, а не доказательство снижения
RSS или универсальная оценка задержки sampling.

Полный saved IDE diagnostic pull на RU/EN сначала опубликовал прежние 19
findings, затем отдельный resource notice при намеренном source threshold
в один байт. Последующая операция получила тот же retirement state.
На обеих локалях счётчики совпали: 242052766 Ready bytes, 5845 publications,
214 native types и 1439 canonical names. Все значения ниже стандартных
порогов: полный наблюдаемый load не требует их повышения. Запрос занял
14,36–14,37 s, peak observed RSS — около 400 MiB.

Свежий standalone checker той же release-сборки и saved snapshot обработал
1992/1992 BSL modules на RU/EN, без unavailable или unchecked modules.
Для выбранного IDE-модуля совпали source/settings hashes, codes, severity
и byte spans. Exit 2 и `metadata_models_complete=false`,
`semantic_profile_complete=false` сохранены. Peak observed RSS checker —
около 1022 MiB; его проход мог пересекаться с функциональной installed-приёмкой.

Установленный VSIX прошёл CF/CFE/EPF/ERF в отдельном профиле редактора:
обычные правки YAML/BSL/SDBL, ACK перед notice, завершение старого процесса,
новый session id, точный replay text/version/hash/UTF-8 bytes и новые ACK.
Актуальные diagnostics после replay используют текущие inputs. Publication
limit 1 останавливает неизменившийся working set после одной автоматической
замены с локализованным `analyzerResourceLimit`; черновики остаются dirty.
Ручное подключение после увеличения лимита восстанавливается. Эти installed
случаи проверили английский UI; RU/EN protocol/help и сообщения покрыты
автоматическими проверками. Публикация Save здесь не выполнялась:
её подтверждение перед retirement отдельно проверено SDK/client regressions.

Все 29 процессов принимаемых runtime-прогонов завершены естественно;
identity guards не отправляли сигналы. Первая непринятая попытка измерителя
встретила временно пустой `/proc` argv и завершила только свой pinned child.
Первый installed harness ожидал CRLF в LF-документе; ожидание исправлено
согласно обычной нормализации EOL редактором. Обе попытки сохранены отдельно
и не входят в принятые результаты.

Большой исходный проект сохранил все 22117 файлов и 6327 каталогов, их bytes,
modes и mtimes. Копий большого проекта, вызовов платформы и новых
информационных баз нет. Финальный scanner подтвердил четыре сохранённые базы
playground и отсутствие лишних.

## Итог EF-18

EF-18 принята по её Definition of Done: единая сессия обслуживает текущие
YAML/BSL и объявленные queries, дерево/свойства и языковые запросы; общие
revision, cancellation/deadline и ресурсная policy сохранены. Saved IDE/checker
используют один профиль и ядро, показывают неполноту и ошибки явно.
Прежние versioned property plans и
[защищённый Save](guarded-source-save.md) проверяют текущую входную версию.
Новый resource lifecycle сохраняет эти границы при смене поколения.

Пределы выше не превращаются в hard RSS/OOM guarantee или полную семантику.
Следующая текущая задача — EF-27: зависимости полей и параметров запросов,
сохранённые настройки СКД и связанные потребители. Затем остаются EF-11,
EF-21 и общая приёмка EF-16; MVP целиком ещё не завершён.
