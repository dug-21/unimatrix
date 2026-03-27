# Risk-Based Test Strategy: col-031 — Phase-Conditioned Frequency Table

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Silent wiring bypass: `run_single_tick` constructs services directly, bypassing `ServiceLayer`; `PhaseFreqTableHandle` silently dropped | High | High | Critical |
| R-02 | AC-12 declared PASS vacuously: `replay.rs` never forwards `current_phase` to `ServiceSearchParams`; all eval runs see `phase_explicit_norm = 0.0` trivially | High | High | Critical |
| R-03 | `phase_affinity_score` called by fused scoring during cold-start: `use_fallback` guard absent or fires after method call; uniform 0.05 boost alters pre-col-031 score identity | High | Med | High |
| R-04 | Wrong cold-start return for PPR: fused-scoring guard logic bleeds into `phase_affinity_score` implementation returning `0.0` instead of `1.0`; PPR personalization vector collapses | High | Med | High |
| R-05 | `json_each` type cast omitted: `CAST(json_each.value AS INTEGER)` dropped or mangled; query returns wrong `entry_id` values or zero rows silently | High | Med | High |
| R-06 | Lock held across scoring loop: `PhaseFreqTableHandle` read lock not released before the scoring iteration begins; per-entry lock contention degrades search latency | Med | Med | High |
| R-07 | Rank normalization formula off-by-one: `1 - rank/N` (0-indexed) used instead of `1 - (rank-1)/N` (1-indexed); single-entry buckets score `0.0` instead of `1.0` | Med | Med | High |
| R-08 | `query_log_lookback_days` not validated in `validate()`: accepts `0` or >3650 values causing empty window or effectively unbounded scan | Med | Med | Medium |
| R-09 | Rebuild failure swallows state: error path replaces existing handle with cold-start instead of retaining it; active phase signal lost on transient SQL error | Med | Low | Medium |
| R-10 | Phase vocabulary mismatch: runtime phase string rename silently strands historical data under old key; new key immediately cold-starts with no alerting | Med | Low | Medium |
| R-11 | `w_phase_explicit = 0.05` calibration regression: phase signal noisy for a deployment; no automated guard except AC-12 which depends on AC-16 | Med | Med | Medium |
| R-12 | Lock acquisition order violated: future refactor of `run_single_tick` acquires `PhaseFreqTableHandle` before `TypedGraphStateHandle`; deadlock potential | Low | Low | Low |
| R-13 | `PhaseFreqRow.freq` typed as `u64` instead of `i64`: sqlx maps `COUNT(*)` to `i64`; compile or runtime deserialization failure | Med | Low | Medium |
| R-14 | Test helper sites miss handle: `server.rs`, `shutdown.rs`, `test_support.rs`, `listener.rs`, `eval/profile/layer.rs` not updated; CI test infra fails to compile | Med | High | High |

---

## Risk-to-Scenario Mapping

### R-01: Silent Wiring Bypass in `run_single_tick`
**Severity**: High
**Likelihood**: High
**Impact**: `PhaseFreqTableHandle` accepted at `spawn_background_tick` but silently dropped before `run_single_tick`; scoring path sees perpetual cold-start `use_fallback = true`; feature ships inert. Lesson #3216 (GH #311, dsn-001) documents this exact failure mode; pattern #3213 enumerates all affected construction sites.

**Test Scenarios**:
1. Integration test: call `SearchService::search` with `current_phase = "delivery"` after a tick cycle that populates the handle; assert `phase_explicit_norm > 0.0` for at least one candidate — proves the handle is not the default cold-start stub.
2. Code review / grep: confirm every `SearchService::new` call site in `background.rs` receives a non-default `PhaseFreqTableHandle` argument (not a freshly-constructed `new_handle()`).
3. Compilation test: confirm `PhaseFreqTableHandle` is not `Option<...>` at any construction site — missing wiring must fail to compile (ADR-005).

**Coverage Requirement**: The integration test (scenario 1) must observe a non-zero `phase_explicit_norm` after a real tick. Code review (scenario 2) must enumerate all `SearchService::new` sites. Both must pass.

---

