# Security Review: col-009-security-reviewer

**PR**: #74 — col-009: Closed-Loop Confidence
**Reviewer**: Fresh-context security reviewer (spawned cold)
**Date**: 2026-03-02

## Risk Level: low

## Summary

The col-009 implementation is structurally sound. The asymmetric design invariant (auto-positive Helpful, flag-not-downweight Flagged, zero implicit touches to `unhelpful_count`) is correctly enforced across all code paths. Session-scoped dedup, atomicity via single lock acquisition, and poison recovery on all Mutex locks are correctly implemented. One functional correctness deficiency was found in the `maintain=true` stale sweep path (signals written to SIGNAL_QUEUE but consumers never triggered), one success_session_count over-counting risk exists in the confidence consumer, and a queue cap enforcement inconsistency allows momentary queue length of 10,001 during each insert. None of these are security vulnerabilities. No unsafe unwraps in production paths, no hardcoded secrets, no injection surface, no privilege escalation risk.

---

## Findings

### Finding 1: Stale Sweep via `maintain=true` Writes Signals But Does Not Drain Consumers

- **Severity**: medium
- **Location**: `/workspaces/unimatrix/crates/unimatrix-server/src/tools.rs:1611-1657`
- **Description**: The `maintain=true` path in `context_status` calls `sweep_stale_sessions()`, writes `SignalRecord`s to SIGNAL_QUEUE via `store_for_sweep.insert_signal()`, but never calls `run_confidence_consumer` or `run_retrospective_consumer`. Signals accumulate in SIGNAL_QUEUE indefinitely until the next `SessionClose` event triggers those consumers. In practice this means: stale sessions swept via `maintain=true` generate signals that are processed at the next SessionClose — which may be for an entirely different session. The functional effect is delayed processing rather than loss, but it contradicts the design intent and could cause the SIGNAL_QUEUE to fill if `maintain=true` is called repeatedly without any SessionClose events (server used MCP-only, no hooks). Spec says stale sweep is wired to "both SessionClose and `maintain=true` in context_status" (SCOPE.md Goal 6), implying full processing including consumer invocation in both cases.
- **Recommendation**: After the spawn_blocking block that writes signals completes, call the consumer functions (or their equivalents for the MCP server context, which lacks `entry_store` as a function parameter at that call site). This requires either passing `entry_store` and `pending_entries_analysis` into the `context_status` handler, or refactoring the consumer into a method on `UnimatrixServer`. Not a security issue, but a functional regression against AC-09.
- **Blocking**: no (existing tests pass; full consumer invocation is a functional gap, not a security fault)

### Finding 2: success_session_count May Over-Count When Same Entry Appears in Multiple Signals in a Single Drain

- **Severity**: low
- **Location**: `/workspaces/unimatrix/crates/unimatrix-server/src/uds_listener.rs:1164-1211`
- **Description**: In `run_confidence_consumer`, Step 2 deduplicates `entry_ids` across all drained signals into `all_entry_ids: HashSet<u64>` for the `helpful_count` increment call (correct — each entry gets one `helpful_count++` regardless of how many sessions signaled it). However, Step 4's `success_session_count` update iterates over `&signals` (the raw signal list, with duplicates preserved) rather than the deduplicated set. If two simultaneous sessions both inject entry #42 and both close with "success", two `Helpful` signals are written containing entry #42. After drain, `all_entry_ids` deduplicates to one increment of `helpful_count` (correct), but the `success_session_count` loop over `&signals` will increment `success_session_count` twice for entry #42 (once per signal). This is a double-count in `PendingEntriesAnalysis.success_session_count` for entries co-injected across multiple sessions in the same drain batch.
- **Recommendation**: Deduplicate the entry_id → increment mapping in Step 4 to match the `all_entry_ids` deduplication already done for `helpful_count`. Or document explicitly that `success_session_count` counts signal occurrences (not unique entries) — but this contradicts the field semantics implied by its name. This is a data quality issue, not a security issue; the `helpful_count` increment (the security-critical counter) is correctly deduplicated.
- **Blocking**: no

### Finding 3: Queue Cap Allows Momentary Over-Count of 10,001

- **Severity**: low
- **Location**: `/workspaces/unimatrix/crates/unimatrix-server/src/db.rs:120-133`
- **Description**: `insert_signal` enforces the 10,000-record cap by: (1) checking `current_len >= 10_000`, (2) deleting the oldest record if so, (3) inserting the new record. The delete and insert are in the same transaction, so after the transaction commits the queue length is at most 10,000. However, the cap deletes exactly one record regardless of how far over 10,000 the queue is. If the queue is somehow already at 10,001 (e.g., due to a race with concurrent inserts on a hypothetically multi-threaded caller), deleting one and inserting one leaves it at 10,001. In practice, redb serializes writes and the Mutex on `Arc<Store>` is not per-method, so concurrent insertion is not possible — this is a cosmetic issue rather than an exploitable race. The SCOPE.md cap policy is "drop oldest records at 10,000", which the code satisfies in the common case.
- **Recommendation**: Document the single-oldest-drop policy explicitly (already partially done in comments). No code change required.
- **Blocking**: no

