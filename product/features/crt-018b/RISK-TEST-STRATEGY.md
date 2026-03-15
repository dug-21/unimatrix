# Risk-Based Test Strategy: crt-018b — Effectiveness-Driven Retrieval

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Double-lock ordering in ADR-001 snapshot pattern causes deadlock (`effectiveness_state.read()` then `cached_snapshot.lock()`) | High | Medium | Critical |
| R-02 | Utility delta applied at only some of the four `rerank_score` call sites, creating asymmetric re-ranking between Step 7 and Step 8 | High | Medium | Critical |
| R-03 | Auto-quarantine fires during a bulk classification event (e.g., many entries cross threshold in the same tick) causing SQLite write contention inside `spawn_blocking` and partial quarantine with inconsistent counters | High | Medium | Critical |
| R-04 | `consecutive_bad_cycles` counter incremented by `context_status` Phase 8 computation path rather than solely by the background tick writer, violating FR-09 and AC-09 | High | Low | High |
| R-05 | Utility delta is placed **outside** the `status_penalty` multiplication at one or more call sites (violating ADR-003), causing Deprecated/Superseded Effective entries to receive an inappropriately boosted score that bypasses the lifecycle penalty | High | Low | High |
| R-06 | Generation cache fields in `SearchService` / `BriefingService` are per-instance rather than shared across clones (`Arc<Mutex<_>>` missing), causing each rmcp-cloned service instance to maintain a stale independent snapshot | High | Medium | High |
| R-07 | Entry absent from `EffectivenessState.categories` (cold start, new entry) receives a non-zero delta — either a panic or a default-to-penalty — rather than the specified 0.0 | Medium | Low | High |
| R-08 | Tick-skipped audit event is not emitted on `compute_report()` error, silently leaving operators unaware that `consecutive_bad_cycles` is frozen | Medium | Medium | Medium |
| R-09 | `BriefingService` injection-history sort uses effectiveness category as primary key rather than as tiebreaker, reversing entries that differ in confidence but share effectiveness category | Medium | Medium | Medium |
| R-10 | `SETTLED_BOOST` constant exceeds the co-access boost maximum (0.03), violating the signal-hierarchy invariant required by FR-04 / Constraint 5 | Low | Low | Medium |
| R-11 | Auto-quarantine fires for `Settled` or `Unmatched` entries when their `consecutive_bad_cycles` counter is nonzero due to stale state from a prior bad run (AC-14 violation) | Medium | Low | Medium |
| R-12 | `EffectivenessReport.auto_quarantined_this_cycle` field is not populated or is populated after the write lock is released, so `context_status` never surfaces which entries were auto-quarantined in the last tick | Low | Medium | Low |
| R-13 | Write lock on `EffectivenessState` is held across the auto-quarantine SQLite call, blocking all concurrent `search()` and `assemble()` read-lock acquisitions for the duration of the SQL write (NFR-02 violation) | High | Medium | Critical |
| R-14 | crt-019 adaptive `confidence_weight` is at its cold-start default (not exercised) in the integration test fixture, masking ±0.05 utility delta interactions at non-trivial spread values | Medium | Medium | Medium |

---

## Risk-to-Scenario Mapping

### R-01: Double-Lock Ordering Deadlock
**Severity**: High
**Likelihood**: Medium
**Impact**: Server hangs permanently on the first search call after a background tick fires; no recovery without restart.

**Test Scenarios**:
1. Concurrent test: two goroutines (or tokio tasks) call `search()` simultaneously while a background tick writer holds the write lock — assert neither panics nor blocks indefinitely.
2. Unit test: invoke the ADR-001 snapshot code path directly, acquiring `effectiveness_state.read()` and then `cached_snapshot.lock()` in sequence — assert neither lock is held when entering the other's acquisition scope.
3. Verify in `search.rs` code review that the read guard from `effectiveness_state` is **dropped** before `cached_snapshot.lock()` is called (locks are never held simultaneously).

**Coverage Requirement**: The two-lock sequence must be verified by a lock-ordering lint or test assertion that the read guard goes out of scope before the mutex is acquired.

---

### R-02: Utility Delta Applied at Inconsistent Call Sites
**Severity**: High
**Likelihood**: Medium
**Impact**: Step 7 and Step 8 re-rank with different signals, producing non-deterministic ordering changes between the initial sort and the co-access re-sort pass.