### R-02: Vacuous AC-12 Gate — `replay.rs` Not Forwarding `current_phase`
**Severity**: High
**Likelihood**: High
**Impact**: AC-12 regression gate (MRR ≥ 0.35, CC@5 ≥ 0.2659, ICD ≥ 0.5340) passes trivially because `current_phase = None` for all eval scenarios; `w_phase_explicit * 0.0 = 0.0`; gate is a noise check, not a regression gate. SR-03 and ADR-004 both flag this as a hard non-separable deliverable.

**Test Scenarios**:
1. After AC-16 fix in `replay.rs`: inspect eval scenario output for at least one row with non-null `current_phase` value — confirms the field is forwarded, not silently dropped.
2. Gate ordering check: reject any AC-12 PASS claim submitted without evidence of non-null `current_phase` in scenario output. This is a process gate, not a code test, but must be enforced at Gate 3b.
3. Diff constraint: `replay.rs` diff must add `current_phase: record.context.phase.clone()` to the `ServiceSearchParams` struct literal at line ~80; diff must NOT touch `extract.rs` or `output.rs` (those already handle phase per the architecture).

**Coverage Requirement**: Eval scenario output file must contain at least one non-null `current_phase` entry. Gate 3b must reject AC-12 PASS without this evidence.

---

### R-03: Fused Scoring `use_fallback` Guard Absent or Fires Too Late
**Severity**: High
**Likelihood**: Med
**Impact**: When `use_fallback = true` and a phase is provided, `phase_affinity_score` returns `1.0`; fused scoring applies `0.05 × 1.0 = 0.05` additive boost to every candidate uniformly; relative rankings change for queries with `current_phase` set during cold-start — violating NFR-04 score identity guarantee.

**Test Scenarios**:
1. AC-11 Test 2: `current_phase = Some("delivery")`, `use_fallback = true` table → assert `phase_explicit_norm = 0.0` for all candidates and that `phase_affinity_score` was NOT called (use a spy/mock or observe the guard via code path).
2. Lock sequence test: assert the read lock is acquired and released before the scoring loop variable is first accessed (not held across the loop body).
3. Score identity test: compare fused scores with `w_phase_explicit = 0.05` + cold-start handle against a baseline computed with `w_phase_explicit = 0.0`; they must be bit-for-bit identical.

**Coverage Requirement**: AC-11 Test 2 plus a score-identity assertion. Both must pass before AC-06 is declared PASS.

---

### R-04: Wrong Cold-Start Return for PPR Caller
**Severity**: High
**Likelihood**: Med
**Impact**: `phase_affinity_score` returns `0.0` on cold-start instead of `1.0`; PPR personalization vector becomes `hnsw_score × 0.0 = 0.0` for all seeds; PageRank collapses to uniform distribution. This breaks #398 integration silently — the method signature is correct but the contract is wrong.

**Test Scenarios**:
1. AC-11 Test 3: call `phase_affinity_score` directly on a `use_fallback = true` table; assert return value is exactly `1.0` (f32 equality).
2. Absent-phase test: call `phase_affinity_score` with a phase not present in the table (populated, `use_fallback = false`); assert `1.0`.
3. Absent-entry test: call `phase_affinity_score` with an `entry_id` not in the `(phase, category)` bucket; assert `1.0`.

**Coverage Requirement**: All three `1.0` return paths must have dedicated unit tests (AC-07). Each test asserts `== 1.0f32`.

---

### R-05: `json_each` Type Cast Omitted or Mangled
**Severity**: High
**Likelihood**: Med
**Impact**: `result_entry_ids` stores unquoted JSON integers (e.g., `[42,7,19]`). Without `CAST(json_each.value AS INTEGER)`, `json_each.value` is a text affinity; the JOIN to `entries.id` (INTEGER) may return zero rows or mismatched rows silently. Pattern #3678 flags this exact risk. The rebuild SQL returns an empty result set; `use_fallback = true`; feature silently inert. Unimatrix #3681 (ADR-003 col-031) documents the verified correct form.

**Test Scenarios**:
1. AC-08 integration test: seed `query_log` with `result_entry_ids = "[42]"` and confirm `query_phase_freq_table` returns a row with `entry_id = 42` (u64) — proves cast works at the store layer.
2. AC-14 normalization test: seed multiple entries for the same `(phase, category)`; confirm all expected `entry_id` values appear in the rebuilt table.
3. SQL inspection: code review confirms `CAST(json_each.value AS INTEGER)` appears in both the `SELECT` clause and the `JOIN` predicate.