### Finding 4: Corrupted SignalRecord in drain_signals Silently Discarded Without Logging

- **Severity**: low
- **Location**: `/workspaces/unimatrix/crates/unimatrix-server/src/db.rs:171-175`
- **Description**: In `drain_signals`, corrupted records (deserialization errors) are silently removed from the queue with `keys_to_delete.push(key)` but without logging. The ADR-001 comment says SIGNAL_QUEUE is ephemeral (records deleted after drain), so silent removal is the right policy. However, for operational visibility, a `tracing::warn!` would help distinguish "no signals" from "signals were corrupted and discarded" in production logs.
- **Recommendation**: Add a `tracing::warn!` with the key and error on deserialization failure. Not a security issue.
- **Blocking**: no

### Finding 5: Rework Threshold Logic — MultiEdit Paths in `extract_rework_events_for_multiedit` Always Set `had_failure=false`

- **Severity**: low
- **Location**: `/workspaces/unimatrix/crates/unimatrix-server/src/hook.rs:339-358`
- **Description**: `extract_rework_events_for_multiedit` produces events with `had_failure=false` for every edit in a MultiEdit call (the comment says "Edit tools can't fail"). This is correct for the Edit tool. However, it means MultiEdit can never contribute to the `failure_since_last_edit` state in `has_crossed_rework_threshold`. A session doing MultiEdit → Bash(fail) → MultiEdit → Bash(fail) → MultiEdit → Bash(fail) → MultiEdit on the same file would not register as rework, because the MultiEdit events don't have `had_failure=true` between them. The rework threshold evaluates `failure_since_last_edit` only when it encounters a Bash event with `had_failure=true`, but MultiEdit entries never set that flag. In practice, Edit-fail-edit cycles dominate rework patterns and this is conservative by design (ADR-002), so this is a known tradeoff, not a bug.
- **Recommendation**: Confirm with ADR-002 intent that MultiEdit-only rework cycles are intentionally excluded. If so, document in the function comment. No code change required.
- **Blocking**: no

---

## Asymmetric Design Invariant Verification

Critical check: `unhelpful_count` must never be touched by implicit signals.

Verified:
- `record_usage_with_confidence` in `run_confidence_consumer` (line 1146-1154) passes `&entry_ids_vec` as `helpful_ids` and `&[]` for all other parameters including unhelpful_ids. Correct.
- `run_retrospective_consumer` writes only to `PendingEntriesAnalysis` (in-memory `EntryAnalysis` counters). It never calls `record_usage_with_confidence`. Correct.
- `Flagged` signal path: `write_signals_to_queue` writes `SignalType::Flagged` records. `run_confidence_consumer` drains only `SignalType::Helpful`. `run_retrospective_consumer` drains only `SignalType::Flagged`. Cross-contamination between consumers is impossible by construction. Correct.
- `unhelpful_count` increment pathway: only reachable via explicit MCP vote (`helpful=false` in `context_search`/`context_lookup` → `record_usage_with_confidence` with non-empty `unhelpful_ids`). The implicit signal pathway never reaches this. Correct.

**Invariant: VERIFIED. Zero regression risk to `unhelpful_count`.**

---

## Session-Scoped Dedup Verification

- `drain_and_signal_session` uses `sessions.remove(session_id)?` — if absent, returns `None` immediately. The session is gone from the registry before the lock is released. Any subsequent call for the same session_id returns `None`. Correct.
- `signaled_entries: HashSet<u64>` on `SessionState`: in `build_signal_output_from_state`, entries already in `signaled_entries` are excluded from `eligible`. However, `signaled_entries` is checked but never written in the current implementation — `build_signal_output_from_state` builds the output from the state but does not append to `signaled_entries` before the session is removed. This is fine because the entire session is removed atomically in the same lock scope (ADR-003). The `signaled_entries` field exists as defense-in-depth for the stale sweep use-case: if the sweep runs before a real SessionClose, the session is already removed, so `drain_and_signal_session` for the real close returns `None`. The `signaled_entries` field is currently unreachable in a meaningful way because any successful signal generation removes the session. This is correct but the field is effectively dead code after removal.
- **AC-03 (idempotent on double SessionClose)**: Verified. First call removes session; second call returns `None`; `process_session_close` branches on `Some(ref output)` so second call is a no-op.

---

## FR-06.2b Compliance Verification

The spec requires: "confidence consumer also increments success_session_count in PendingEntriesAnalysis."

`run_confidence_consumer` steps 4+: increments `success_session_count` for all entry_ids in the drained Helpful signals. Both the "existing entry" (increment) and "new entry" (insert with count=1) paths handle this. The second pass also handles the TOCTOU: if an entry was added between the first pass (lock released) and third pass (lock reacquired), it increments rather than double-inserts. Correct.

Note: as per Finding 2, the count may be inflated when multiple simultaneous sessions signal the same entry. The `helpful_count` increment (the confidence-affecting counter) is correctly deduplicated.

