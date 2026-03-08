# crt-011: Confidence Signal Integrity

## Problem Statement

The `success_session_count` field in `PendingEntriesAnalysis` is over-counted when multiple signals drain for the same entry in a single consumer run. In `run_confidence_consumer` (listener.rs:1413-1460), Step 4 iterates over ALL drained signals and increments `success_session_count` once per signal per entry_id, rather than once per unique entry_id across all signals. The same bug exists in `run_retrospective_consumer` (listener.rs:1504-1530) for `rework_session_count`.

Since `success_session_count` feeds into the retrospective entries analysis (which flows into observation metrics and ultimately impacts confidence-driven ranking decisions), this corruption silently inflates session counts, making entries appear more heavily used than they are.

Additionally, the handler-to-service-to-store confidence path (#32) lacks integration tests. The existing tests call `Store` methods directly, bypassing the MCP tool handler layer. This means regressions in the handler wiring (parameter parsing, dedup integration, service delegation) would go undetected.

**Who is affected?** Every agent that relies on confidence-ranked search results or briefing content selection. Corrupted session counts distort the observation pipeline that feeds retrospective analysis and ultimately confidence scoring.

**Why now?** This is the first feature in the Intelligence Sharpening milestone. Wave 3 (crt-013: Retrieval Calibration) depends on correct confidence data. Fixing this first is a prerequisite.

## Goals

1. Fix `success_session_count` over-counting in `run_confidence_consumer` so each (session_id, entry_id) pair is counted at most once per consumer drain cycle.
2. Fix `rework_session_count` over-counting in `run_retrospective_consumer` using the same per-session deduplication approach.
3. Add integration tests covering the handler-to-service-to-store confidence path through MCP tool handlers.
4. Verify that `helpful_count` increments (Step 3 of `run_confidence_consumer`) are already correctly deduplicated (they use `HashSet<u64>` in Step 2).

## Non-Goals

- **Not changing the 6-factor confidence formula.** The formula itself (`compute_confidence` in `unimatrix-engine/src/confidence.rs`) is correct. Only the signal ingestion path is broken.
- **Not adding new confidence factors.** No changes to weights, Wilson score parameters, or co-access affinity.
- **Not fixing `UsageDedup` (in-memory MCP-level dedup).** `UsageDedup` handles per-agent-per-entry access/vote dedup for MCP tool calls. It is correct and unrelated to the signal queue consumer bug.
- **Not modifying the `SIGNAL_QUEUE` schema or `SignalRecord` format.** The queue structure is fine; the bug is in the consumer logic.
- **Not addressing observation metrics normalization (#103 / nxs-009).** That is a separate feature.
- **Not changing `SessionRegistry` or session lifecycle persistence.** The session infrastructure works correctly; the bug is downstream in signal consumption.

## Background Research

### The Over-Counting Bug (#75)

**Root cause:** In `run_confidence_consumer` (listener.rs:1364-1461), `drain_signals(SignalType::Helpful)` can return multiple `SignalRecord`s — one per session that closed since the last drain. Each `SignalRecord` contains an `entry_ids: Vec<u64>` list. Step 4 iterates `for signal in &signals { for &entry_id in &signal.entry_ids { ... } }`, incrementing `success_session_count` for each occurrence. If the same entry_id appears in signals from different sessions, this is correct (one count per session). But the first pass/second pass structure (lines 1415-1428 and 1440-1460) has a subtlety: entries fetched in the second pass that were already added between passes get double-counted (line 1446: `existing.success_session_count += 1` in the "Added between our first pass and now" case).

However, the **primary** over-counting path is: if the same entry appears in multiple signals from the SAME session (e.g., stale session sweep + normal close, or multiple stale sweeps), `success_session_count` increments multiple times for what should be a single session.

**Impact:** The `PendingEntriesAnalysis.entries` HashMap flows into `EntryAnalysis` records that are merged via `server.rs:57-63` and then consumed by the retrospective pipeline for observation metrics. Over-counted session counts distort the metrics used for hotspot detection and entry health assessment.

### The rework_session_count Bug (same pattern)

In `run_retrospective_consumer` (listener.rs:1504-1530), the same pattern exists: iterating over signals and entry_ids without deduplication. `rework_session_count` and `rework_flag_count` increment for every signal/entry combination.

### Existing Dedup for helpful_count

Step 2-3 of `run_confidence_consumer` (lines 1382-1411) correctly deduplicate `helpful_count` increments: a `HashSet<u64>` collects unique entry_ids across all signals, then `record_usage_with_confidence` is called once with the unique set. This path is correct.

### Handler-Level Test Gap (#32)

Current confidence integration tests (`crates/unimatrix-store/tests/sqlite_parity.rs`) call `store.record_usage_with_confidence()` directly. No tests exercise the full path:
1. MCP tool handler receives `context_search`/`context_get` request
2. Handler calls `UsageService::record_usage_for_entries_mcp()`
3. `UsageService` applies `UsageDedup` filtering
4. `UsageService` calls `store.record_usage_with_confidence()`
5. Confidence is recomputed and stored

The handler→service→store chain has no integration coverage, meaning broken wiring between layers would not be caught.

### Crate Boundaries

| Crate | Role in Confidence Path |
|-------|------------------------|
| `unimatrix-observe` | Defines `EntryAnalysis` struct with `success_session_count` / `rework_session_count` |
| `unimatrix-store` | Signal queue persistence (`insert_signal`, `drain_signals`), usage recording (`record_usage_with_confidence`) |
| `unimatrix-engine` | Confidence formula (`compute_confidence`), scoring constants |
| `unimatrix-server` | Signal consumers (`run_confidence_consumer`, `run_retrospective_consumer`), `UsageService`, `UsageDedup`, MCP tool handlers |

## Proposed Approach

### Fix 1: Deduplicate session counts in `run_confidence_consumer`

In Step 4, track which `(session_id, entry_id)` pairs have already been counted using a `HashSet<(String, u64)>`. Only increment `success_session_count` for pairs not yet seen in this drain cycle. Each `SignalRecord` carries a `session_id`, so the key is available. This preserves the correct semantic: if entry X appeared in sessions A and B, it counts twice (once per session). But if entry X appears in two signals from the SAME session, it counts once.

### Fix 2: Deduplicate session counts in `run_retrospective_consumer`

Same approach: use a `HashSet<(String, u64)>` to track which `(session_id, entry_id)` pairs have had `rework_session_count` incremented. Only increment once per unique pair per drain cycle.

`rework_flag_count` is intentionally NOT deduplicated. It counts individual rework flagging events (not sessions) and serves as a severity/priority signal for `PendingEntriesAnalysis` cap eviction. Higher values mean "keep this entry for analysis." This semantic distinction between `rework_flag_count` (event counter) and `rework_session_count` (session counter) will be documented in an ADR.

### Fix 3: Handler-level integration tests

Write integration tests in `crates/unimatrix-server/` that exercise the MCP tool handler → `UsageService` → `Store` confidence path. Tests should:
- Call `context_search` or `context_get` via the MCP handler
- Verify that `access_count`, `helpful_count`, `confidence` are updated correctly
- Verify that `UsageDedup` prevents double-counting within a session
- Verify confidence recomputation occurs after counter updates

Use the existing test infrastructure (`TestServiceContext` or equivalent) to avoid creating isolated test scaffolding.

## Acceptance Criteria

- AC-01: `success_session_count` increments at most once per unique `(session_id, entry_id)` pair per `run_confidence_consumer` drain cycle. Two different sessions containing the same entry correctly increment by 2.
- AC-02: `rework_session_count` increments at most once per unique `(session_id, entry_id)` pair per `run_retrospective_consumer` drain cycle.
- AC-03: `helpful_count` dedup in `run_confidence_consumer` Step 2-3 remains correct and is not regressed by changes.
- AC-04: A unit test reproduces the over-counting bug (multiple signals with overlapping entry_ids) and verifies the fix.
- AC-05: `rework_flag_count` is NOT deduplicated (it counts flagging events, not sessions). The semantic distinction between `rework_flag_count` and `rework_session_count` is documented in an ADR and in code comments.
- AC-06: At least one integration test exercises the full `context_search` handler → `UsageService` → `Store` → confidence recomputation path.
- AC-07: At least one integration test exercises the `context_get` handler → `UsageService` → `Store` → confidence recomputation path.
- AC-08: Integration tests verify `UsageDedup` prevents double access_count increment for the same agent+entry within a session.
- AC-09: All existing tests pass with no regressions.
- AC-10: The fix handles the edge case where `drain_signals` returns signals from multiple sessions with overlapping entry_ids (each unique `(session_id, entry_id)` pair increments once; different sessions correctly count separately).

## Constraints

- **No schema changes.** The `SIGNAL_QUEUE`, `entries`, and `sessions` tables remain unchanged.
- **No crate boundary changes.** The fix is contained within `unimatrix-server` (consumer functions) and new tests.
- **Extend existing test infrastructure.** Per CLAUDE.md: "Test infrastructure is cumulative — extend existing fixtures and helpers, never create isolated scaffolding."
- **No changes to `compute_confidence` formula.** The formula in `unimatrix-engine` is correct; only the signal ingestion is broken.
- **Backward-compatible.** Already-corrupted session counts in existing databases are not retroactively fixed (no migration needed; the counts only exist in-memory `PendingEntriesAnalysis`).

## Resolved Questions

1. **`rework_flag_count` should NOT be deduplicated.** Research confirmed it counts individual rework flagging events, not unique sessions. It is named "rework **flag** count" (distinct from `rework_session_count`). It serves as a severity/priority signal in `PendingEntriesAnalysis` cap eviction (server.rs:66-70): entries with the lowest `rework_flag_count` are dropped first. Higher values = more problematic = keep for analysis. Deduplicating would lose signal.
2. **Per-session dedup for session counts (DECIDED).** Each unique `(session_id, entry_id)` pair should count once. If entry X appears in signals from sessions A and B, `success_session_count` increments by 2 (once per session). Global dedup would be wrong -- we want to know how many sessions used the entry.

## Open Questions

None remaining.

## Tracking

- GitHub Issue: #136 (https://github.com/dug-21/unimatrix/issues/136)
- Related: #75 (session over-counting bug), #32 (missing handler-level integration tests)