**Coverage Requirement**: AC-08 must pass with a non-empty result and correct `entry_id` values.

---

### R-06: Lock Held Across Scoring Loop
**Severity**: Med
**Likelihood**: Med
**Impact**: If the `PhaseFreqTableHandle` read lock is acquired inside the scoring loop (per-entry) rather than once pre-loop, concurrent background tick writes are blocked for the full scoring duration. Under load, this serializes all searches behind tick writes, degrading latency. NFR-02 requires the lock be released before the loop begins.

**Test Scenarios**:
1. Code review: confirm the `PhaseFreqTableHandle.read()` call site in `search.rs` is before the candidate iteration loop, and the guard is dropped (or goes out of scope) before the first loop iteration.
2. Lock sequence unit test: construct a `SearchService` with a populated handle; start a write task that holds the write lock for 100ms; confirm a concurrent search completes without blocking (proving the read lock was released before scoring).

**Coverage Requirement**: Code review is mandatory. The concurrency test is recommended but the code structure check is the primary gate.

---

### R-07: Rank Normalization Off-by-One Formula
**Severity**: Med
**Likelihood**: Med
**Impact**: Using `1.0 - rank/N` (1-indexed rank) produces `0.0` for single-entry buckets (N=1, rank=1: `1 - 1/1 = 0`). This means the only entry in a bucket gets affinity `0.0` instead of `1.0`, suppressing that entry's phase contribution entirely. ADR-001 explicitly documents the correct form: `1.0 - ((rank-1) as f32 / N as f32)`.

**Test Scenarios**:
1. AC-13 single-entry test: seed one entry for `(phase="scope", category="decision")`; after rebuild, assert `phase_affinity_score(entry_id, "decision", "scope") == 1.0f32`.
2. AC-14 multi-entry normalization: seed entries with frequencies 10, 5, 1 in the same bucket; assert rank-1 scores `1.0`, rank-2 scores `0.666...`, rank-3 scores `0.333...` (N=3 formula).
3. Last-entry test: in a 5-entry bucket, assert rank-5 entry scores `(5-1)/5 = 0.8` (not `0.0`).

**Coverage Requirement**: AC-13 and AC-14 both pass with exact score assertions.

---

### R-08: `query_log_lookback_days` Range Not Validated
**Severity**: Med
**Likelihood**: Med
**Impact**: Value `0` produces `WHERE ts > strftime('%s','now') - 0` (no window — returns no rows; `use_fallback = true`). Values > 3650 produce effectively unbounded scans. ADR-002 specifies range `[1, 3650]` must be enforced in `validate()`.

**Test Scenarios**:
1. Unit test: `InferenceConfig::validate()` with `query_log_lookback_days = 0` returns an error.
2. Unit test: `InferenceConfig::validate()` with `query_log_lookback_days = 3651` returns an error.
3. Unit test: `query_log_lookback_days = 1` and `= 3650` both pass validation.

**Coverage Requirement**: Boundary tests at 0, 1, 3650, and 3651.

---

### R-09: Rebuild Failure Overwrites Existing State with Cold-Start
**Severity**: Med
**Likelihood**: Low
**Impact**: If the error path in `run_single_tick` calls `*guard = PhaseFreqTable::new()` (cold-start) instead of simply not writing on error, a transient SQL failure resets a populated table to `use_fallback = true`. Phase signal lost until next successful tick (~15 minutes).

**Test Scenarios**:
1. AC-04 failure retention test: inject a store mock that returns an error from `query_phase_freq_table`; assert that after the tick, the handle still contains the pre-tick table (non-cold-start) and `tracing::error!` was emitted.
2. Code inspection: confirm the error branch in `run_single_tick` contains no write to the handle — only the success branch writes.

**Coverage Requirement**: AC-04 must include the failure retention assertion, not only the success swap assertion.

---

### R-10: Silent Phase Vocabulary Staleness
**Severity**: Med
**Likelihood**: Low
**Impact**: Phase string rename (e.g., "delivery" → "implement") in the workflow layer strands all frequency history under "delivery"; "implement" starts cold. No alert, no detection, no migration path in scope. CON-09 classifies this as expected behavior, but it is operationally invisible.

**Test Scenarios**:
1. Unit test: populate table with phase="delivery"; call `phase_affinity_score` with phase="implement"; assert returns `1.0` (neutral cold-start, not an error).
2. Documentation test: confirm `CON-09` and phase rename behavior are documented in the `PhaseFreqTable` module-level doc comment or `phase_affinity_score` doc comment.