---

## Schema v4 Migration Verification

Migration follows the 3-step pattern:
1. `CURRENT_SCHEMA_VERSION = 4` in `migration.rs` — correct.
2. `migrate_v3_to_v4` opens `SIGNAL_QUEUE` (triggers table creation) and conditionally writes `next_signal_id = 0` if absent — idempotent, correct.
3. `migrate_if_needed` calls `migrate_v3_to_v4` when `current_version <= 3`, then writes `schema_version = 4` in the same transaction — correct.

The migration is also callable from v0, v1, v2 starting points (chains entry-rewriting migration then applies v3→v4 in the same transaction). Correct and consistent with prior migration pattern.

---

## Blast Radius Assessment

**If the fix has a subtle bug, worst case:**

1. `insert_signal` fails silently (logged as `warn!`): signals are lost. Effect: no confidence update for that session. Wilson score is unaffected meaningfully (5-vote minimum guard). Blast radius: informationally bounded.

2. `drain_signals` fails silently (logged as `warn!`): consumer returns early, queue builds up until SIGNAL_QUEUE cap evicts oldest. Effect: delayed or dropped confidence increments. Not data corruption. Blast radius: confidence evolution slightly slower.

3. `drain_and_signal_session` panics inside `build_signal_output_from_state`: session removed from registry (because `remove()` was called before `build_signal_output_from_state`), but `Option` is `None` — panic would propagate through `dispatch_request` and be caught by the per-connection `tokio::spawn` handler. The session is gone from memory; no duplicate signal can be generated. Effect: one lost session's signals. Blast radius: bounded.

4. Confidence consumer calls `record_usage_with_confidence` with a list of entry_ids that includes a deleted entry: the implementation handles this with per-entry skip. No panic.

5. If `Mutex` poisons (internal panic): `unwrap_or_else(|e| e.into_inner())` recovers the poisoned guard on all six Mutex lock sites in session.rs. Correct poison recovery everywhere. No blind `unwrap()` in production Mutex lock paths.

**Worst-case failure mode**: missed confidence signals for some sessions. Never data corruption, never privilege escalation, never information disclosure.

---

## Regression Risk

- All existing MCP tools unchanged. No MCP wire format changes.
- `build_report` signature change adds `entries_analysis: Option<Vec<EntryAnalysis>>` parameter. All 7 existing call sites in `report.rs` tests and the production `report.rs` file updated with `None`. Backward-compatible wire format via `#[serde(default)]`.
- `dispatch_request` signature adds `pending_entries_analysis` parameter. All test call sites updated with `make_pending()`. No existing behavior changes.
- `SessionState` gains 4 new fields (`signaled_entries`, `rework_events`, `agent_actions`, `last_activity_at`), all initialized to empty/zero defaults in `register_session`. Existing callers (col-007, col-008) unchanged.
- `SIGNAL_QUEUE` table: new table, no existing data. Migration is idempotent. Existing table count comment ("14 tables") updated to 15 in `db.rs` open comment. One stale comment in `test_open_creates_all_tables` still says "14 tables" (the original test) while a new test `test_open_creates_all_15_tables` covers the full set. The stale comment is cosmetic.
- **Zero regression to `unhelpful_count`**: verified above.

---

## OWASP Assessment

| Concern | Assessment |
|---------|-----------|
| Injection (file path in payload) | `file_path` from hook input stored as `String` in `ReworkEvent`, used only for string comparison in `has_crossed_rework_threshold`. Never passed to filesystem, shell, or SQL. No injection surface. |
| Deserialization of untrusted data | `deserialize_signal` operates on redb-stored bytes (server-written, not externally provided). Deserialization errors handled with silent removal. Safe. |
| Broken access control | Signal generation is server-side only. Hook events arrive over UDS authenticated by peer credentials (UID verification). No external actor can inject a fake SessionClose. |
| Input validation at system boundaries | `hook_outcome` is an `&str` matched by a `match` expression: `"success"` / anything else → Abandoned. Unknown values default to Abandoned (no signals). No injection surface. |
| Secrets in code | None found. |
| Privilege escalation | Signal consumers call `record_usage_with_confidence` with the same trust level as any server-side operation. No new privilege paths. |

---

## PR Comments

Posted 2 comments on PR #74.

---

## Self-Check

- [x] Full git diff read
- [x] SCOPE.md and ARCHITECTURE.md read from disk
- [x] All affected source files read in full (session.rs, signal.rs, db.rs, migration.rs, schema.rs, uds_listener.rs, tools.rs excerpt, hook.rs excerpt, server.rs excerpt)
- [x] OWASP concerns evaluated for each changed file
- [x] Blast radius assessed — worst case named
- [x] Input validation checked at system boundaries
- [x] No hardcoded secrets in diff
- [x] Findings posted as PR comments via gh CLI
- [x] Risk level accurately reflects findings (low — no security vulnerabilities, only functional correctness gaps)
- [x] Report written to correct path
