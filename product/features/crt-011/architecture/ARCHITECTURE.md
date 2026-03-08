# crt-011: Architecture — Confidence Signal Integrity

## Overview

This feature fixes session count over-counting in the signal consumer pipeline and adds handler-level integration tests for the confidence path. The changes are confined to `unimatrix-server` (consumer functions + new tests). No schema changes, no new crates, no changes to the confidence formula.

## Architecture Decisions

### ADR-001: Per-Session Dedup Using (session_id, entry_id) HashSet

**Context:** `run_confidence_consumer` and `run_retrospective_consumer` iterate over all drained signals and increment session counts without deduplication. The same entry can appear in multiple signals from the same session (e.g., stale sweep + normal close).

**Decision:** Introduce a `HashSet<(String, u64)>` keyed on `(session_id, entry_id)` in both consumer functions. Before incrementing `success_session_count` or `rework_session_count`, check if the pair has already been counted. If so, skip the increment.

**Rationale:**
- Per-session (not global) dedup preserves the correct semantic: entry X used in sessions A and B counts as 2 sessions.
- `SignalRecord` already carries `session_id`, so no new data is needed.
- The signal queue cap (10,000 records) bounds the worst-case HashSet size.
- This mirrors the existing `HashSet<u64>` dedup for `helpful_count` in Step 2 of `run_confidence_consumer`.

**Consequences:**
- String cloning for session_id keys — acceptable given small drain batch sizes.
- The dedup HashSet must persist across the three-pass structure in `run_confidence_consumer` (first pass under lock, fetch outside lock, third pass under lock).

### ADR-002: rework_flag_count Remains Un-Deduplicated

**Context:** `rework_flag_count` and `rework_session_count` are both incremented in the same loop in `run_retrospective_consumer`. The fix deduplicates `rework_session_count` but must decide about `rework_flag_count`.

**Decision:** Do NOT deduplicate `rework_flag_count`. It is an event counter (number of times an entry was flagged for rework), not a session counter. It is used as a severity/priority signal for `PendingEntriesAnalysis` cap eviction — entries with the lowest `rework_flag_count` are dropped first when the 1000-entry cap is reached.

**Rationale:**
- The field name distinguishes it from `rework_session_count`.
- Its downstream consumer (cap eviction in `PendingEntriesAnalysis::upsert`) uses it as a priority signal. Higher values = more problematic = keep for analysis.
- Deduplicating would lose information about how many times an entry was flagged within a drain cycle.

**Consequences:**
- Code comments must clearly document the semantic distinction at the increment site.
- Both fields are incremented in the same loop but with different dedup behavior — clarity is critical.

### ADR-003: Integration Tests at UsageService Level

**Context:** #32 requests handler-level integration tests for the confidence path. The question is what "handler level" means: (a) MCP transport-level tool dispatch, or (b) service-level calls that the handlers delegate to.

**Decision:** Write integration tests at the `UsageService` level, exercising `record_usage_for_entries_mcp()` and `record_access_fire_and_forget()`. These tests construct a `UsageService` with real `Store` and `UsageDedup` dependencies, then verify the full service-to-store chain including dedup and confidence recomputation.

Additionally, write tests at the `UnimatrixServer` level using the existing `make_server()` helper to exercise `record_usage_for_entries()` through the server's public API, which is what the MCP tool handlers call.

**Rationale:**
- Existing test infrastructure (`make_server()`, `insert_test_entry()`) already supports `UnimatrixServer`-level testing.
- `UsageService` is the service layer that encapsulates the handler's business logic. Testing here covers the handler-to-service-to-store chain without requiring MCP transport setup.
- Pure MCP transport tests (stdin/stdout JSON-RPC) would require significant new scaffolding and are not proportionate to the risk.

**Consequences:**
- Tests do not exercise MCP JSON-RPC deserialization or tool dispatch routing. This is acceptable because those paths are covered by existing param deserialization tests.

## Component Changes

### Modified: `unimatrix-server/src/uds/listener.rs`

#### `run_confidence_consumer`

**Before:** Step 4 iterates `for signal in &signals { for &entry_id in &signal.entry_ids { ... } }` and unconditionally increments `success_session_count`.

**After:** A `HashSet<(String, u64)>` tracks counted `(session_id, entry_id)` pairs. The HashSet is populated across all three passes:

```
Pass 1 (under lock): For each (session_id, entry_id), check/insert into HashSet.
  If already seen → skip. If new AND entry exists in pending → increment.
  Collect entry_ids needing fetch.

Fetch (outside lock): Fetch metadata for unknown entry_ids.

Pass 3 (under lock): For each fetched entry_id, check HashSet again.
  If entry was added between passes → check HashSet, increment if pair is new.
  If entry still missing → insert new EntryAnalysis, mark pair in HashSet.
```

#### `run_retrospective_consumer`

**Before:** Step 4 iterates and unconditionally increments both `rework_flag_count` and `rework_session_count`.

**After:** A `HashSet<(String, u64)>` tracks counted `(session_id, entry_id)` pairs for `rework_session_count` only. `rework_flag_count` continues to increment unconditionally.

### New Tests: Integration Coverage

Location: `crates/unimatrix-server/src/services/usage.rs` (extend existing test module) and `crates/unimatrix-server/src/server.rs` (extend existing test module).

**UsageService tests:**
1. `test_mcp_usage_confidence_recomputed` — call `record_usage_for_entries_mcp`, verify confidence changes.
2. `test_mcp_usage_dedup_prevents_double_access` — same agent+entry, verify access_count stays at 1.

**Server-level tests:**
3. `test_confidence_path_search_to_store` — insert entry, call `record_usage_for_entries`, verify access_count + confidence.
4. `test_confidence_path_dedup_across_calls` — two calls with same agent+entry, verify dedup.

**Consumer tests (new):**
5. `test_confidence_consumer_dedup_same_session` — insert two signals with overlapping entry_ids and same session_id, run consumer, verify `success_session_count` increments once per unique entry.
6. `test_confidence_consumer_different_sessions` — insert signals from two different sessions with overlapping entry_ids, verify `success_session_count` increments twice (once per session).
7. `test_retrospective_consumer_rework_dedup` — same pattern for `rework_session_count`.
8. `test_retrospective_consumer_flag_count_not_deduped` — verify `rework_flag_count` increments for each signal regardless of dedup.

## Integration Surface

No changes to public API, MCP tool signatures, or inter-crate interfaces. The changes are internal to `unimatrix-server`:
- `run_confidence_consumer` — internal function in `uds/listener.rs`
- `run_retrospective_consumer` — internal function in `uds/listener.rs`
- New tests extend existing test modules

## Data Flow (After Fix)

```
Session Close
  → write_signals_to_queue (1 SignalRecord per session)
  → SIGNAL_QUEUE (SQLite)
  → drain_signals (batch of SignalRecords)
  → run_confidence_consumer:
      Step 2: HashSet<u64> for helpful_count dedup (existing, correct)
      Step 3: record_usage_with_confidence (existing, correct)
      Step 4: HashSet<(session_id, entry_id)> for success_session_count dedup (NEW)
  → run_retrospective_consumer:
      HashSet<(session_id, entry_id)> for rework_session_count dedup (NEW)
      rework_flag_count: no dedup (intentional)
  → PendingEntriesAnalysis
  → context_retrospective handler drains on call
```
