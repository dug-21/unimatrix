## ADR-007: Durable compaction_events Table — Insert-Only at handle_compact_payload, Co-located with increment_compaction, No Lock Held Across the INSERT

### Context
crt-054's Surface A is a NEW durable table, `compaction_events`, the gate boundary crt-055 compares PostToolUse read `ts` against to compute `compaction_reread`. Compaction is authoritative server-side but today lives only as an in-memory counter (`increment_compaction`, `infra/session.rs:554-559`); no persisted compaction timestamp exists, so the now-live `context_reload` (#758) is cross-session file overlap, not compaction-gated. crt-055 cannot compute the compaction tax without a durable timestamp to gate against. crt-054 supplies exactly that.

There is no prior ADR for this table — it is genuinely new (the prior crt-054 design persisted activity onto `cycle_review_index`, now wholly crt-055's). The crt-055 producer contract (binding) fixes the columns; this ADR fixes the write site, the write discipline, and resolves the #1 open question: **lock ordering at the seam (SR-01).**

The authoritative seam is `handle_compact_payload` (`uds/listener.rs:1737`), specifically co-located with the existing `increment_compaction` call at `listener.rs:1854`. Investigation of the seam:
- `increment_compaction` (`session.rs:554-559`) takes the `sessions` mutex, does one integer increment, and **releases it internally** — it holds no lock across its own return.
- By `listener.rs:1854` the per-session buffer lock has already been taken and released (the transcript-tail read at `listener.rs:1833-1835` drops its guard before any await/formatting).
- `handle_compact_payload` is `async` and has the `Store` reachable via `services.store_ops` (the `ServiceLayer` owns `store_ops: StoreService`, `services/mod.rs:239`).

So at line 1854 **no registry lock and no buffer lock is held**; an INSERT placed here acquires only the DB connection, ordered after (not nested inside) the registry/session locks. This dissolves SR-01: there is no lock nesting to deadlock against.

### Decision
Add a new `compaction_events` table, insert-only, one row per compaction event, written at `handle_compact_payload` co-located with `increment_compaction` (`listener.rs:1854`).

Columns (per the binding crt-055 contract):

| Column | Type | Null/Default | Semantics |
|--------|------|--------------|-----------|
| `id` | INTEGER | PK (rowid) | surrogate key |
| `session_id` | TEXT | NOT NULL | the compacting session; the cycle join is resolved at review via the session→`feature_cycle` declaration chain — `feature_cycle` is deliberately NOT stored |
| `compacted_at` | INTEGER | NOT NULL | Unix **seconds** of the event (server wall clock — `now_secs()` / `SystemTime…as_secs()`, the unit already used server-side) — the gate boundary `compaction_reread` compares against. The unit is documented explicitly in the DDL comment (below) so no consumer mis-reads it as millis. |
| `high_water` | INTEGER | NOT NULL DEFAULT 0 | `TranscriptBuffer.high_water` (`session_transcript.rs:53`, accessor `:333`; monotonic bytes sent, invariant I3) captured at the handler — **reserved** for a future precise byte-boundary gate; populated now to avoid a second migration |

Write discipline:
1. **Exactly one INSERT per compaction event**, never updated. `feature_cycle` is NOT stored — the row is written regardless of declaration (server-authoritative), and attributed to a cycle only at review. This is the producer-only dissolution of the held/registered-route edge case: the event is durable and session-keyed, independent of whether the session's buffer was held at drain (contrast ADR-004, where Surface B *is* declaration-gated).
2. **Lock ordering (SR-01 resolved):** place the INSERT at `listener.rs:1854`, after `increment_compaction` returns and after the buffer-tail guard has dropped. No registry/session/buffer lock is held across the INSERT; it acquires only the DB connection. The INSERT must NOT be moved inside any `sessions`/registry critical section.
3. **`high_water` capture:** read `session_state.transcript`'s `high_water()` under the buffer lock at the seam (the same `Arc` the tail read at `:1833` already shares — no new registry read), capture the value, drop the guard, then INSERT. Capturing the scalar before the INSERT keeps the buffer lock and the DB acquisition non-overlapping.
4. **Transaction shape (Open Q4 resolved):** a single **autocommit INSERT helper on `store_ops`, no explicit transaction**, no lock held. SQLite autocommits a lone INSERT; wrapping it in an explicit transaction buys nothing and would only widen the window. The helper is a thin single-statement INSERT (add one if `store_ops` lacks it). It does not contend with the briefing write path (that has completed by `:1854`).
5. **Hot-path posture:** the compaction handler already does async briefing I/O (`services.briefing.index(...).await`, `:1804`) before this point, so it is not a microsecond-critical path; the single INSERT is on-path, **non-blocking**, and ordered after `increment_compaction`. The compaction ACK must not be blocked by INSERT failure — on DB error, log ids/counts (no content) and let the compaction response proceed (mirrors the briefing graceful-degradation at `:1810`). Surface A absence for a cycle is then a fail-loud absence at crt-055's review, never a fabricated row.
6. **Observability — named failure metric (not a generic log line):** a silently-dropped INSERT undercounts compactions and the gate signal degrades invisibly. On INSERT failure, emit a **named metric/counter** (e.g. `compaction_events_insert_failed`), not just a `tracing::warn!`, so systematic failure is detectable. This counter also lets crt-055 cross-check the row-derived `compaction_count` against the in-memory `increment_compaction` count and flag drift between the two.
7. **Content-free:** no payload, no `tracing` of content. `id`/`session_id`/`compacted_at`/`high_water` only (ADR-005).

DDL — the `compacted_at` unit is documented in the schema comment, e.g.:

```sql
CREATE TABLE IF NOT EXISTS compaction_events (
    id           INTEGER PRIMARY KEY,
    session_id   TEXT    NOT NULL,
    compacted_at INTEGER NOT NULL,  -- Unix SECONDS (server wall clock, now_secs/.as_secs()); the compaction_reread gate boundary
    high_water   INTEGER NOT NULL DEFAULT 0  -- TranscriptBuffer.high_water (bytes sent) at compaction; reserved for future byte-boundary gating
);
CREATE INDEX IF NOT EXISTS idx_compaction_events_session ON compaction_events(session_id);
```

**Clock-unit boundary (not crt-054's reckoning):** crt-054 guarantees + documents **seconds** and changes nothing in storage. The gate-side `ts/1000` normalization (PostToolUse `ts` is epoch millis, `session_metrics.rs:115`) belongs to crt-055 at the gate, per crt-055 Binding constraint 8 — crt-054 supplies seconds and rows; crt-055 does all reckoning.

Indexing: index on `session_id` (review-time lookup is by session). Multiplicity: 0..N rows per session; the boundary-selection semantics when a session compacts multiple times are a crt-055 reckoning detail, out of crt-054's scope.

Migration: a new table via `CREATE TABLE IF NOT EXISTS` (idempotent — no `pragma_table_info` pre-check needed for a whole new table), added to the `run_main_migrations` upgrade block under an `if current_version < N` guard, AND to the `create_tables_if_needed` fresh-create path (`db.rs`), taking the next `CURRENT_SCHEMA_VERSION` bump (ADR-008). Cascade-file existence verified (#4484).

### Consequences
Easier: SR-01 dissolved by placement — no lock nesting, the INSERT is ordered after the registry critical section, not within it; the durable compaction gate crt-055 needs exists with one INSERT; the held/registered edge case disappears because the row is declaration-independent and session-keyed.

Harder: one new table + migration + fresh-create site to keep in lockstep (the three-path bump hygiene, #4153); a DB error at the seam is tolerated (logged + a named failure counter, non-blocking) so a compaction ACK is never blocked — accepted because a missing row reads as fail-loud absence at review, not a corrupt value, and the named counter makes systematic loss detectable and lets crt-055 flag row-vs-increment drift; `high_water` is server-captured at the handler, not wire-precise (SR-10) — documented as reserved so crt-055/future gating does not over-trust it.

Cross-refs: crt-055 producer contract §"Surface A" (the binding columns), ADR-008 (the schema-version bump this table takes), ADR-005 (content-free), ADR-004 (the declaration-gating contrast — Surface A is NOT gated), `increment_compaction` (`session.rs:554-559`), `handle_compact_payload` (`listener.rs:1737`/`:1854`), #4484 (cascade-file existence), #4153 (three-path bump).