**Coverage Requirement**: Test 1 must pass. Documentation presence is a code review check.

---

### R-11: `w_phase_explicit = 0.05` Causes CC@5 / ICD Regression
**Severity**: Med
**Likelihood**: Med
**Impact**: The 0.05 weight is judgment-based (ADR-004 consequence; OQ-03 in spec). If col-030 baselines were measured with scenarios that had non-null `current_phase`, raising the weight to 0.05 may shift scores below the AC-12 thresholds. AC-12 is the only automated guard and it depends on AC-16 being complete.

**Test Scenarios**:
1. Pre-gate eval run: re-run col-030 baseline eval with `w_phase_explicit = 0.05` and AC-16 applied; compare MRR, CC@5, ICD against thresholds before declaring AC-12.
2. Sensitivity test: run eval with `w_phase_explicit = 0.0` and `0.05`; confirm CC@5 and ICD do not fall below 0.2659 / 0.5340 respectively.

**Coverage Requirement**: AC-12 gate run with AC-16 present; scenario output verified to contain non-null `current_phase`. Results reported against all three metric thresholds.

---

### R-12: Lock Acquisition Order Violated by Future Refactor
**Severity**: Low
**Likelihood**: Low
**Impact**: Required order in `run_single_tick` is `EffectivenessStateHandle` → `TypedGraphStateHandle` → `PhaseFreqTableHandle`. A future refactor acquiring `PhaseFreqTableHandle` before `TypedGraphStateHandle` while a concurrent writer holds the opposite order creates a deadlock. No compile-time enforcement exists (NFR-03).

**Test Scenarios**:
1. Code comment check: `run_single_tick` must have an inline comment at the lock sequence site naming the three handles in the required order.
2. Static order audit: grep `phase_freq_table.write()` and `typed_graph.write()` in `run_single_tick`; confirm `typed_graph` write precedes `phase_freq_table` write in file order.

**Coverage Requirement**: Code review mandatory. Comment presence is a Gate 3b check.

---

### R-13: `PhaseFreqRow.freq` Wrong Integer Type
**Severity**: Med
**Likelihood**: Low
**Impact**: `COUNT(*)` in SQLite maps to `i64` via sqlx 0.8. If `freq` is typed as `u64`, sqlx deserialization fails at runtime (not compile time for sqlx 0.8 dynamic queries); rebuild returns an error; `use_fallback = true`. Silent until the first tick.

**Test Scenarios**:
1. AC-08 integration test: seed rows and call `query_phase_freq_table`; confirm the call succeeds (no sqlx error) and returns `PhaseFreqRow` values with correct `freq` values.
2. Type inspection: code review confirms `PhaseFreqRow.freq: i64` (not `u64`).

**Coverage Requirement**: AC-08 must pass without sqlx deserialization errors.

---

### R-14: Test Helper Sites Not Updated for New Constructor Parameter
**Severity**: Med
**Likelihood**: High
**Impact**: `server.rs`, `shutdown.rs`, `test_support.rs`, `listener.rs`, `eval/profile/layer.rs` all construct `SearchService` or call `spawn_background_tick`. If any site is missed when adding the required `PhaseFreqTableHandle` parameter, CI fails to compile. ADR-005 and pattern #3213 enumerate the known sites.

**Test Scenarios**:
1. CI compilation: `cargo build --workspace` passes with no missing argument errors across all crates.
2. Site enumeration: grep `crates/unimatrix-server/` for `SearchService::new` and `spawn_background_tick`; confirm every occurrence has been updated to pass `PhaseFreqTableHandle`.

**Coverage Requirement**: CI must compile cleanly. The grep audit must be performed and documented before delivery is declared complete.

---

## Integration Risks