**Test Scenarios**:
1. Unit test: construct a search result with an Effective and an Ineffective entry of identical similarity/confidence; verify the Effective entry ranks first in both Step 7 output and Step 8 output (AC-05).
2. Unit test: for each of the four `rerank_score` call sites, pass a known Effective entry and assert the returned score includes `+UTILITY_BOOST` relative to the same entry without classification.
3. Code review checklist: exactly four call sites in `search.rs` include `utility_delta(...)` in their score computation — no more, no fewer.

**Coverage Requirement**: All four call sites exercised by unit tests asserting non-zero deltas for non-Unmatched categories.

---

### R-03: Bulk Auto-Quarantine SQLite Contention
**Severity**: High
**Likelihood**: Medium
**Impact**: If N entries cross the threshold in the same tick, each synchronous `quarantine_entry()` call runs inside `spawn_blocking`. Failure of one quarantine (e.g., entry already quarantined by concurrent manual operation) could abort the loop, leaving later threshold-crossing entries un-quarantined with their counters reset to 0 (counter reset before confirming write success).

**Test Scenarios**:
1. Integration test: seed 5 entries all with `consecutive_bad_cycles` at threshold; fire one background tick; verify all 5 entries are Quarantined and all 5 audit events are written (not just the first).
2. Integration test: manually quarantine one of the 5 entries before the tick fires; verify the remaining 4 are still quarantined and no panic/abort occurs for the already-quarantined entry.
3. Unit test: verify counter is reset to 0 only after a successful `quarantine_entry()` call — not before.

**Coverage Requirement**: Multi-entry quarantine loop must be tested with at least one failure case (already-quarantined entry) to verify per-entry error isolation.

---

### R-04: `context_status` Writing EffectivenessState
**Severity**: High
**Likelihood**: Low
**Impact**: If Phase 8 of `compute_report()` writes to `EffectivenessState` instead of only to `StatusReport.effectiveness`, every `context_status` call advances `consecutive_bad_cycles`, causing premature auto-quarantine in active deployments.

**Test Scenarios**:
1. Integration test: call `context_status` 10 times with a known Ineffective-producing fixture; assert `EffectivenessState.consecutive_bad_cycles` remains 0 for all entries (AC-01, AC-09).
2. Unit test: verify `compute_report()` in `status.rs` does not acquire a write lock on `EffectivenessStateHandle` — the handle is either not held by `StatusService` or is held read-only.

**Coverage Requirement**: Explicit integration test confirming `context_status` does not advance counters.

---

### R-05: Utility Delta Outside Status Penalty Multiplication
**Severity**: High
**Likelihood**: Low
**Impact**: A Deprecated entry with Effective classification receives the full +0.05 boost regardless of its 0.7× lifecycle penalty. Deprecated entries can rank above Active entries in retrieval results, undermining the status lifecycle signal.

**Test Scenarios**:
1. Unit test: Deprecated Effective entry with sim=0.75 and conf=0.60 — assert final score = `(rerank_score + 0.05) * 0.7`, not `rerank_score * 0.7 + 0.05`.
2. Unit test: Superseded Noisy entry — assert final score = `(rerank_score - 0.05) * 0.5`, not `rerank_score * 0.5 - 0.05`.
3. Code review: verify the parenthesization at all four call sites matches the formula in ADR-003.

**Coverage Requirement**: Deprecated and Superseded entries with non-zero utility delta must each have a numeric assertion test verifying the penalty-inside placement.

---

### R-06: Generation Cache Not Shared Across Service Clones
**Severity**: High
**Likelihood**: Medium
**Impact**: Under rmcp concurrency, each clone of `SearchService` maintains an independent stale `EffectivenessSnapshot`. After a background tick, some clones see new classifications and some see old ones, producing non-deterministic ordering divergence across concurrent requests.

**Test Scenarios**:
1. Unit test: create two clones of `SearchService` from the same instance; trigger a background tick (generation bump); assert both clones see the updated categories on their next `search()` call.
2. Unit test: assert `cached_snapshot` field type is `Arc<Mutex<EffectivenessSnapshot>>`, not a plain `EffectivenessSnapshot` — verifying the shared wrapper is present.

**Coverage Requirement**: Clone-sharing test must exercise generation mismatch detection across at least two clone instances.

---

