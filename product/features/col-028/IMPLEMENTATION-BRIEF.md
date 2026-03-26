# col-028 Implementation Brief: Unified Phase Signal Capture (Read-Side + query_log)

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/col-028/SCOPE.md |
| Architecture | product/features/col-028/architecture/ARCHITECTURE.md |
| Specification | product/features/col-028/specification/SPECIFICATION.md |
| Risk Strategy | product/features/col-028/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/col-028/ALIGNMENT-REPORT.md |

---

## Goal

Resolve two related phase-capture gaps — both share the same root cause and unlock the same
three downstream consumers (phase-conditioned frequency table, Thompson Sampling per-(phase, entry)
arms, and gap detection). Gap 1: all four read-side MCP tools (`context_search`, `context_lookup`,
`context_get`, `context_briefing`) emit phase-free usage events despite `SessionState.current_phase`
being available at call time, and `context_get`/`context_briefing` carry incorrect access weights.
Gap 2: the `query_log` table has no `phase` column, so phase is never persisted regardless of what
is captured in memory. Both changes are additive and backward-compatible. GH issues closed: #394
(in-memory) and #397 (persistence).

---

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| SessionState (infra/session.rs) | pseudocode/session-state.md | test-plan/session-state.md |
| Phase Helper + Four Read-Side Call Sites + query_log Write Site (mcp/tools.rs) | pseudocode/tools-read-side.md | test-plan/tools-read-side.md |
| D-01 Guard (services/usage.rs) | pseudocode/usage-d01-guard.md | test-plan/usage-d01-guard.md |
| Schema Migration v16→v17 (unimatrix-store) | pseudocode/migration-v16-v17.md | test-plan/migration-v16-v17.md |

### Cross-Cutting Artifacts

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

