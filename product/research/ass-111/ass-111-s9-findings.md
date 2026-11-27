# FINDINGS: S9 — Audit trustworthiness (fire-and-forget audit writes)

**Spike**: ass-111 S9 (add-on, GH #1050)
**Date**: 2026-11-27
**Approach**: evaluation + measurement
**Confidence**: directional overall. The loss-path inventory and the per-write cost are **empirical**: they come from code tracing with file:line citations, a live-store audit, live-log counts and a scratch benchmark.
**Agent**: researcher-ass-111-s9

**SECURITY NOTE**: The repo and #1050 are public. The body of this file describes defects in full. The **Recommendations Summary** names no exploitable specifics and refers to inventory rows only. Keep tool-name-plus-mechanism detail out of public posts until the Stage 1 fixes land (same rule as S7 R27).

---

## Findings

### Q: "Inventory every audit write path (crates/unimatrix-server/src/infra/audit.rs and all callers, incl. background/tick, UDS, export/import): how each can lose, reorder, or misattribute rows (async spawn, swallowed errors, outside the state-change transaction, shutdown/OOM/container stop), and which state changes emit no audit row."

**Answer**:
- No V1 audit write runs inside the state-change transaction. Every class-A mutation commits first. The audit row is then written later by a detached task in its own transaction.
- The errors are handled in three ways:
  - **swallowed silently** with `let _ =` at the core knowledge-mutation sites (store, correct, tag, deprecate, quarantine, restore, lesson-learned);
  - **logged at WARN and dropped** at the other sites;
  - **propagated** only in CLI import provenance.
- **Twelve kinds of state change or security event emit no audit row at all** (rows 3, 10, 11, 13–17, 20, 23, 24 and 28). Rows 17 and 20 are the derived/telemetry state that row 31 classifies as non-audit.
- The live production store shows the consequence. Its audit log has had **zero successful rows since 2026-05-23**, while 1,381 entries, 1,109 agent enrollments and 1,717 graph edges were written. In the window when the audit log still worked, 27% of agent-authored entries left no audit trace.

**Evidence**

*Audit write primitives*

| P | Primitive | Mechanics | Error handling |
|---|---|---|---|
| P1 | `SqlxStore::log_audit_event` (`unimatrix-store/src/audit.rs:21-76`) | Opens its **own** transaction on the write pool (`audit.rs:22-26`). The id comes from the counter allocator (`counters.rs:139-161`, the #1003 reconcile). `timestamp` = wall time **when the audit row is written** (`audit.rs:33`), in seconds. Write pool = 1 connection (`pool_config.rs:76`), acquire timeout 5 s (`pool_config.rs:20`) | Returns `Err` |
| P2 | `AuditLog::log_event` (`infra/audit.rs:31-35`) | `block_sync` → `block_in_place` + `block_on` (`audit.rs:63-77`) | Returns `Err` |
| P3 | `AuditLog::log_event_async` (`infra/audit.rs:45-50`) | Direct await of P1 | Returns `Err` |
| P4 | `UnimatrixServer::audit_fire_and_forget` (`server.rs:658-669`) | `tokio::spawn`, handle dropped. The doc comment says "audit writes in-flight at runtime shutdown may be silently lost" | `warn!` and drop |
| P5 | `SecurityGateway::emit_audit` (`services/gateway.rs:325-339`) | `spawn_blocking` + P2, handle dropped | `let _ =`: **silent** |

*Inventory.* "Txn" = whether the audit row is in the state-change transaction. Every row has the shutdown / kill behaviour described in the section after the table.

| # | Path | State change | Audit write | Txn | On error | Loss, reorder and misattribution |
|---|---|---|---|---|---|---|
| 1 | `context_store` → `StoreService::insert` (`services/store_ops.rs:209-247`) | Entry, tags, vector map and outcome index, as **3 separate transactions** (`:209-226`) | `tokio::spawn` P3 (`:241-247`) | No | `let _`: **silent** | Lost on pool timeout, kill or shutdown. If a later step fails after `insert` commits (`:215-226`), the entry exists, the caller gets an error and **no audit row is written** |
| 2 | `context_store` near-duplicate (`store_ops.rs:168-181`) | None (returns the existing entry) | P5 | n/a | silent | Lost silently |
| 3 | `context_store` declared edges (`mcp/tools.rs:1145-1163`) | `graph_edges` rows | **None** | — | — | **No audit row.** The store row's `detail` does not list the edges |
| 4 | `context_correct` (`services/store_correct.rs:67-102`) | Deprecate the original and insert the correction (one transaction, `correct_entry`) | `tokio::spawn` P3 (`:97-102`) | No | `let _`: **silent** | As row 1 |
| 5 | `context_tag` (`services/store_tag.rs:~100-185`) | Add, remove or replace a tag | `tokio::spawn` P3 (`:179-185`) | No | `let _`: **silent** | As row 1. The audit is also **skipped by design** if metadata serialization fails (`:150-162`) |
| 6 | `context_deprecate` (`tools.rs:1739` → `server.rs:1028` → `change_status_with_audit` `:1168-1251`) | Status update (own transaction) | `tokio::spawn` P3 (`:1242-1247`) | No | `let _`: **silent** | As row 1 |
| 6b | Deprecate eager edge cleanup (`tools.rs:1752-1770`; `server.rs:683-736`) | `graph_edges` DELETE | P4 (the `edge_cleanup` row with removed tuples) | No | warn | The edges are deleted irreversibly. The only record of them is a row that can be lost |
| 7 | `context_quarantine` quarantine and restore (`tools.rs:2306,2344` → `server.rs:1045,1067`) | Status and `pre_quarantine_status` | As row 6 | No | `let _`: **silent** | As row 1. An Admin action can go unaudited |
| 8 | `context_enroll` (`tools.rs:2392-2434`) | `agent_registry` trust and capabilities (`registry.enroll_agent`, its own transaction) | P4 | No | warn | Lost on timeout, kill or shutdown. `credential_type:"none"` is hard-coded (`:2430`, G9) |
| 9 | Lesson-learned write in `context_cycle_review` (`tools.rs:4996` → `insert_with_audit` `server.rs:750-822`) | Entry (separate transactions) | `tokio::spawn` P3 (`:794-799`) | No | `let _`: **silent** | As row 1. `agent_id:"cortical-implant"` |
| 10 | **`context_edge`** create, delete and redirect (`tools.rs:3858-4096`; writes at `:3998`, `:4012`, `:4033`, `:4062`) | `graph_edges`: Supports, Contradicts, etc. | **None** | — | — | **No audit row** for an agent-driven change to the knowledge graph, including deleting or redirecting Contradicts edges, which changes what is served |
| 11 | Agent auto-enroll on first sight (`infra/registry.rs:67-78`); `bootstrap_defaults` (`:59`) | `agent_registry` rows | **None** | — | — | **No audit row.** 1,109 enrollments in the live store since the outage began; none are attributable |
| 12 | Background auto-quarantine (`background.rs:1440` → `emit_auto_quarantine_audit` `:1541-1595`) | Status → Quarantined | `tokio::spawn` P3 | No | warn | Lost on timeout or shutdown. One live-log failure (UNIQUE) |
| 13 | **Background auto-extraction** (`background.rs:1890`) | New Active entries (`created_by:"background-tick"`) | **None** | — | — | **No audit row.** 2,213 such entries in the live store |
| 14 | Background dead-knowledge one-shot deprecation (`background.rs:1330-1396`) | Status → Deprecated | **None** | — | — | **No audit row** |
| 15 | Background orphaned-edge compaction and repoint (`background.rs:805-860`, `:869`) | `graph_edges` DELETE and UPDATE | **None** | — | — | **No audit row** for bulk edge deletes |
| 16 | Background edge writers: NLI (`services/nli_detection.rs:81`), enrichment (`graph_enrichment_tick.rs:123ff`), behavioral (`behavioral_signals.rs:63`), co-access promotion | `graph_edges` inserts | **None** | — | — | No audit row. Machine-derived edges, but they change ranking and serving (S4 LI-3) |
| 17 | Maintenance (`services/status.rs:1070` vector map delete, `:1205` embedding dim, `:1282` confidence, `:1496,:1551` activity GC) | Derived and learning state | **None** | — | — | Derived state (see row 31 for classification) |
| 18 | `tick_skipped` (`background.rs:1513-1534`) | None (operational event) | `tokio::spawn` P3 | — | warn | Lost on timeout or shutdown |
| 19 | Transcript purge (`uds/listener.rs:2153-2176`; `status.rs:1601-1629`) | Purges an in-memory transcript buffer | `tokio::spawn` P3, after the lock is released | — | warn | Lost on timeout or shutdown |
| 20 | UDS hook cycle events (`uds/listener.rs:3085-3121`); session register/close; observations | `cycle_events` (the "structural audit trail", crt-025 ADR-003 #3000), sessions, observations | Not in `audit_log`. `tokio::spawn` direct write | No | warn | 6 live-log `insert_cycle_event` pool-timeout losses; 300+ observation-write timeouts |
| 21 | UDS peer-auth failure (`uds/listener.rs:649-668`) | None (security event) | `spawn_blocking` + P2 | — | warn | Lost on timeout or shutdown |
| 22 | HTTP `http_access` success and denial (`http/router/slug_guard.rs:52-68`, `:117-138`) | None (access or denial) | `handle.spawn` P3 into the target slug's log, or the principal's home slug for a denial | — | warn | Lost on timeout or shutdown. An unknown slug is dropped (`:55-57`). **A principal with no home slug writes no denial row** (`:118`). The row is not linked to the tool-call row (no request id) |
| 23 | **HTTP 401 auth failures** (`http/auth.rs:296-302`) | None (security event) | **None** | — | — | **No audit row** for a failed bearer authentication (asymmetric with row 21) |
| 24 | **MCP capability denials** (`server.rs:636-656` `require_cap`), content-scan rejects (`gateway.rs:300-312`), rate-limit rejects (`gateway.rs:88-95`) | None (security event) | **None** | — | — | **No audit row** for a denied or rejected MCP call. Probing is invisible |
| 25 | Read tools: lookup `:945`, get `:1307`, status `:1860`, briefing `:2194`, cycle `:4220`, cycle_review reads `:2664,:3656,:3719`; search (`services/search.rs:1437`, P5); `security_scan_warning` (`gateway.rs:220`, P5) | None (access) | P4 or P5 | — | warn or **silent** | Lost on timeout or shutdown |
| 26 | CLI import (`import/mod.rs:102` BEGIN IMMEDIATE … `:124` COMMIT; `record_provenance` `:266`, `:535-567`) | Whole store | V1 audit rows are copied **with 8 of 12 columns** (`import/inserters.rs:157-178`). The provenance row is written **after COMMIT** | No | Propagated | Attribution columns (`credential_type`, `capability_used`, `agent_attribution`, `metadata`) are lost. A crash between COMMIT and provenance leaves an import with no provenance row. Provenance `agent_id:"system"`, not the operator |
| 27 | CLI export (`export.rs:685-716`) | None (bulk data egress) | **None** | — | — | It exports only 8 of 12 audit columns (`:690-692`), which feeds row 26. **No audit row for the export itself** |
| 28 | CLI project register/delete/purge, token files, snapshot (`projects.rs`, `snapshot.rs`) | Routing, credentials, store destruction | **None** | — | — | No audit (G11's domain) |
| 29 | Audit retention (`retention.rs:271`; `status.rs:1578-1594`) | None | — | — | — | `gc_audit_log` is a no-op, which keeps the log append-only. It logs a WARN every tick: 7,681 lines in the live log |
| 30 | Storage enforcement (`db.rs:1022-1037`) | — | `BEFORE UPDATE` / `BEFORE DELETE` triggers only. `recursive_triggers` is never set (repo-wide grep). There is no audit hash chain: `chain_verify.rs` covers entries only | — | — | Verified on a scratch copy: UPDATE and DELETE are blocked, but **`INSERT OR REPLACE` with an existing `event_id` overwrites a row** (REPLACE does not fire DELETE triggers when `recursive_triggers=0`). **Backdated inserts into id gaps succeed.** `DROP TRIGGER` then DELETE succeeds. Nothing detects any of these |
| 31 | *Classification note* | Derived and learned state (rows 16 and 17, query_log, co_access, HNSW) | — | — | — | Not audit-class. It is governed by S4's learning-sink rules and is rebuildable. P2 must say so explicitly, otherwise "every state change" is unmeetable |

*Shutdown, OOM, kill and container stop (all fire-and-forget rows)*
- **Graceful shutdown** (`infra/shutdown.rs:119-246`):
  - It joins the acceptors (up to 35 s each), aborts the UDS listener (1 s) and the tick (1 s), dumps vectors and drops handles.
  - It does **not drain** detached audit tasks.
  - When `main` returns, runtime drop cancels every pending `tokio::spawn` task at its next await. `spawn_blocking` tasks (P5, row 21) run on, but by then the pool may be closed, and their errors are silent or WARN.
  - Pending audit tasks hold `Arc<Store>` clones, so they can also make `try_unwrap` fail and skip compaction (`:226-238`).
- **SIGKILL or OOM-kill**: any audit transaction not yet committed is lost, while the mutation it records is already committed.
  - Measured: fire-and-forget lost 1 row per kill in 3 of 3 runs.
  - "Sync-separate" (awaited, but its own transaction) lost 1 row in 1 of 3 runs, which is the window between the two commits.
  - Same-transaction lost 0 in 3 of 3.
- **Container stop**:
  - Docker's default grace period is 10 s. `docker-compose.yml` sets no `stop_grace_period`, and the Dockerfile sets no `STOPSIGNAL` (`Dockerfile:155-156`).
  - `graceful_shutdown` may legitimately wait 35 + 35 s, so SIGKILL can arrive **mid-shutdown**. Fire-and-forget rows in flight are lost (C27 already forbids relying on graceful shutdown).
- **Host power loss**: `synchronous=NORMAL` in WAL mode (`pool_config.rs:145`) can roll back the last committed transactions. With separate transactions, the mutation and its audit row can roll back independently.

*Reorder and misattribution*
- `event_id` is allocated when the detached task runs, so audit order is not commit order under concurrency.
- `timestamp` is the audit-write time in whole seconds, not the mutation time.
- `credential_type:"none"` is hard-coded on tool rows (G9). `session_id` is empty on background rows. Import is attributed to `"system"`.
- `http_access` rows and tool rows cannot be joined.
- Export and import strip 4 attribution columns (rows 26–27). In the live store all 5,580 rows read `credential_type='none'` with empty attribution, including rows written after vnc-014 (2026-04-22) added the columns.

*Live evidence (read-only copy of the live store, taken 2026-11-27; live log read in place)*
- **Six-month total audit outage.**
  - The last successful non-import audit row is from 2026-05-23. Counter `next_audit_id`=5580 against `MAX(event_id)`=5581: the #633/#1003 desync. Every allocation since then has hit UNIQUE and rolled back.
  - Since that date: 1,381 entries created, 1,109 agents enrolled, 1,717 edges written and 1,458 entries created or updated (504 now Deprecated, 13 Quarantined), **all without audit rows**.
  - The running daemon started on 2026-11-23, before the #1003 fix (2dfc415e, 2026-11-26), so the outage continues in the running process.
- **Logged failures are a lower bound.**
  - The live log has 939 `audit_fire_and_forget: write failed` lines and 1 auto-quarantine failure.
  - The `let _` sites (rows 1, 4–7, 9) **log nothing**, so their losses are invisible.
  - 363 write-pool timeouts appear across observations, cycle events, analytics batches ("batch discarded") and agent-registry errors returned to clients. That shows the write-pool contention behind the timeout loss path is real.
- **Loss inside the working window** (2026-02-23 → 2026-05-23):
  - 556 of 2,055 agent-authored entries (**27.1%**) are named in **no** audit row of any kind. This is a lower bound on missing store/correct rows, because a later read row naming the entry counts as coverage.
  - April 2026 has **0 audit rows** while 350 agent entries were created.
  - The per-row cause (#302/#308 starvation, desync, shutdown) cannot be separated, because the silent sites leave no log.
- **Fire-and-forget hid the failure.** A 100% audit outage ran for six months and was found by a human (#1003), not by the system. That falsifies "fire-and-forget is acceptable" for state-change audit even in V1.

**Recommendation**: Treat rows 1, 3–11, 13–15, 26–28 as **class-A defects** (a committed state change can lack its audit row), and rows 21–24 as **class-B defects** (a security event can lack its audit row). The fix is Q2's recommendation. Do not fix V1 site by site with retries: a retry cannot close the commit-then-audit window.

---

### Q: "Options for trustworthy audit: same-transaction audit write, transactional outbox, synchronous durable write, per-store tamper-evident hash chain + verification; retention/export. Recommend one, with failure semantics (fail the operation vs degrade)."

**Answer**:
- Adopt **same-transaction audit for every authoritative state change (class A), fail-closed**: if the audit insert fails, the transaction rolls back and the operation fails.
- Add a **per-store audit hash chain**, anchored outside the store.
- Add a **monotonic-insert trigger**.
- Security events with no mutation (class B) are written synchronously before the response.
- Access/read audit (class C) is the only class allowed off the request path. Even there it goes through a bounded, drained, loss-counted writer, never V1-style fire-and-forget.
- The hypothesis "fire-and-forget is acceptable for any audit class" is **rejected**:
  - Silent loss is acceptable for no class.
  - Asynchronous writing is acceptable for class C only, and only when the loss is recorded.

**Evidence: options**

| Option | Closes commit-without-audit? | Crash (measured, 3 SIGKILLs) | Cost vs today (Q3) | Fit with per-project SQLite | Verdict |
|---|---|---|---|---|---|
| **Same-transaction audit write** | Yes, by construction (atomic commit) | 0 lost in 3/3 | **Cheaper** than today: one commit instead of two | Native: the audit table is in the same file. It was the original design (#84, #267), abandoned for the #302/#308 pool starvation, which came from the blocking bridge and the second transaction, not from the audit row itself | **Adopt (class A)** |
| Transactional outbox | Yes, if the outbox row is in-txn | Same as same-txn for the outbox | Same-txn cost + a relay | Redundant **inside** one SQLite file: `audit_log` already *is* the durable outbox. Useful only to ship rows to an external sink (SIEM, tenant export, control-plane anchor) | Use `audit_log` + watermark as the outbox for **downstream** shipping only |
| Synchronous durable write (awaited, separate transaction) | No: window between two commits | 1 lost in 1/3 | ≈ today (2 commits); p50 478 µs vs 347 µs same-txn | Fits, but the extra commit buys nothing | Reject for class A. **Use for class B** (no mutation to share a transaction with) |
| Per-store hash chain + verification | Detects tampering (REPLACE overwrite, backfill, DROP TRIGGER + DELETE, truncation past an anchor); does **not** prevent loss | n/a | +61 µs p50 on top of same-txn; verify ≈ 2.5 µs/row (1M rows ≈ 2.5 s, Python upper bound) | One chain per store; matches P1's per-project chain | **Adopt with same-txn**: complementary, not an alternative |
| `synchronous=FULL` (local fsync per commit) | Host-crash durability only | — | 2.3× p50 (803 µs), 0.43× throughput | Off-box durability is the RPO question (D3/E4), not this | Reject; leave durability to D3 |

**Recommended design (directional)**
1. **Audit classes.**
   - **A — state change**: every committed change to authoritative state in a knowledge store or the control plane. That covers entries, status, tags, edges of every origin, registry and policy, routing, grants, tokens and purge.
     - It applies whether the actor is a client, a background job or an operator CLI.
     - It is written in the same transaction and fails closed.
   - **B — security event, no mutation**: authentication failures, authorization, tier and slug denials, rejected attestations, rate-limit and content rejects.
     - Written awaited in its own transaction before the response, with a bounded timeout.
     - On failure the request is still denied (the outcome is unchanged), plus an ERROR log, a metric, and a `gap` marker in the next row that does land.
     - Floods are aggregated per (principal, window) so a probe cannot become write amplification.
     - Destination per G10.
   - **C — access**: successful reads, search, briefing, `http_access` success.
     - Goes through a bounded in-process queue with batched commits.
     - The queue is drained with a deadline at shutdown.
     - Drops are counted and persisted as an `audit_dropped{n, from, to}` row, so the log is "complete or evidently incomplete".
     - A cell crash may lose up to one flush interval; that is declared.
   - **Not audit**: derived and learned state (row 31).
2. **Mechanics** (store crate only, per C6).
   - Each class-A store method takes an `AuditRecord` and writes it on the same transaction, for example `insert_with_audit`, `update_status_with_audit`, `tag_with_audit`, `edge_write_with_audit`, `enroll_with_audit`.
   - No class-A server code calls a standalone audit write.
   - The `event_id` allocator is **replaced by SQLite rowid assignment** (`INSERT … (event_id=NULL) … RETURNING event_id`). With DELETE forbidden, it is monotonic, and it removes the counter-desync failure class that caused the outage (#5840).
   - `timestamp` is the transaction's time. Add a millisecond column.
3. **Tamper evidence.**
   - Additive columns `prev_hash` and `row_hash`: SHA-256 over the length-prefixed canonical fields plus `prev_hash`. A genesis row is written when the V2 project is created.
   - Imported V1 rows stay unchained under one `v1_import_digest` row (P1 rule: nothing is re-hashed).
   - Add a `BEFORE INSERT` trigger that rejects `NEW.event_id <= MAX(event_id)`. That blocks the REPLACE overwrite and gap backfill seen at row 30. Imports stay compatible because the export is `ORDER BY event_id`.
   - Optionally also set `PRAGMA recursive_triggers=ON` per connection.
   - Anchor `(project_id, epoch, event_id, row_hash)` into the control-plane audit log periodically and on every drain or move (idempotent saga, per C18).
   - Add a `verify` CLI and a fleet verification job (C20).
4. **Failure semantics.**
   - **Class A: fail the operation.** This trades a silent 100% audit outage (V1, six months) for a loud write outage. It is acceptable because the allocator change removes the known trigger, and an alert on audit-insert errors makes it visible.
   - **Class B: deny anyway, then record the gap.**
   - **Class C: degrade with accounting.**
5. **Retention and export.**
   - Audit is retained for the life of the project, with no time-based GC: keep `gc_audit_log` a no-op, but stop the per-tick WARN.
   - Export and tenant export carry **all** audit columns plus the chain head. This fixes rows 26–27.
   - Removal happens only by whole-project purge, recorded as class A in the control-plane log (S2 Q4).
   - Sealed cold-segment archival to object storage comes after GA. The retention period itself goes to D15.
6. **Roles (human direction, not researched).** Audit logs are tenant-scoped. The **auditor role** (tenant-scoped, read-only, views audit logs) is a **later roadmap item**: no interface exists yet. See the proposed amendments.

**Recommendation**: Use same-transaction, fail-closed class-A audit with a rowid allocator, a per-store hash chain, a monotonic-insert trigger and a control-plane anchor. Write class B synchronously before the response, and class C through a drained, loss-counted queue. Reject V1-style fire-and-forget for every class.

---

### Q: "Measured per-write latency/throughput cost of the recommendation vs today; interaction with per-project stores, control-plane audit log (G11), vectors-in-SQLite (C1), Litestream RPO, multi-cell (C16–C28 in ass-111-s2-architect-review-2.md)."

**Answer**:
- The recommendation is **faster than today**, because it makes one commit instead of two.
  - Single writer: p50 347 µs vs 493 µs; throughput 2,166 vs 1,622 mutations/s (+34%).
  - 8 concurrent writers: 1,974 vs 1,563/s (+26%).
- The hash chain costs about 61 µs p50 and 13% throughput on top of same-transaction, and is still faster than today.
- Under write-pool pressure, today's path **lost 8 audit rows of committed mutations per run (3/3 runs)**. Same-transaction and sync-separate lost 0 in that scenario.
- Fire-and-forget gives no foreground latency benefit at steady state. With one write connection, the spawned audit transaction queues in front of the caller's next mutation.

**Evidence: method and environment**
- *Code*: a throwaway crate in the scratchpad (`…/scratchpad/s9/bench`, never in the repo). It depends by path on `unimatrix-store`, uses `SqlxStore::open` with `PoolConfig::default()` (1 write connection, 5 s acquire, WAL, `synchronous=NORMAL`), and calls the real `log_audit_event` and `counters::next_audit_event_id`.
- *Mutation*: a status flip using the same SQL as `SqlxStore::update_status` (SELECT, UPDATE entries, two counter updates) on a caller-owned transaction. An entry-insert variant uses the real `SqlxStore::insert`.
- *Variants*:
  - **MutOnly**: no audit.
  - **FireForget**: today's pattern, `tokio::spawn(log_audit_event)`. Throughput is measured until every task has drained.
  - **SyncSeparate**: awaited, separate transaction.
  - **SameTxn**: audit insert on the mutation's transaction.
  - **SameTxnChain**: SameTxn plus reading the previous row hash and SHA-256.
  - **SameTxnFull**: SameTxn with `synchronous=FULL`.
- *Data*: a fresh copy, per run, of the live store taken via the SQLite backup API (150.9 MB; 5,955 entries; 5,581 audit rows). n = 2,000 operations × 3 reps; medians reported.
- *Host*: AMD EPYC-Genoa, 16 vCPU, 62 GB; Linux 6.12; container overlay FS; release build; sqlx 0.8.6 (libsqlite3-sys 0.30.1, bundled SQLite); tokio 1.54.1 multi-thread.
- *Caveats*: the page cache was warm. Overlay-FS fsync behaviour differs from host NVMe, so the FULL numbers are indicative only.

| Scenario | p50 µs | p95 µs | p99 µs | ops/s (to drain) | Committed without audit |
|---|---|---|---|---|---|
| Status, mutation only | 279 | 472 | 1,267 | 2,678 | — |
| **Status + fire-and-forget (today)** | 493 | 758 | 8,155 | 1,622 | 0 |
| Status + sync-separate | 478 | 662 | 7,704 | 1,691 | 0 |
| **Status + same-txn (recommended)** | **347** | 536 | 2,545 | **2,166** | 0 |
| Status + same-txn + chain | 408 | 660 | 2,520 | 1,893 | 0 |
| Status + same-txn, FULL | 803 | 1,977 | 3,691 | 929 | 0 |
| Insert only | 388 | 765 | 7,710 | 1,722 | — |
| Insert + fire-and-forget (today) | 639 | 1,394 | 9,154 | 1,091 | 0 |
| Insert + sync-separate | 642 | 1,163 | 9,828 | 1,084 | 0 |
| *8 concurrent*: mutation only | 2,125 | 3,185 | 13,406 | 3,127 | — |
| *8 concurrent*: fire-and-forget | 4,179 | 13,840 | 16,103 | 1,563 | 0 |
| *8 concurrent*: sync-separate | 3,908 | 13,841 | 15,961 | 1,677 | 0 |
| *8 concurrent*: same-txn | 3,279 | 12,820 | 15,905 | 1,974 | 0 |
| *8 concurrent*: same-txn + chain | 3,239 | 11,855 | 13,978 | 1,954 | 0 |
| *8 conc. + 6 s write-connection hog*: fire-and-forget | 4,401 | 13,928 | 16,883 | 272 | **8 (3/3 runs)** |
| … sync-separate | 3,705 | 12,182 | 15,829 | 279 | 0 |
| … same-txn | 3,143 | 12,308 | 14,987 | 285 | 0 |

- In the hog scenario, one 6 s write transaction stands in for a long tick, GC or drain step, or an observation-write flood. All variants failed 8 mutations *visibly*, because the caller got an error. Only fire-and-forget also dropped 8 audit rows for mutations that **had** committed.
- Crash and shutdown probes (2,000 operations, SIGKILL at 0.9 s, 3 runs each):

| Variant | SIGKILL: committed without audit | Exit with pending tasks |
|---|---|---|
| Fire-and-forget | 1, 1, 1 | 0, 0, 1 |
| Sync-separate | 1, 0, 0 | 0 |
| Same-txn | 0, 0, 0 | 0 |

- The single-writer backlog at exit is small. A concurrent backlog at exit, as in the hog case, scales the loss.
- Raw CSVs are in `…/scratchpad/s9/data/{lat,conc,hog}.csv` and `crash.txt`.

**Interactions**
- **Per-project stores.**
  - Audit cost is per project. Every project has its own file and its own write connection, so the cost runs in parallel across projects on a cell.
  - The measured ≈2,000 audited mutations/s per project is orders of magnitude above demand. The heaviest real corpus made 5,955 entries in 9 months.
  - Chains are per store. There is no global sequence; cross-store ordering uses timestamp plus correlation id.
- **Control-plane audit (G11).**
  - The same pattern applies: same-transaction class A and a chain in the control-plane SQLite DB (and later in Postgres under Option C: row triggers, a statement-level TRUNCATE trigger and REVOKE, per S2).
  - Because no transaction spans the control plane and a project store (C18), a cross-plane operation writes class-A rows in **both** logs, carrying one `correlation_id`, as idempotent saga steps.
  - The control-plane log also holds the platform denial log (G10), the project chain anchors, and operator CLI actions (row 28).
- **Vectors in SQLite (C1).**
  - The embedding BLOB (about 1.5 KB) joins the entry insert's transaction, and the audit row (about 100–300 B) joins the same transaction. It adds bytes but no commits.
  - HNSW insertion stays post-commit as derived state, with no audit.
  - Under today's pattern, C1 would still leave the audit row as a second commit.
- **Litestream RPO.**
  - With same-transaction audit, every restored replica is transaction-consistent: no replica holds a mutation without its audit row.
  - With fire-and-forget, a replica can hold unaudited mutations even when nothing failed, because the snapshot races the pending tasks.
  - On failover, data and audit roll back **together** (the E4/R3 truncation). Truncation is detectable only against an anchor, so the anchor interval should be ≤ the RPO interval.
  - The R2 PUT cost is driven by the sync interval, not by commits (S2), so this is neutral to slightly favourable: half the WAL frames.
  - Same-transaction audit does **not** reduce RPO. Off-box durability stays D3/E4.
- **Multi-cell (architect review 2).**
  - *C16*: the two-cell move test must also run audit chain-verify before and after a move: no gap, no fork, head continuous across the epoch.
  - *C17*: audit attribution comes only from the validated `RoutedRequest` (principal, tenant, project, epoch). That is G9's source of truth.
  - *C19*: add `epoch` to audit rows, with class-A `project_opened{epoch}` written on open and `project_drained{epoch}` before the snapshot. The chain head goes to the control plane at drain. A stale-epoch writer cannot extend the chain, because fencing closes the store.
  - *C20*: fleet jobs that mutate a project (purge, quarantine purge, model-epoch re-embed) write class-A rows in the project store as `system:<job>`, correlated to the control-plane job row.
  - *C22*: the class-C queue is cell-local and loss-tolerant. A crash loses at most one flush interval, as declared.
  - *C26*: audit bytes must never trip an enforceable quota, or class-A fail-closed turns a quota hit into a write refusal for the audit row itself. Check quota against data rows before the transaction.
  - *C27*: same-transaction class A meets "no reliance on graceful shutdown for durability". Class C is declared best-effort.

**Recommendation**: Adopt the recommendation on performance grounds as well as integrity grounds: it is faster than today. Budget the chain (about 61 µs p50) as accepted. Do not adopt `synchronous=FULL`. Bind the C16 move test, the C19 epoch rows and the C18 dual-log correlation into Stage 2 acceptance.

---

### Q: "Amendments to P2, G9, G11, stages."

**Answer**: Restate P2 by audit class, with same-transaction and tamper-evidence language. Extend G9 to cover correlation, epoch, system and operator principals, and export/import column fidelity. Extend G11 to cover operator CLI actions, anchors and the platform denial log. Add a new gap **G18 (audit write path)** to Stage 1. The exact text is in "Proposed amendments to ass-111-findings.md" below.

**Evidence**:
- Q1 inventory rows 1–31, the live-store outage, and the bench and crash probes.
- S1 G9, G10 and G11.
- S7's verdict "P2: Holds" (`ass-111-findings.md:140`) and its P2 text (`:584`). Both overstate V1: the draft text cannot be met without G18.

**Recommendation**: Amend S7 as below. Without G18, the verdict on P2 changes from "Holds" to "**Holds only after Stage 1 G18**".

---

## Proposed amendments to ass-111-findings.md

*(Proposals only. S9 does not edit the synthesis.)*

1. **Hard-constraint verdict, P2 row** (`:140`):
   - Replace "Holds." with: "Holds **after Stage 1 G18** (S9). V1 audit is written outside the state-change transaction with swallowed errors. Twelve kinds of state change or security event emit no row. The live store has a six-month audit outage. V1 rows are kept unchanged. V2 attribution columns are additive. The control plane gets its own audit log (G11)."
2. **Principle 2 text** (`:584`): replace it with:
   > **The audit log is append-only, complete and tamper-evident.** Every committed change to authoritative state — in a knowledge store or the control plane, by a client, a background job or an operator — writes its audit entry in the same transaction, or does not commit. Security events that change no state (authentication failures, denials, rejected attestations) are recorded before the response. Access to knowledge is recorded as access; any loss is itself recorded. Derived and learned state (indexes, confidence, co-access, telemetry) is not audited state. Each store's audit log is a hash chain whose head is anchored outside the store. Each entry is attributed to the authenticated principal, credential type and attestation tier, kept separate from the self-asserted agent annotation. DDL triggers forbid update, delete and non-monotonic insert. The audit log is never a learning source.
3. **G9** (S1 gap list; S7 Stage 1 "Attribution"):
   - Add to the additive columns: a `request_id`/`correlation_id` that joins `http_access`, tool, and control-plane rows; `epoch` (C19); and a millisecond timestamp.
   - Background rows carry `principal = system:<job>`. Import provenance carries the **importing operator principal**, not `"system"`.
   - **Export and import must carry every audit column** (rows 26–27). Today they carry 8 of 12, so S7 Q3's "no audit row is rewritten" is false in effect: four columns are dropped.
4. **G10**: a principal with no home slug currently writes **no** denial row (row 22). Under V2, every denial goes to the platform log at minimum. HTTP 401s and MCP capability, content and rate denials (rows 23–24) are class B.
5. **G11**: the control-plane audit is class A, same-transaction and chained. It also holds:
   - operator CLI actions (project register/delete/purge, token issue/revoke, snapshot, export; row 28);
   - the platform denial log (G10);
   - project chain anchors;
   - saga correlation rows (C18).
   Tenant data export is a class-B access event recorded in the control plane.
6. **New gap G18 — audit write path** (S9): class-A audit runs in the same transaction and fails closed. It comes with:
   - a rowid allocator with no counter;
   - a monotonic-insert trigger;
   - a per-store chain;
   - store-crate `*_with_audit` methods (C6);
   - audit for rows 3, 10, 11, 13–15;
   - a class-B synchronous writer and a class-C drained, counted queue;
   - no per-tick GC WARN.
7. **Stages**:
   - **Stage 1**, under *Attribution and learning sinks*: add G18 (class A + chain + trigger + allocator). It touches the same write sites as G9; `store/write_ext.rs` and `store/write.rs` are pre-split at first touch, already listed for G9. *Must prove*:
     - an **audit census test** in which every mutating path in inventory rows 1–15 yields exactly one row in-transaction (extend the existing store/server test fixtures; no new scaffolding);
     - a **SIGKILL probe** with 0 committed-without-audit over N kills;
     - a **write-hog probe** with 0 committed-without-audit;
     - `verify` passes on a migrated store.
   - **Stage 2**: class-B writer (with G10); control-plane audit chain (G11); anchoring saga; the C16 move test verifies audit chain continuity; the C19 epoch rows; class-C queue.
   - **Stage 3**: migration receipts carry the V1 audit digest and the V2 genesis head, and state the V1 history gaps (see R30).
   - **Stage 4**: the fleet audit-verification job runs under the C20 scheduler.
   - **Stage 7 (GA)**: audit retention published (D15); chain verify green fleet-wide; security review covers rows 21–30.
   - **Post-GA**: the **auditor role** (tenant-scoped read-only audit viewer; human direction) once an audit-viewing interface exists; sealed audit archival; tenant-visible anchors (with KI-CHAIN-XV-STRONG).
   - **"What must be proven first"**: insert "G18 same-transaction audit, census test" alongside item 3 (G9).
8. **Risks**:
   - **R28 (new)**: audit-coupled availability. Fail-closed class A turns an audit defect into a write outage. *Mitigation*: rowid allocator, census test, alert on audit-insert errors. *Residual*: accepted by design.
   - **R29 (new)**: V1 audit is non-transactional and partly missing, with known tamper paths (rows 1–30). *Mitigation*: Stage 1 G18. *Residual*: none after Stage 1 for V2; V1 lines stay as they are.
   - **R30 (new)**: V1 audit history has a six-month hole (live store) and lost attribution columns. It cannot be repaired. *Mitigation*: declare it in migration receipts. *Residual*: permanent.
   - **Extend R11**: the operator, as the raw-DB adversary, can also rewrite a whole audit chain until anchors leave the operator's sole control.
9. **Decisions**: add D24–D27 (below; IDs provisional for the S7 editor).
10. **V1 defects table**:
    - add the fire-and-forget class-A loss (rows 1, 4–9);
    - add the unaudited mutators (rows 3, 10, 11, 13–15);
    - add the live-store audit outage (the running daemon predates the #1003 fix);
    - add the export/import column drop (rows 26–27);
    - add the trigger bypass (row 30).
    Disposition: Stage 1 G18 on `main`; V1 per D27.
11. **Q3 migration text** (`:414`, `:442-447`): "no audit row is rewritten" → "no audit row is rewritten; V1 rows carry the columns the V1 export emits (8 of 12 unless D27 fixes the export before the V1 cut)".
12. **Roles**: record "auditor role (tenant-scoped, read-only, audit logs) — deferred; no interface" as a roadmap item. This is a human direction, not a decision.

---

## Escalations for human

*(For S7's decision list. S9 makes no decisions.)*
- **D24 — Audit classes and failure semantics.** Adopt A (same-transaction, **fail the operation**), B (synchronous, deny anyway, record gaps) and C (best-effort, loss recorded), and reject fire-and-forget for all classes. Fail-closed class A trades silent audit loss for possible write outages (R28). *Authority*: likely H1 (security posture) and H2 (scope). *Gates*: Stage 1 G18.
- **D25 — Access-audit completeness for SaaS tenants.** Is best-effort, loss-counted read/access audit acceptable for every tenant, including Med Search? Or must some tenants get complete, synchronous read audit, at about the class-B cost on every read? *Recommendation*: best-effort counted by default, with a per-tenant upgrade path. Ties to D11 and D15.
- **D26 — Anchor strength.** Choose between anchoring audit chain heads in the control plane only (detects everything except an operator who controls both) and also issuing tenant-visible heads in export and migration receipts (detects operator rewrites). *Recommendation*: control plane at Stage 2, tenant-visible receipts by GA. Ties to R11 and KI-CHAIN-XV-STRONG.
- **D27 — V1 export fidelity before the V1 cut.** The V1 export drops 4 of 12 audit columns, and S7 says the branch-point tag fixes the export format (T20). Options:
  - (a) fix export and import to carry all columns **before** the cut, as a small V1 change on `main`;
  - (b) accept the loss and declare it in receipts.
  *Recommendation*: (a). Also restart the live daemon on a binary that contains the #1003 fix, which is an operational step, not a decision.
- **Audit retention** (fold into D15): keep audit for the life of the project with no time-based GC, or set a retention window with sealed archives. *Recommendation*: life of the project until GA.

---

## Unanswered Questions

- **The Litestream interaction was not measured.** It is gated on the C12 checkpoint PoC, which is outside S9. The transaction-consistency claim rests on Litestream shipping committed WAL state; that is directional.
- **Off-box-before-ack durability cost** (synchronous replication) was not measured. It belongs to D3/E4 and is independent of the audit design.
- **The Postgres control-plane audit cost** was not measured. Option C is not adopted until an HA trigger fires.
- **The per-cause split of the live-store loss** (desync vs pool timeout vs shutdown) cannot be determined. The silent `let _` sites leave no log, so only the 939 logged failures and the coverage gaps are attributable.
- **The outbox relay cost** was not measured. Analysis rejects the outbox as redundant inside one SQLite file.

---

## Out-of-Scope Discoveries

1. **ADR drift (architect-owned).** vnc-002 ADR-001 (#84) and vnc-006 ADR-003 (#267) record same-transaction write+audit. The code abandoned it for #302/#308 fire-and-forget, with no superseding ADR. Correcting them falls to uni-architect.
2. **The KI-AUDIT capability (#5546) is marked `delivery:proven`, but its `done_when` ("each mutation writes an attributed row") is not met** (inventory rows 1–15). The campaign may not edit the capability map; flag it for uni-zero.
3. **Live operations.** The running daemon (started 2026-11-23) predates the #1003 fix. Restarting it on a current binary self-heals the allocator going forward. The six-month hole is permanent.
4. **Write-pool contention is a V1 durability issue beyond audit.** It caused 363 pool timeouts: discarded analytics batches, lost observations, 6 lost `cycle_events` rows (contrary to crt-025 ADR-003 #3000's no-silent-loss intent), and agent-registry errors surfaced to clients. **Security-relevant**: any caller able to generate write-pool load raises the drop probability of fire-and-forget audit rows for concurrent mutations. That is an audit-suppression lever until G18 lands.
5. **Import provenance** is written after COMMIT and attributed to `"system"`. A crash in between leaves an unprovenanced import, and C-5 then refuses a re-import into that target.
6. **The multi-transaction insert paths** (rows 1 and 9) can leave an entry with no vector mapping and no audit row, while returning an error that invites a retry, so duplicates are possible.
7. **The crt-058 edge-cleanup reconstructability metadata** (`metadata` column) is lost by export and import.
8. **`gc_audit_log` WARN spam**: 7,681 log lines. It is noise and should be removed with G18.
9. **`emit_audit` uses `spawn_blocking` + `block_in_place`**. On a current-thread runtime it would panic inside a detached task without being seen. This is a test-runtime hazard only.
10. **A reusable pattern candidate** (for a downstream session to validate, not stored): "the audit row shares the mutation's transaction; the allocator must not be able to trail the table; fail closed". The six-month silent outage is the lesson's evidence.

---

## Recommendations Summary

*(Safe for public posting: no exploitable specifics.)*
- **Q1 (inventory)**: V1 writes no audit row inside the state-change transaction. Errors are swallowed or only logged. Several kinds of state change and security event produce no audit row at all (inventory rows 3, 10–11, 13–15, 21–24, 26–28). Audit-log integrity relies on triggers that a raw-database actor can get around (row 30). A long-running silent audit outage was confirmed in a real store. Treat inventory rows 1–15 and 26–28 as class-A defects and rows 21–24 as class-B defects.
- **Q2 (design)**: Put every authoritative state change's audit row in the same transaction, fail closed. Replace the counter allocator with rowid assignment. Add a per-store hash chain, a monotonic-insert trigger and a control-plane anchor. Write security events synchronously before the response, and access events through a drained, loss-counted queue. Reject fire-and-forget for every audit class.
- **Q3 (cost)**: Measured: the recommendation is faster than today, at +34% throughput for a single writer and +26% with 8 concurrent writers, because it makes one commit instead of two. The hash chain adds about 61 µs p50. Today's pattern lost audit rows of committed mutations under write pressure and on kill; same-transaction lost none. It fits per-project stores, G11, C1, Litestream (replicas stay transaction-consistent) and C16–C28, provided the move tests verify audit-chain continuity.
- **Q4 (amendments)**: Restate P2 by audit class (same-transaction, tamper-evident, derived state excluded). Extend G9 (correlation id, epoch, system and operator principals, full-column export) and G11 (operator actions, anchors, platform denial log). Add gap G18 to Stage 1 with census, kill and write-hog proofs. Add risks R28–R30 and decisions D24–D27. Record the tenant-scoped auditor role as a post-GA roadmap item.

---

Unimatrix entries consulted: #84, #267, #5714, #5546, #5840, #3000, #101, #261, #253, #4855, #5681, #5551, #4604, #5692 (briefing), #102.
