# Test Plan — compaction-INSERT helper + failure counter (`store_ops`)

**Component**: `fn insert_compaction_event(&self, session_id: &str, compacted_at_secs: i64, high_water: i64) -> Result<()>` — a thin single-statement **autocommit** INSERT on `store_ops` (no explicit transaction), parameterized. On failure: increment the named counter `compaction_events_insert_failed`, log ids/counts only (no content), return `Err`.
**Pseudocode**: `pseudocode/compaction-insert-helper.md` · **Layer**: unit + integration (fault-injection).
**Anchor ACs**: **AC-04a** (named counter, non-blocking, content-free). **Risks**: **R-15 (High)**, R-03 (shared), security (parameterized INSERT).

## Happy path — unit/integration

`crates/unimatrix-store/` tests (alongside the store_ops tests).

1. `test_insert_compaction_event_writes_row` — Act: `insert_compaction_event("sess-1", 1_700_000_000, 4096)`. Assert: `Ok(())`, one row with the exact values; `compacted_at` stored as the passed seconds value (no implicit unit conversion in the helper).
2. `test_insert_is_autocommit_no_explicit_transaction` — structural: a single statement, no `BEGIN`/explicit txn wrapping (ADR-007). One autocommit INSERT.
3. `test_insert_is_parameterized` — security: `session_id` is bound as a parameter, never string-interpolated (no SQL-injection surface). A `session_id` containing SQL metacharacters round-trips as data.

## Failure path (AC-04a, R-15) — INTEGRATION, fault-injection MANDATORY

4. `test_insert_failure_increments_named_counter` — **MANDATORY.** Arrange: inject a store INSERT failure (e.g. a closed/poisoned connection, a fault-injection store handle, or a constraint-violating fixture). Act: call the helper at the seam. Assert: the **named counter** `compaction_events_insert_failed` increments by exactly **1** — a generic `tracing::warn!` assertion does NOT satisfy this AC (the metric is the load-bearing assertion, ADR-007 §6). Returns `Err`. (R-15, AC-04a.)
5. `test_insert_failure_counter_is_content_free` — the counter (and the accompanying log) carry ids/counts only — NO transcript bytes in the metric label or log line. (ADR-005/§7, R-15 scenario 2, AC-04a.)
6. `test_insert_failure_returns_err_no_panic` — the helper returns `Err` (never panics); the caller (writer) decides non-blocking continuation. (R-15.)

### Negative-mutation (AC-04a)
- A failure path that only logged (no named counter) must fail `test_insert_failure_increments_named_counter`. This is the explicit guard against the prior log-only posture (ADR-007 §6 replaced it).

## Cross-component
- The non-blocking seam behavior (compaction ACK proceeds, handler does not panic) is asserted in compaction-events-writer.md test 9; this file owns the helper-level named-counter + content-free + autocommit assertions.
- Downstream drift-detector (row-derived `compaction_count` vs `increment_compaction`) is **crt-055-owned**, NOT a crt-054 test — crt-054 only guarantees the counter exists and increments so the drift is observable. (R-15 scenario 3.)