---

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Phase helper shape | Free function `current_phase_for_session(&SessionRegistry, Option<&str>) -> Option<String>` at module scope in `mcp/tools.rs`; `pub(crate)` for unit testability without handler construction | SCOPE.md §D-04, ARCHITECTURE.md §Component 2 | architecture/ADR-001-phase-helper-free-function.md (#3513) |
| Phase snapshot timing | First statement in each handler body, before any `.await`; single `get_state` call shared for both `UsageContext.current_phase` and `QueryLogRecord.phase` at the `context_search` site | ARCHITECTURE.md §ADR-002, SPECIFICATION.md §C-01 | architecture/ADR-002-phase-snapshot-placement.md (#3514) |
| Weight-0 guard location | Early-return guard at top of `record_briefing_usage`, before `filter_access`; not at `AccessSource` dispatch level | SCOPE.md §D-01, ARCHITECTURE.md §Component 4 | architecture/ADR-003-weight-zero-guard-briefing.md (#3515) |
| confirmed_entries trigger for context_lookup | Request-side cardinality: `target_ids.len() == 1` (not response-side); reflects agent intent, not query artifact | SCOPE.md §D-02, ARCHITECTURE.md §Component 1 | architecture/ADR-004-confirmed-entries-request-cardinality.md (#3516) |
| confirmed_entries ships with no consumer | Add field now so Thompson Sampling inherits populated data; sessions are ephemeral and non-backfillable | SCOPE.md §D-03, ARCHITECTURE.md §Component 1 | architecture/ADR-005-confirmed-entries-no-consumer.md (#3517) |
| UsageContext doc comment update | Required deliverable: update `current_phase` doc comment to reflect read-side tools now populate the field (was "None for all non-store operations") | ARCHITECTURE.md §Component 3 | architecture/ADR-006-usagecontext-doc-comment-update.md (#3518) |
| query_log.phase column position | Appended as last positional parameter (`?9` in INSERT, index 9 in SELECT/deserializer); no reindexing of existing binds | ARCHITECTURE.md §Component 5 | architecture/ADR-007-query-log-phase-column-append-last.md (#3519) |

---

## Files to Create or Modify

### unimatrix-server crate

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/infra/session.rs` | Add `confirmed_entries: HashSet<u64>` field to `SessionState`; add `record_confirmed_entry(&self, session_id: &str, entry_id: u64)` to `SessionRegistry`; update `register_session` initialiser; update `make_state_with_rework` and all test helpers (pattern #3180) |
| `crates/unimatrix-server/src/mcp/tools.rs` | Add free function `current_phase_for_session`; update four read-side handlers (`context_search`, `context_lookup`, `context_get`, `context_briefing`) for phase capture, weight corrections, and `confirmed_entries` recording; update `UsageContext.current_phase` doc comment per ADR-006; pass `phase` to `QueryLogRecord::new` in `context_search` |
| `crates/unimatrix-server/src/services/usage.rs` | Add D-01 early-return guard at top of `record_briefing_usage` before `filter_access`; update `UsageContext.current_phase` doc comment |
| `crates/unimatrix-server/src/server.rs` | Update lines 2059 and 2084: `assert_eq!(version, 16)` → 17 (SR-02 schema version cascade) |

### unimatrix-store crate

| File | Change |
|------|--------|
| `crates/unimatrix-store/src/migration.rs` | Bump `CURRENT_SCHEMA_VERSION` to 17; add `if current_version < 17` migration branch with `pragma_table_info` pre-check, `ALTER TABLE query_log ADD COLUMN phase TEXT`, and `CREATE INDEX IF NOT EXISTS idx_query_log_phase ON query_log (phase)` |
| `crates/unimatrix-store/src/analytics.rs` | Add `phase: Option<String>` to `AnalyticsWrite::QueryLog` variant; add `phase` as `?9` in SQL INSERT column list and bind; treat INSERT + SELECT + deserializer as atomic change unit (SR-01) |
| `crates/unimatrix-store/src/query_log.rs` | Add `phase: Option<String>` to `QueryLogRecord` struct with doc comment; update `QueryLogRecord::new()` signature (phase as final arg); update `insert_query_log` to pass `record.phase.clone()`; add `phase` to both SELECT statements; update `row_to_query_log` to read index 9 as `Option<String>` |

### Test files

| File | Change |
|------|--------|
| `crates/unimatrix-store/tests/migration_v16_to_v17.rs` | **Create new** — six tests: T-V17-01 (fresh DB at v17), T-V17-02 (v16→v17 migration), T-V17-03 (index present), T-V17-04 (idempotency), T-V17-05 (pre-existing rows get phase=None), T-V17-06 (schema_version=17) |
| `crates/unimatrix-store/tests/migration_v15_to_v16.rs` | Update all `assert_eq!(... 16)` → 17; rename `test_current_schema_version_is_16` → `_is_17`; update inline comments |
| `crates/unimatrix-server/src/eval/scenarios/tests.rs` | Update `insert_query_log_row` helper to include `phase` column binding (NULL/None); all 15+ call sites updated via the shared helper |
| `crates/unimatrix-server/src/mcp/knowledge_reuse.rs` | Update `make_query_log` struct literal to include `phase: None` |

### UDS compile fix (no semantic change)

| File | Change |
|------|--------|
| `crates/unimatrix-server/src/uds/listener.rs` | Line 1324: pass `None` as final arg to `QueryLogRecord::new(...)` — compile fix only, no phase semantics |

---

## Data Structures

### SessionState (infra/session.rs) — new field

```rust
// col-028 fields
/// Entry IDs explicitly retrieved by the agent this session.
///
/// Populated by `context_get` (always) and `context_lookup` (single-ID
/// requests only — request-side cardinality, not result-set cardinality).
/// Not populated by briefing, search, write, or mutation tools.
/// In-memory only; reset on register_session; never persisted.
/// First consumer: Thompson Sampling (future feature).
pub confirmed_entries: HashSet<u64>,
```

Initialised to `HashSet::new()` in `register_session`.

### QueryLogRecord (unimatrix-store/src/query_log.rs) — new field

```rust
pub phase: Option<String>,  // col-028: workflow phase at query time; None for UDS rows
```

### AnalyticsWrite::QueryLog (unimatrix-store/src/analytics.rs) — new variant field

```rust
QueryLog {
    // ... existing eight fields unchanged ...
    phase: Option<String>,   // NEW — col-028
}
```

### Access weight table (post-feature state)

| Tool | access_weight |
|------|--------------|
| `context_search` | 1 (unchanged) |
| `context_lookup` | 2 (unchanged) |
| `context_get` | 2 (changed from 1) |
| `context_briefing` | 0 (changed from 1) |

---

## Function Signatures

### current_phase_for_session (mcp/tools.rs, free function)

```rust
pub(crate) fn current_phase_for_session(
    registry: &SessionRegistry,
    session_id: Option<&str>,
) -> Option<String> {
    session_id.and_then(|sid| registry.get_state(sid))
              .and_then(|s| s.current_phase.clone())
}
```

### SessionRegistry::record_confirmed_entry (infra/session.rs)

```rust
pub fn record_confirmed_entry(&self, session_id: &str, entry_id: u64)
```

Follows the synchronous lock-and-mutate pattern of `record_category_store`.

### QueryLogRecord::new (unimatrix-store/src/query_log.rs) — updated signature

```rust
pub fn new(
    session_id: String,
    query_text: String,
    entry_ids: &[u64],
    similarity_scores: &[f64],
    retrieval_mode: &str,
    source: &str,
    phase: Option<String>,   // NEW — col-028, added as final parameter
) -> Self
```

### D-01 guard in record_briefing_usage (services/usage.rs)

```rust
fn record_briefing_usage(&self, entry_ids: &[u64], ctx: UsageContext) {
    // D-01 guard (col-028): weight-0 is an offer-only event.
    // Must appear before filter_access to avoid burning the dedup slot.
    // EC-04 contract enforcement: access_count is NOT incremented for briefing.
    if ctx.access_weight == 0 {
        return;
    }
    // ... existing body unchanged ...
}
```

### Migration SQL — v16→v17 (unimatrix-store/src/migration.rs)

```rust
if current_version < 17 {
    let has_phase_column: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM pragma_table_info('query_log') WHERE name = 'phase'",
    )
    .fetch_one(&mut **txn).await
    .map(|count| count > 0)
    .unwrap_or(false);

    if !has_phase_column {
        sqlx::query("ALTER TABLE query_log ADD COLUMN phase TEXT")
            .execute(&mut **txn).await
            .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
    }

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_query_log_phase ON query_log (phase)",
    )
    .execute(&mut **txn).await
    .map_err(|e| StoreError::Migration { source: Box::new(e) })?;

    sqlx::query("UPDATE counters SET value = 17 WHERE name = 'schema_version'")
        .execute(&mut **txn).await
        .map_err(|e| StoreError::Migration { source: Box::new(e) })?;
}
```

### analytics.rs INSERT (atomic change unit — SR-01)

```sql
INSERT INTO query_log
    (session_id, query_text, ts, result_count,
     result_entry_ids, similarity_scores, retrieval_mode, source, phase)
VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
```

`.bind(phase)` appended as the ninth bind; existing eight binds unchanged.

### query_log.rs SELECT (both scan functions — atomic with INSERT)

```sql
SELECT query_id, session_id, query_text, ts, result_count,
       result_entry_ids, similarity_scores, retrieval_mode, source, phase
FROM query_log WHERE ...
```

`row_to_query_log` reads index 9:
```rust
phase: row.try_get::<Option<String>, _>(9)
          .map_err(|e| StoreError::Database(e.into()))?,
```

---

## Constraints

| ID | Constraint |
|----|-----------|
| C-01 | Phase snapshot must be the first statement in each handler body, before any `.await`. `get_state` returns a `Clone`; no lock is held across an `await`. |
| C-02 | `pragma_table_info` pre-check is mandatory for every ADD COLUMN migration in this codebase. `ALTER TABLE ADD COLUMN IF NOT EXISTS` is not supported by SQLite. |
| C-03 | D-01 guard must be in `record_briefing_usage`, before `filter_access`. Moving the guard to the `AccessSource` dispatch level is out of scope and requires a separate ADR. |
| C-04 | Single `get_state` call per handler invocation. At `context_search`, the same snapshot variable serves both `UsageContext.current_phase` and `QueryLogRecord.phase`. Two separate calls are prohibited (SR-06 correctness requirement). |
| C-05 | `phase` column added as last positional parameter (`?9`). No existing bind index may change. |
| C-06 | `w_phase_explicit` remains 0.0. No changes to the scoring pipeline or re-ranking formula. |
| C-07 | No consumer of `confirmed_entries` in this feature. Implementing a consumer is a scope variance. |
| C-08 | `uds/listener.rs:1324` is a compile-fix only — pass `None` for phase. No UDS phase semantics. |
| C-09 | The analytics.rs INSERT, both `scan_query_log_*` SELECT statements, and `row_to_query_log` are a single atomic change unit. They must be modified in the same commit (SR-01 enforcement). |
| C-10 | SPECIFICATION.md §FR-02 pseudocode uses invalid Rust `?` syntax for `Option<String>`. Use the §Exact Signatures version (`and_then` chaining) verbatim. |

---

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `sqlx` | Existing | SQLite access via `rusqlite` (bundled); parameterized binding for migration SQL and analytics INSERT |
| `tokio` | Existing | Async runtime; `spawn_blocking` in analytics drain |
| `std::collections::HashSet` | Existing | `confirmed_entries: HashSet<u64>` and existing `signaled_entries` |
| `SessionRegistry::get_state` | Existing internal | Returns `Clone` of `SessionState`; used by `current_phase_for_session` |
| `SessionRegistry::record_category_store` | Existing internal pattern | `record_confirmed_entry` follows this lock-and-mutate pattern |
| `UsageContext.current_phase` | Existing field | Already declared in `services/usage.rs`; this feature populates it for read-side tools |
| `UsageDedup.filter_access` | Existing internal | D-01 guard must precede this call in `record_briefing_usage` |
| `AnalyticsWrite::QueryLog` | Existing variant | Gains `phase: Option<String>` field |
| `enqueue_analytics` | Existing | Analytics drain channel; unchanged except variant gains new field |
| `make_state_with_rework` test helper | Existing | Must be updated per pattern #3180 |
| `migration_v15_to_v16.rs` test file | Existing | Pattern to follow for new `migration_v16_to_v17.rs` |

---

## NOT in Scope

- Changes to the scoring pipeline or `w_phase_explicit` (remains 0.0 per ADR-003).
- Any consumer of `confirmed_entries` — Thompson Sampling is a separate feature.
- Phase-conditioned frequency table (ass-032 Loop 2).
- Thompson Sampling per-(phase, entry) arms.
- Gap detection.
- Backfill of historical `query_log` rows — pre-existing rows get `phase = NULL`.
- Phase capture for `context_correct`, `context_deprecate`, `context_quarantine`.
- Phase semantics for the UDS `insert_query_log` call site — compile fix only (`phase: None`).
- Persistence of `confirmed_entries` — in-memory only, reset on session registration.
- Moving the D-01 guard to the `AccessSource` dispatch level (SR-07 risk noted; deferred to separate ADR).

---

## Alignment Status

From ALIGNMENT-REPORT.md (reviewed 2026-03-26):

| Check | Status |
|-------|--------|
| Vision Alignment | PASS — directly enables Wave 1A session-conditioned intelligence; resolves read-side phase blindness blocking ass-032 Loop 2, Thompson Sampling, and gap detection |
| Milestone Fit | PASS — correctly positioned post-crt-025 (WA-1, COMPLETE), pre-ass-032/Thompson Sampling |
| Scope Gaps | PASS — all 20 SCOPE.md ACs addressed |
| Scope Additions | WARN — SPECIFICATION.md adds AC-21 through AC-24 beyond SCOPE.md's 20 criteria |
| Architecture Consistency | PASS — six components coherent; ADR decisions consistent with codebase patterns |
| Risk Completeness | PASS — 16 risk items, integration risks, edge cases, security risks, failure modes covered |

**WARN accepted**: AC-21 through AC-24 are risk mitigations responding to SR-01 through SR-04
from SCOPE-RISK-ASSESSMENT.md. AC-23 (UDS compile fix) is a mandatory mechanical consequence of the
`QueryLogRecord::new` signature change scoped in SCOPE.md; it is not truly additive. AC-21, AC-22,
and AC-24 add no new behaviour — they are atomicity obligations, a pre-gate grep check, and a doc
comment. No product owner resolution required; all four are included in this brief's scope.

---

## Delivery Gate Checklist

The following are code-review gates (not automatable) that delivery must verify before PR:

1. **AC-12**: `current_phase_for_session` is the first statement in each of the four handler bodies, before any `.await`.
2. **AC-21**: analytics.rs INSERT, both `scan_query_log_*` SELECTs, and `row_to_query_log` modified in the same commit.
3. **AC-22**: `grep -r 'schema_version.*== 16' crates/` returns zero matches.
4. **AC-23**: `cargo build --workspace` compiles without error.
5. **AC-24**: `confirmed_entries` field carries its full doc comment per SPECIFICATION.md §Exact Signatures.

Minimum automated test gate: AC-07 (D-01 guard integration test), AC-17 (phase round-trip read-back via real analytics drain), `cargo test --workspace` green.