### R-07: Absent Entry in EffectivenessState Receives Non-Zero Delta
**Severity**: Medium
**Likelihood**: Low
**Impact**: Newly inserted entries or entries not yet seen by the background tick are penalized or boosted incorrectly, causing regression for new knowledge relative to pre-crt-018b behavior.

**Test Scenarios**:
1. Unit test: call `utility_delta(None)` and assert return value is exactly `0.0` (AC-06).
2. Unit test: run `search()` against an empty `EffectivenessState`; assert result ordering is identical to pre-crt-018b scoring (no utility delta contribution in any direction).
3. Unit test: call `effectiveness_priority(None)` in briefing and assert return is `0` (neutral, not negative).

**Coverage Requirement**: Empty state must produce 0.0 delta at both the utility function level and the full search pipeline level.

---

### R-08: Tick-Skipped Audit Event Not Emitted
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Operators have no visibility into how frequently ticks fail. `consecutive_bad_cycles` appears frozen without explanation; false-positive quarantine delay is opaque.

**Test Scenarios**:
1. Integration test: inject a `compute_report()` error (mock or fault injection); assert an audit event with `operation = "tick_skipped"` and a non-empty `reason` field is written to the audit log.
2. Integration test: after a skipped tick, assert `EffectivenessState` is identical to its pre-error state (no partial update, no counter change).

**Coverage Requirement**: The error path in `maintenance_tick()` must have at least one integration test asserting the audit event is written and state is unchanged.

---

### R-09: Briefing Sort Uses Effectiveness as Primary Key
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Two entries with confidence 0.90 (Effective) and 0.40 (Ineffective) are served in the wrong order — the high-confidence Ineffective entry should rank first because confidence is primary. Reversing this degrades briefing quality for mature knowledge bases.

**Test Scenarios**:
1. Unit test: briefing injection history with entry A (confidence=0.90, Ineffective) and entry B (confidence=0.40, Effective) — assert A ranks first (primary confidence key respected) (AC-07).
2. Unit test: briefing injection history with entry A (confidence=0.60, Ineffective) and entry B (confidence=0.60, Effective) — assert B ranks first (tiebreaker on equal confidence) (AC-07).
3. Unit test: same two-key sort check for convention lookup path (AC-08).

**Coverage Requirement**: Both the primary-key-respected and the tiebreaker-activated scenarios must be tested for injection history and convention lookup.

---

### R-10: SETTLED_BOOST Exceeds Co-Access Max
**Severity**: Low
**Likelihood**: Low
**Impact**: `SETTLED_BOOST > 0.03` would make Settled classification the dominant query-time differentiator, overwhelming the co-access signal for entries with equal confidence.

**Test Scenarios**:
1. Unit test: assert `SETTLED_BOOST < 0.03` (AC-03, Constraint 5). This is a compile-time constant — a single assertion test is sufficient.

**Coverage Requirement**: One constant invariant assertion.

---

### R-11: Auto-Quarantine Firing for Settled/Unmatched Entries
**Severity**: Medium
**Likelihood**: Low
**Impact**: A Settled entry with a stale nonzero `consecutive_bad_cycles` counter (e.g., it was Ineffective in prior ticks and then recovered) is incorrectly quarantined when a threshold check uses only the counter value without re-checking the current category.

**Test Scenarios**:
1. Unit test: entry with `consecutive_bad_cycles = 10` and current category `Settled` — assert auto-quarantine does NOT fire (AC-14).
2. Unit test: entry with `consecutive_bad_cycles = 10` and current category `Unmatched` — assert auto-quarantine does NOT fire (AC-14).
3. Integration test: entry transitions from Ineffective (2 cycles) to Settled (1 cycle); assert counter resets to 0 and no quarantine occurs (counter reset on recovery).

**Coverage Requirement**: All non-Ineffective/non-Noisy categories must have an explicit no-quarantine assertion.

---

### R-12: `auto_quarantined_this_cycle` Field Not Populated
**Severity**: Low
**Likelihood**: Medium
**Impact**: `context_status` output does not surface recently auto-quarantined entries, eliminating the operator diagnostic workflow (Workflow 4) described in the spec.

**Test Scenarios**:
1. Integration test: trigger auto-quarantine for one entry; call `context_status` in the same tick cycle; assert `auto_quarantined_this_cycle` in the response contains the quarantined entry's ID (AC-13, FR-14).

