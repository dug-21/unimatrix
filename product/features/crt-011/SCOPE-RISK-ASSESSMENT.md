# crt-011: Scope Risk Assessment

## Feature Context

**Feature:** crt-011 — Confidence Signal Integrity
**Scope:** Fix session count over-counting in signal consumers (#75), add handler-level integration tests (#32)
**Crates:** unimatrix-server (primary), unimatrix-observe (type definitions), unimatrix-store (signal queue), unimatrix-engine (confidence formula — read-only)

---

## Identified Scope Risks

### SR-01: Per-Session Dedup Key Requires String Cloning

**Risk Level:** LOW
**Category:** Performance

The per-session dedup uses `HashSet<(String, u64)>` keyed on `(session_id, entry_id)`. Each signal's `session_id` is a `String` that must be cloned into the HashSet key. In typical operation, `drain_signals` returns a small number of signals (1-5), so the cloning cost is negligible. However, if a large backlog accumulates (e.g., server restart with many stale sessions), the number of signals could be larger.

**Mitigation:** The signal queue has a 10,000-record cap (signal.rs:120). Even in the worst case, the HashSet contains at most 10,000 entries with short session_id strings. This is well within acceptable memory bounds.

### SR-02: Race Condition Between First Pass and Second Pass in run_confidence_consumer

**Risk Level:** MEDIUM
**Category:** Correctness

The current `run_confidence_consumer` Step 4 has a three-pass structure (lines 1415-1460): first pass under lock, fetch outside lock, third pass under lock. Between the first and third passes, another thread could modify `PendingEntriesAnalysis`. The existing code handles this with "Added between our first pass and now" logic (line 1446), but the dedup fix must account for this: the dedup HashSet must be maintained across all three passes to prevent the "already added" case from double-counting.

**Mitigation:** The dedup HashSet is populated during the first pass (entries already counted) and checked during the third pass (new entries). Since the HashSet tracks `(session_id, entry_id)` pairs seen across ALL passes, the race is handled correctly. Architecture should specify this clearly.

### SR-03: Integration Test Setup Complexity

**Risk Level:** MEDIUM
**Category:** Feasibility

Handler-level integration tests require constructing a `UnimatrixServer` with all its dependencies (Store, VectorIndex, EmbedPipeline, services, UsageDedup, SessionRegistry). If existing test infrastructure does not support this, significant scaffolding may be needed, conflicting with the "extend existing test infrastructure" constraint.

**Mitigation:** Review existing test patterns in `crates/unimatrix-server/src/server.rs` (which has 1900+ lines including tests). The `server.rs` tests already construct `UnimatrixServer` instances. The integration tests should follow the same pattern. If handler-level tests (at the MCP transport layer) are too complex, testing at the `UsageService` level may satisfy the intent of #32 while staying practical.

### SR-04: Semantic Ambiguity Between rework_flag_count and rework_session_count

**Risk Level:** LOW
**Category:** Documentation / Future Maintenance

The decision to NOT dedup `rework_flag_count` while deduping `rework_session_count` creates a subtle semantic distinction that future contributors may not understand. Both fields are incremented in the same loop (listener.rs:1510-1511), but with different dedup behavior.

**Mitigation:** ADR documenting the distinction. Code comments at the increment site. Both the architecture doc and code should make clear that `rework_flag_count` is an event counter (how many times flagged) while `rework_session_count` is a session counter (how many unique sessions flagged this entry).

---

## Risk Summary

| ID | Risk | Level | Architect Action |
|----|------|-------|-----------------|
| SR-01 | String cloning in dedup HashSet | LOW | Accept — bounded by signal queue cap |
| SR-02 | Three-pass race in confidence consumer | MEDIUM | Specify dedup HashSet lifecycle across all passes |
| SR-03 | Integration test setup complexity | MEDIUM | Identify existing test patterns; consider UsageService-level as fallback |
| SR-04 | Semantic ambiguity in rework counters | LOW | ADR + code comments |

## Top 3 Risks for Architect Attention

1. **SR-02:** The three-pass structure in `run_confidence_consumer` must maintain dedup state correctly across lock/unlock boundaries.
2. **SR-03:** Integration test feasibility depends on existing test infrastructure; architect should specify the test boundary.
3. **SR-04:** The rework_flag_count vs rework_session_count distinction needs explicit documentation to prevent future confusion.