**Background tick / ServiceLayer boundary**: The most dangerous integration point in col-031. `run_single_tick` has an established pattern (lesson #3216) of direct service construction that bypasses `ServiceLayer`. The `PhaseFreqTableHandle` must traverse the full chain: `ServiceLayer::with_rate_config` → `phase_freq_table_handle()` accessor → `main.rs` → `spawn_background_tick` → `background_tick_loop` → `run_single_tick`. Any gap in this chain produces a silent perpetual cold-start.

**Scoring / locking boundary**: `search.rs` acquires the read lock once, extracts a bucket snapshot, releases the lock, then runs the scoring loop against the snapshot. If the snapshot extraction is incomplete (wrong phase or wrong category key lookup), entries silently receive `1.0` (neutral) rather than their actual affinity. The only detection is an integration test that asserts `phase_explicit_norm > 0.0` for a known-seeded entry.

**Eval harness / scoring pipeline boundary**: The eval replay path (`replay.rs`) is the sole bridge between recorded scenario context and live scoring. The `current_phase` field on `ServiceSearchParams` is new in col-031. If `replay.rs` does not set it, the eval harness silently activates the `None` branch — perpetual `phase_explicit_norm = 0.0` — and AC-12 becomes a noise check rather than a regression gate. This is the SR-03 / R-02 risk.

**PPR integration contract boundary**: `phase_affinity_score` is published as a public API for #398. Its cold-start return value (`1.0`) is a contract commitment. Any change to the method between col-031 and #398 delivery that alters the cold-start return value breaks PPR personalization silently. The doc comment (AC-17) is the only enforcement mechanism.

---

## Edge Cases

- **Empty `query_log`** (fresh deployment): SQL returns zero rows; `use_fallback = true`; all scores identical to pre-col-031. Test: AC-01 (cold-start construction) plus a rebuild with empty store.
- **All `query_log` rows have `phase = NULL`** (pre-col-028 environment): `WHERE phase IS NOT NULL` filters all rows; `use_fallback = true`. Same graceful degradation. Must be explicitly tested.
- **Single-entry `(phase, category)` bucket**: Rank formula corner case. Assert score `= 1.0`, not `0.0` (ADR-001). R-07 covers this.
- **`lookback_days = 1`**: Minimal window. If no rows in the past 24 hours, `use_fallback = true`. Validate the arithmetic `strftime('%s','now') - 1 * 86400` is correct for `INTEGER` `ts`.
- **`result_entry_ids = NULL`**: Filtered by `WHERE result_entry_ids IS NOT NULL`. Test: seed a row with `result_entry_ids = NULL`; confirm it does not appear in the rebuilt table.
- **`result_entry_ids` referencing a deleted entry**: `JOIN entries e ON ... = e.id` silently drops orphaned IDs. The rebuilt table will not contain them. No error emitted — correct behavior, but must be understood.
- **Phase string case sensitivity**: `"Delivery"` and `"delivery"` are different keys in the `HashMap`. A caller providing mixed-case phases silently misses the bucket. No normalization is in scope; document as a known behavior.

---

## Security Risks

**Untrusted input surface**: `PhaseFreqTable` is internal state rebuilt exclusively from `query_log`. There is no direct external input path. The phase string originates from `query_log.phase`, which was written by the MCP server from `ServiceSearchParams.current_phase` — itself from agent-supplied tool call parameters.

- **Phase string injection**: `current_phase` is used as a HashMap key lookup only (no SQL interpolation in the scoring path). The rebuild SQL uses `q.phase` as a group-by value, not as a filter parameter. No injection risk in the scoring path. The rebuild SQL binds only `lookback_days` (an integer) as a parameter — no user-supplied strings reach the SQL.
- **`result_entry_ids` JSON parsing**: The `CAST(json_each.value AS INTEGER)` form processes JSON stored in `query_log`. Malformed JSON in `result_entry_ids` (e.g., non-integer values) would cause `json_each` to produce text values that fail the INTEGER cast — the JOIN produces no rows for that entry, silently. This is graceful degradation, not a security risk.
- **Blast radius if `query_log` is poisoned**: If an adversary can write malicious `phase` strings to `query_log`, they could cause unexpected HashMap keys. Since the table is internal and only affects scoring (not data returned to callers), the blast radius is limited to search ranking manipulation — no data exfiltration, no code execution.
- **`lookback_days` config**: Operator-controlled via TOML; not an external input. Range validation `[1, 3650]` prevents degenerate window values (R-08).

**Assessment**: Low security risk. No untrusted external strings reach SQL parameters. The only sensitive input is the integer `lookback_days` which is operator-configured and range-validated.

---

## Failure Modes

| Failure | System Behavior | Detection |
|---------|----------------|-----------|
| Rebuild SQL error (transient) | Retain existing handle state; `tracing::error!` emitted; next tick retries | Log monitoring |
| `query_log` empty / all-NULL phase | `use_fallback = true`; scores identical to pre-col-031 | No user-visible degradation |
| `PhaseFreqTable::rebuild` returns empty rows | `use_fallback = true` (FR-04); same as above | No user-visible degradation |
| Lock poison | `.unwrap_or_else(|e| e.into_inner())` recovers; scoring continues with poisoned-but-recovered state | Rust poison recovery; no panic |
| `lookback_days = 0` passed through | `validate()` should reject; if not, SQL window is zero seconds — zero rows, `use_fallback = true` | AC validates range; R-08 covers this |
| `current_phase = None` at query time | Lock never acquired; `phase_explicit_norm = 0.0`; pre-col-031 score identity | No user-visible degradation |
| Background tick fails to fire | Stale table retained (possibly cold-start); scoring degrades to pre-col-031 | Existing tick health monitoring |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: `run_single_tick` direct construction bypasses `ServiceLayer` | R-01, R-14 | ADR-005: `PhaseFreqTableHandle` is required non-optional at all construction sites; missing wiring is a compile error. Pattern #3213 enumerates all sites. |
| SR-02: `w_phase_explicit = 0.05` raises sum to 1.02; future sum-assertion risk | R-11 | FusionWeights doc-comment updated to state `0.95 + 0.02 + 0.05 = 1.02`; `validate()` unchanged per ADR-004 additive exemption. |
| SR-03: AC-12 / AC-16 non-separability; vacuous gate risk | R-02, R-11 | ADR-004 / NFR-05: AC-16 is a hard delivery prerequisite for AC-12. Gate 3b must reject AC-12 PASS without evidence of non-null `current_phase` in eval output. |
| SR-04: Phase rename strands historical data silently | R-10 | Accepted. Cold-start fallback (`use_fallback = true`) is the only recovery. Documented in CON-09 and `phase_affinity_score` doc comment as expected operational behavior. |
| SR-05: `lookback_days = 30` is session-frequency-dependent, not cycle-representative | R-08 | ADR-002: TOML-configurable; range-validated `[1, 3650]`; #409 owns cycle-aligned GC as the correct long-term successor. |
| SR-06: Two cold-start values from one method; PPR caller confusion risk | R-03, R-04 | ADR-003: `phase_affinity_score` returns `1.0` on cold-start (PPR neutral). Fused scoring guards on `use_fallback` before calling the method. Doc comment (AC-17) names both callers explicitly. |
| SR-07: Lock acquisition order implicit; future deadlock risk | R-12 | Architecture requires code comment at tick lock sequence site naming the required order. No compile-time enforcement exists; code review is the gate. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | Integration test proving non-zero `phase_explicit_norm` after real tick; eval output inspection confirming non-null `current_phase`; Gate 3b process enforcement |
| High | 6 (R-03, R-04, R-05, R-06, R-07, R-14) | AC-11 (3 unit tests), AC-07 (3 unit tests), AC-08 integration test, AC-13/AC-14 normalization tests, lock-hold code review, CI compilation |
| Medium | 5 (R-08, R-09, R-11, R-13, R-10) | Validation boundary tests, AC-04 failure retention, AC-12 eval gate run, type inspection, phase-rename unit test |
| Low | 1 (R-12) | Code comment presence check; lock-order static audit |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned failures gate rejection — found #2758 (Gate 3c grep non-negotiables), #3579 (Gate 3b: mandatory tests absent in delivery wave). Informs R-02 and R-14 risk elevation.
- Queried: `/uni-knowledge-search` for RwLock Arc handle threading — found #1560 (background-tick Arc/RwLock pattern), #3213 (run_single_tick direct construction pattern), #2961 (Arc startup threading via ServiceLayer). Informs R-01 severity (High/High confirmed by lesson #3216).
- Queried: `/uni-knowledge-search` for json_each query_log scoring — found #3681 (ADR: CAST form), #3678 (json_each integer cast pattern). Informs R-05.
- Queried: `/uni-knowledge-search` for eval harness AC-12 AC-16 — found #3688 (ADR-004: AC-16 non-separable). Confirms R-02 Critical rating.
- Stored: nothing novel — all patterns already present in Unimatrix (#1560, #3213, #3678). The `run_single_tick` bypass risk is already captured as pattern #3213 and lesson #3216. No duplicate storage warranted.