**Coverage Requirement**: One integration test verifying the field is populated after a quarantine event.

---

### R-13: Write Lock Held During Auto-Quarantine SQL Write
**Severity**: High
**Likelihood**: Medium
**Impact**: If the write lock on `EffectivenessState` is not released before the `quarantine_entry()` synchronous SQLite call, all concurrent `search()` and `assemble()` read-lock acquisitions block for the duration of the SQL write. Under bulk quarantine this could be hundreds of milliseconds, violating NFR-01 and NFR-02.

**Test Scenarios**:
1. Code review: verify the write lock `MutexGuard` / `RwLockWriteGuard` on `EffectivenessState` is explicitly dropped (goes out of scope or is `drop()`-ed) before any `quarantine_entry()` call in `maintenance_tick()`.
2. Concurrency test: while auto-quarantine is running, issue a `search()` call on a second thread; assert the search completes without blocking for more than 10ms.
3. Unit test: verify the architecture invariant that NFR-02 specifies — write lock released before SQL writes begin.

**Coverage Requirement**: The lock-release-before-SQL ordering must be verified by both code inspection and a concurrency test.

---

### R-14: crt-019 Adaptive Weight Not Exercised in Integration Fixture
**Severity**: Medium
**Likelihood**: Medium
**Impact**: Integration tests run with `confidence_weight = 0.15` (cold-start default) throughout, meaning the utility delta interaction at `weight = 0.25` is never tested. The boundary case where `confidence_weight * conf ≈ utility_delta` in magnitude is untested.

**Test Scenarios**:
1. Integration test prerequisite: assert `EffectivenessState` confidence spread is non-zero in the fixture before exercising search re-ranking (AC-17 item 4).
2. Integration test: run the Effective-vs-Ineffective ordering test at both spread extremes (minimum weight 0.15, maximum weight 0.25); assert consistent ordering at both ends.

**Coverage Requirement**: At least one integration test must run with a fixture where `observed_spread >= 0.20` to confirm the combined formula at full confidence weight.

---

## Integration Risks

### Background Tick Writer / EffectivenessState Write Lock / Search Reader Contention
The background tick acquires a write lock on `EffectivenessState` (Component 2) while `SearchService` and `BriefingService` hold read locks during active queries. Write starvation is possible under high query load: if read locks are continuously held, the write lock never acquires. This must not block the maintenance tick indefinitely.

**Scenario**: Under sustained `context_search` load, verify the background tick write completes within one tick interval (15 minutes) and does not indefinitely defer due to read-lock starvation.

### EffectivenessState Write / Auto-Quarantine / ConfidenceState Write Ordering
`maintenance_tick()` writes `EffectivenessState` before calling `run_maintenance()`, which writes `ConfidenceState`. Auto-quarantine fires between these two writes. A confidence recompute (`fire-and-forget`) is triggered per quarantined entry. If the confidence recompute races with the subsequent `run_maintenance()` confidence batch write, stale confidence values may overwrite freshly computed per-entry values.

**Scenario**: Verify the fire-and-forget confidence recompute triggered by auto-quarantine does not interfere with the `run_maintenance()` confidence batch refresh that follows in the same tick.

### `EffectivenessReport` Availability in `StatusReport.effectiveness`
`maintenance_tick()` reads `report.effectiveness` to populate `EffectivenessState`. If Phase 8 of `compute_report()` fails silently (returns `Some(EffectivenessReport { by_category: {}, ... })` instead of producing an error), the write will replace live classifications with an empty map, effectively resetting all utility deltas to 0.0 for the next 15 minutes.

**Scenario**: Inject a fixture where `compute_effectiveness_aggregates()` returns an empty result set; verify `EffectivenessState.categories` is not emptied and a tick-skipped event is emitted or a warning is logged.

---

## Edge Cases

