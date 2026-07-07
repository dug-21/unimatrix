# Component: Audit op-list update

**Crate/file:** `unimatrix-store/src/audit.rs:79-92` (the `audit_write_count_since` query op-list at `:84`).
**ADR:** ADR-009 (final bullet). **FR:** FR-09. **Risk:** R-07 (latent budget signal voided if missed).

## Purpose

Add `'context_tag'` to the operation allow-list of `audit_write_count_since` so `context_tag`
mutations are counted by the persistent per-agent write counter. This is a **latent, non-enforcing**
signal (future SLN1 budget) — NOT a live throttle. The live throttle is `check_write_rate` in the
service (see store-tag-service.md). Do NOT wire this counter into any enforcement path.

## Change (one-line SQL edit)

Current (`audit.rs:79-92`):

```
pub async fn audit_write_count_since(&self, agent_id: &str, since: u64) -> Result<u64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log
         WHERE agent_id = ?1 AND timestamp >= ?2
         AND operation IN ('context_store', 'context_correct')",     // ← add 'context_tag'
    )
    ...
}
```

After:

```
        "SELECT COUNT(*) FROM audit_log
         WHERE agent_id = ?1 AND timestamp >= ?2
         AND operation IN ('context_store', 'context_correct', 'context_tag')",
```

Also update the doc-comment at `audit.rs:77-78` ("Only counts `context_store` and `context_correct`
operations.") to include `context_tag`.

## Data flow

Input: `agent_id`, `since` (unchanged). Output: `u64` count now including `operation="context_tag"`
rows. Bound parameters unchanged (`?1`, `?2`). No schema change, no new column, no index change.

## Error handling

Unchanged — `StoreError::Database(e.into())` on query failure.

## Key test scenarios (hints)

1. **Inclusion (R-07/FR-09/AC-06b):** log N `context_tag` audit events for `agent-a`; assert
   `audit_write_count_since("agent-a", 0) == N` (extends the existing `test_write_count_since_*`
   fixtures in `infra/audit.rs` — test infra is cumulative, do not scaffold anew).
2. **Mixed ops:** `context_store` + `context_correct` + `context_tag` for one agent all count;
   `context_search`/`context_lookup`/etc. still excluded (extend
   `test_write_count_since_non_write_ops_excluded`).
3. **Not a live throttle:** no assertion that exceeding a count blocks a write — there is no
   enforcement consumer (guard against a tester writing a rejection path that does not ship).
