# crt-011: Specification — Confidence Signal Integrity

## Feature Summary

Fix session count over-counting in `run_confidence_consumer` and `run_retrospective_consumer` by introducing per-session deduplication. Add integration tests covering the handler-to-service-to-store confidence path.

## Domain Model

### Entities

| Entity | Location | Description |
|--------|----------|-------------|
| `SignalRecord` | `unimatrix-store/src/signal.rs` | Queue record: session_id, entry_ids, signal_type, signal_source |
| `EntryAnalysis` | `unimatrix-observe/src/types.rs` | Per-entry analysis: rework_flag_count, injection_count, success_session_count, rework_session_count |
| `PendingEntriesAnalysis` | `unimatrix-server/src/server.rs` | HashMap<u64, EntryAnalysis> with 1000-entry cap, mutex-protected |
| `UsageDedup` | `unimatrix-server/src/infra/usage_dedup.rs` | In-memory MCP-level dedup for access, votes, co-access |
| `UsageService` | `unimatrix-server/src/services/usage.rs` | Service layer wrapping store + dedup for usage recording |

### Counter Semantics

| Counter | Dedup? | Semantic | Source |
|---------|--------|----------|--------|
| `success_session_count` | YES — per (session_id, entry_id) | Number of distinct sessions where entry was used successfully | run_confidence_consumer |
| `rework_session_count` | YES — per (session_id, entry_id) | Number of distinct sessions where entry was flagged for rework | run_retrospective_consumer |
| `rework_flag_count` | NO | Total number of rework flagging events (severity signal) | run_retrospective_consumer |
| `helpful_count` | YES — per unique entry_id (existing) | Number of implicit helpful signals | run_confidence_consumer Step 2-3 |
| `access_count` | YES — per (agent_id, entry_id) via UsageDedup | Number of distinct agent accesses | UsageService |

## Functional Requirements

### FR-01: success_session_count Dedup in run_confidence_consumer

**Pre-condition:** `drain_signals(SignalType::Helpful)` returns N signals, possibly with overlapping entry_ids and/or duplicate session_ids.

**Behavior:**
1. Construct a `HashSet<(String, u64)>` to track counted `(session_id, entry_id)` pairs.
2. In Pass 1 (under lock), for each signal and each entry_id:
   - If `(signal.session_id, entry_id)` is already in the HashSet, skip.
   - Otherwise, insert the pair into the HashSet and increment `success_session_count` if the entry exists in `PendingEntriesAnalysis`.
3. In Pass 3 (under lock), for fetched entries:
   - Check the HashSet before incrementing. Only increment if the `(session_id, entry_id)` pair is new.

**Post-condition:** Each unique `(session_id, entry_id)` pair increments `success_session_count` exactly once per drain cycle.

### FR-02: rework_session_count Dedup in run_retrospective_consumer

**Pre-condition:** `drain_signals(SignalType::Flagged)` returns N signals.

**Behavior:**
1. Construct a `HashSet<(String, u64)>` to track counted `(session_id, entry_id)` pairs.
2. For each signal and each entry_id:
   - If `(signal.session_id, entry_id)` is already in the HashSet, skip the `rework_session_count` increment.
   - Otherwise, insert the pair and increment `rework_session_count`.
   - Always increment `rework_flag_count` regardless of dedup state.

**Post-condition:** Each unique `(session_id, entry_id)` pair increments `rework_session_count` exactly once. `rework_flag_count` increments for every occurrence.

### FR-03: Existing helpful_count Dedup Preserved

**Constraint:** The existing `HashSet<u64>` dedup in Step 2-3 of `run_confidence_consumer` must not be modified or regressed. The fix only changes Step 4.

### FR-04: Handler-Level Integration Tests

**Requirement:** Add tests exercising the confidence path through service-level APIs:

| Test ID | API Under Test | Verifies |
|---------|---------------|----------|
| T-INT-01 | `UsageService::record_usage_for_entries_mcp` | Confidence recomputed after usage |
| T-INT-02 | `UsageService::record_usage_for_entries_mcp` | UsageDedup prevents double access_count |
| T-INT-03 | `UnimatrixServer::record_usage_for_entries` | access_count + confidence updated |
| T-INT-04 | `UnimatrixServer::record_usage_for_entries` | Dedup across repeated calls |

### FR-05: Consumer Dedup Unit Tests

| Test ID | Consumer | Verifies |
|---------|----------|----------|
| T-CON-01 | run_confidence_consumer | Same session, overlapping entry_ids → success_session_count = 1 per entry |
| T-CON-02 | run_confidence_consumer | Different sessions, overlapping entry_ids → success_session_count = 2 per entry |
| T-CON-03 | run_retrospective_consumer | Same session, overlapping entry_ids → rework_session_count = 1 per entry |
| T-CON-04 | run_retrospective_consumer | rework_flag_count increments for every signal (no dedup) |

## Acceptance Criteria Mapping

| AC | FR | Test |
|----|----|----- |
| AC-01 | FR-01 | T-CON-01, T-CON-02 |
| AC-02 | FR-02 | T-CON-03 |
| AC-03 | FR-03 | Existing tests (no regression) |
| AC-04 | FR-01, FR-02 | T-CON-01, T-CON-03 |
| AC-05 | FR-02 | T-CON-04, code comments, ADR-002 |
| AC-06 | FR-04 | T-INT-01 or T-INT-03 |
| AC-07 | FR-04 | T-INT-01 or T-INT-03 (get path uses same service) |
| AC-08 | FR-04 | T-INT-02 or T-INT-04 |
| AC-09 | FR-03 | Full test suite pass |
| AC-10 | FR-01 | T-CON-02 |

## Constraints

- No schema changes to any SQLite tables.
- No modifications to `compute_confidence` in `unimatrix-engine`.
- No changes to `SignalRecord`, `SignalType`, or `SignalSource`.
- No changes to `UsageDedup` (MCP-level dedup is correct and separate).
- All new tests extend existing test modules using existing helpers (`make_server`, `insert_test_entry`).
- No new crate dependencies.

## Error Handling

No new error paths. The dedup HashSet operations (`insert`, `contains`) are infallible. The existing error handling in both consumers (drain failure → warn + return, spawn_blocking failure → warn + continue) is unchanged.

## Performance Impact

Negligible. The dedup HashSet adds O(n) time and O(n) space where n = total (signal, entry) pairs in a drain batch. Typical drain batches contain 1-5 signals with 5-20 entries each. The 10,000-record signal queue cap bounds worst-case.