| Edge Case | Risk | Scenario |
|-----------|------|----------|
| `AUTO_QUARANTINE_CYCLES = 0` at startup | Auto-quarantine fires when it should be disabled | Unit test: counter at 100, threshold 0 — confirm no quarantine call (AC-12) |
| `AUTO_QUARANTINE_CYCLES = 1` | Quarantine on first bad tick, no persistence guard | Integration test: verify quarantine fires on tick 1, not before |
| Entry transitions Ineffective → Effective → Ineffective in 3 ticks | Counter should be 1, not 3 (reset on recovery) | Unit test: tick 1=Ineffective (counter=1), tick 2=Effective (counter=0), tick 3=Ineffective (counter=1) — no quarantine |
| Server restart mid-accumulation | Counter reset; no retroactive quarantine | Integration test: simulate restart after 2 consecutive bad ticks; assert counter is 0 post-restart, no quarantine on next tick |
| Entry deleted/deprecated between tick write and auto-quarantine SQL | `quarantine_entry()` on a non-Active entry | Integration test: verify quarantine of already-deprecated entry does not panic; entry remains deprecated (not quarantined) or error is logged and skipped |
| `EffectivenessState` empty at first `search()` call | Nil-map access panics | Unit test: `search()` against empty state produces identical results to pre-crt-018b (R-07) |
| All entries in knowledge base are `Settled` | SETTLED_BOOST becomes universal tiebreaker | Unit test: all-Settled search produces same relative ordering as confidence-only (no cross-category distortion) |
| Very large `HashMap` (> 5,000 entries) | Clone latency exceeds 1ms budget | Benchmark / load test: 5,000-entry state, confirm clone completes under 1ms with generation cache skipping it |

---

## Security Risks

### Auto-Quarantine as Denial-of-Service Vector
**Untrusted input surface**: `AUTO_QUARANTINE_CYCLES` is read from an environment variable at server startup. An operator who can set environment variables can set `AUTO_QUARANTINE_CYCLES = 1`, causing every entry classified Ineffective for even one tick to be quarantined within 15 minutes.

**Blast radius**: An attacker with env-var write access could configure the system to auto-quarantine its entire active knowledge base within `1 × 15 minutes`. Recovery requires manual `context_quarantine` restore operations per entry.

**Mitigation scenario**: Verify `UNIMATRIX_AUTO_QUARANTINE_CYCLES` is validated at startup: must be a non-negative integer; reject values that are implausibly large (e.g., > 1000) with a startup error rather than silent acceptance.

### Audit Event Agent Identity
**Untrusted input surface**: The `agent_id = "system"` in auto-quarantine audit events is a hardcoded string. If any other code path (e.g., a future MCP tool) can emit audit events with `agent_id = "system"`, it becomes difficult to distinguish genuine auto-quarantine events from manually crafted ones.

**Blast radius**: Low — audit events are observability artifacts; no security boundary is enforced based on `agent_id`.

**Mitigation scenario**: Verify the audit event writer for auto-quarantine uses a constant (not a user-controlled string), and that the `agent_id` field is not propagated from any MCP request parameters in this code path.

### EffectivenessState Lock Poison Attack Surface
**Untrusted input surface**: A panic in any code holding the `RwLock<EffectivenessState>` write guard poisons the lock. The poison recovery pattern `.unwrap_or_else(|e| e.into_inner())` is specified in the architecture (reusing the `CategoryAllowlist` convention). If not applied uniformly, a panic during the write path will render search permanently degraded.

**Blast radius**: All subsequent `search()` and `assemble()` calls panic on lock acquisition until server restart.

**Mitigation scenario**: Verify all read and write acquisitions of `EffectivenessStateHandle` use `.unwrap_or_else(|e| e.into_inner())` and never use `.unwrap()` or `.expect()`.

---

## Failure Modes

| Failure | Expected Behavior | AC / FR Reference |
|---------|------------------|-------------------|
| `compute_report()` returns error during tick | Hold all counters; emit `tick_skipped` audit event; do not modify `EffectivenessState`; continue to next tick | FR-09, FR-13, ADR-002 |
| `quarantine_entry()` fails for one entry in bulk quarantine | Log warning; skip that entry; continue to next threshold-crossing entry; counters for successful entries are reset | Architecture error boundaries |
| `EffectivenessState` lock poisoned | `.unwrap_or_else(|e| e.into_inner())` — degrade to stale state, not panic | Architecture poison recovery |
| `EffectivenessState` is empty on first search | `utility_delta(None) = 0.0` — behavior identical to pre-crt-018b | NFR-06, AC-06 |
| `BriefingService` receives empty state | `effectiveness_priority(None) = 0` — sort degrades to confidence-only | ADR-004, NFR-06 |
| `AUTO_QUARANTINE_CYCLES` env var missing or malformed | Default to 3; do not crash | FR-12, AC-11 |
| Server restart with counters mid-accumulation | Counters reset to 0; no retroactive quarantine; N-cycle guard restarts fresh | Constraint 6, NFR-07 |
| Background tick fires while write lock is held by a prior write | Second write queues on the lock; no double-write corruption | `Arc<RwLock<_>>` semantics |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (in-memory restart resets consecutive counters; N-cycle guard deferred under frequent restarts) | R-03 (partial), Failure Modes table | Addressed explicitly as Constraint 6 / NFR-07. In-memory reset is intentional and documented. No additional mitigation — operator must not restart repeatedly to circumvent the guard. The risk is accepted. |
| SR-02 (HashMap clone cost grows linearly with entry count) | R-01, R-06, R-13 (ADR-001 resolution) | Addressed by ADR-001: generation counter + `Arc<Mutex<EffectivenessSnapshot>>` eliminates per-search clone on the common path. Clone cost amortized to once per 15-minute tick. Scalability risk reduced from Medium to Low for foreseeable entry counts. |
| SR-03 (auto-quarantine irreversible without operator action; false-positive risk) | R-11, R-03, Security Risks (auto-quarantine audit identity) | Addressed by FR-11 (rich audit event schema), FR-14 (`auto_quarantined_this_cycle` visibility), and Workflow 4 (operator diagnosis flow). Restore path exists via `context_quarantine`. Risk mitigated but not eliminated — operators must monitor audit log. |
| SR-04 (±0.05 utility delta interaction with adaptive confidence weight unstated) | R-05, R-14 | Addressed in ARCHITECTURE.md Component 3 with full combined formula at both spread extremes. ADR-003 positions delta inside penalty multiplication. FR-06 in spec contains explicit magnitude analysis at min and max confidence weights. Fully addressed. |
| SR-05 (Settled boost as dominant differentiator in mature knowledge bases) | R-10 | Addressed by Constraint 5 (`SETTLED_BOOST = 0.01 < 0.03 = co-access max`) and AC-03/FR-04 constant invariant assertion. Risk reduced to Low. |
| SR-06 (BriefingService wiring miss causes silent injection-history regression) | R-06, R-09 | Addressed by ADR-004: `EffectivenessStateHandle` is a required non-optional constructor parameter. Incomplete wiring is a compile error. Fully mitigated. |
| SR-07 (tick error increments consecutive counter on stale data, false-positive quarantine) | R-04, R-08, R-13 | Addressed by ADR-002 (hold semantics) and FR-09/FR-13 (tick-skipped audit event). Hold-on-error is the implemented behavior. Fully mitigated. |
| SR-08 (crt-019 dependency not exercised in test fixture) | R-14 | Addressed by AC-17 item 4 and Constraint 11 in SPECIFICATION.md. Integration test fixture must confirm non-zero confidence spread. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-03, R-13) | 12 scenarios (lock ordering, 4-site coverage, bulk quarantine isolation, lock-release-before-SQL) |
| High | 4 (R-04, R-05, R-06, R-07) | 10 scenarios (no-write from context_status, penalty placement, clone sharing, absent-entry zero-delta) |
| Medium | 6 (R-08, R-09, R-10, R-11, R-14, integration risks) | 12 scenarios (tick-skip audit, sort key order, constant invariant, category restriction, spread fixture) |
| Low | 2 (R-12, edge cases) | 8 scenarios (visibility field, all listed edge cases) |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — relevant findings: #1366 (Tick Loop Error Recovery), #1542 (Background Tick Writers: Define Error Semantics for Consecutive Counters Before Implementation), #732 (Extract-and-Catch Pattern). These directly informed R-08 and R-04 severity elevation.
- Queried: `/uni-knowledge-search` for "risk pattern" category:pattern — findings: double-lock deadlock pattern (informed R-01 as Critical), write-lock-held-during-SQL pattern (informed R-13 as Critical).
- Queried: `/uni-knowledge-search` for "ConfidenceState Arc RwLock" — findings: #255 (ADR-004: Batched Confidence Recomputation), #1480 (parameter-passing over shared state). Informed R-06 (clone sharing across rmcp service clones).
- Stored: nothing novel — R-01 (double-lock) and R-13 (write-lock-held-during-SQL) are already captured as patterns in Unimatrix (#1366, existing ADR series). The generation-cache-across-clones risk (R-06) is specific to rmcp's `Clone` requirement and may warrant a pattern entry if it recurs in a subsequent feature.
