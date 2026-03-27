# Risk-Based Test Strategy: col-031

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `json_each` integer cast produces no rows or wrong rows — frequency table silently empty | High | Med | Critical |
| R-02 | Cold-start semantic drift: `use_fallback=true` returns `1.0`, not `0.0`, causing a uniform 0.05 score shift when `current_phase` is set — ranking-preserving but not bit-for-bit identical to pre-col-031 | High | High | Critical |
| R-03 | Lock ordering violation: scoring loop holds `PhaseFreqTableHandle` read lock into scoring work, blocking background tick write | High | Low | High |
| R-04 | Eval gate declared vacuous PASS — AC-12 passes because AC-16 incomplete; `phase_explicit_norm` never exercised in eval replay | High | Med | Critical |
| R-05 | AC-16 and scoring activation treated as independently shippable; delivery wave ships one without the other | High | Med | Critical |
| R-06 | Retention window subquery counts distinct `feature_cycle` values by row recency, not calendar time — open cycles always included, boundary behavior untested | Med | Med | High |
| R-07 | `phase_affinity_score` linear scan on Vec sorted by score (not entry_id) produces incorrect lookup for entries at mid-bucket | Med | Med | High |
| R-08 | `w_phase_explicit = 0.05` default change breaks existing tests that assert the old `0.0` default | Med | High | High |
| R-09 | `FusionWeights` sum-check comment not updated — future calibration work uses stale `0.97` total | Low | Med | Medium |
| R-10 | Phase vocabulary mismatch (case, renamed phase) silently degrades to neutral — operator has no observability into mismatch | Med | Low | Medium |
| R-11 | Single-entry bucket normalization edge case: `1.0 - (0/1) = 1.0` makes top-and-only entry indistinguishable from absent entry | Low | Med | Medium |
| R-12 | Background tick rebuild failure retains stale state with no operator visibility beyond `tracing::error!` — could persist across many ticks unnoticed | Med | Low | Medium |
| R-13 | `PhaseFreqRow` struct exported from `unimatrix-store` — public API surface grows; future breaking changes to the struct propagate to `unimatrix-server` | Low | Low | Low |
| R-14 | Tick wall-time budget exceeded by SQL aggregation on large `query_log` (>20K rows, cold SQLite page cache) | Med | Low | Medium |

---

## Risk-to-Scenario Mapping

### R-01: `json_each` integer cast produces no rows or wrong rows
**Severity**: High
**Likelihood**: Med
**Impact**: Frequency table silently empty on every rebuild; `phase_explicit_norm` always `0.0`; scoring appears to work but the feature is inert. Unimatrix #3678 confirms this is a recurring implementation-surprise pattern.

**Test Scenarios**:
1. Integration test (AC-08): seed `query_log` with a known `result_entry_ids = [42]` JSON array; call `Store::query_phase_freq_table(20)`; assert the result contains a row for `entry_id=42`. If the cast form is wrong, this returns zero rows.
2. Negative case: seed a row with `result_entry_ids = []` (empty array); assert rebuild produces no rows for that log entry and does not panic.
3. Property test: seed multiple rows with varied `result_entry_ids` arrays; assert the total `freq` count matches the expected per-entry occurrence count.

**Coverage Requirement**: The AC-08 integration test must run against a real SQLite `TestDb`, not a mock. A mock cannot catch `json_each` expansion failures.

---

### R-02: Cold-start semantic drift — uniform `1.0` score shift vs. `0.0` pre-col-031
**Severity**: High
**Likelihood**: High
**Impact**: When `current_phase` is set and the table is cold-start, all candidates receive `phase_explicit_norm = 1.0` rather than `0.0`. This adds a uniform `+0.05` offset to all fused scores. Relative ranking is preserved, but absolute scores differ from pre-col-031. AC-11 as written requires "bit-for-bit identical" — this is unachievable when `current_phase` is set. SPECIFICATION.md NFR-03 acknowledges the distinction but implementation must not mistake ranking-preservation for score identity.

**Test Scenarios**:
1. Unit test (AC-11 — `current_phase = None` path): run scoring with `current_phase = None` and any table state; assert `phase_explicit_norm == 0.0` for all candidates; assert fused scores match pre-col-031 reference values exactly.
2. Unit test (cold-start + `current_phase = Some("delivery")`): assert `phase_affinity_score` returns `1.0` for all entries; assert `phase_explicit_norm = 1.0` for all candidates; assert relative ordering of fused scores is unchanged from pre-col-031 (not absolute score equality).
3. Code-comment verification: `phase_freq_table.rs` must document the cold-start/ranking-preservation distinction vs. score identity — reviewer check.

**Coverage Requirement**: AC-11 must explicitly test both the `None` path (score identity) and the `Some` + cold-start path (ranking preservation). Conflating them yields a false pass.

---

### R-03: Lock ordering violation — scoring loop holds read lock into scoring work
**Severity**: High
**Likelihood**: Low
**Impact**: If the `PhaseFreqTableHandle` read lock is held through the scoring loop (rather than released before it), the background tick write blocks for the entire scoring pass. Under concurrent search load this degrades tick frequency. In the worst case, if another handle is acquired while this lock is held, deadlock risk appears. Unimatrix #3682 documents the mandated ordering.

**Test Scenarios**:
1. Code review: confirm that in `search.rs` the `PhaseFreqTableHandle` read guard is dropped (via scope close) before the `for candidate in candidates` loop begins.
2. Code review: in `background.rs`, confirm that the TypedGraphState swap block's scope closes before the PhaseFreqTable swap block opens — no nested lock acquisition.
3. Concurrency stress test: spawn N concurrent search tasks and one background tick task; assert no deadlock or timeout within the existing tick timeout.

**Coverage Requirement**: Lock ordering must be auditable via code review alone. Any `let guard = handle.read()` that survives into a scoring loop is a violation.

---

### R-04: Eval gate vacuous PASS — AC-12 passes before AC-16 is complete
**Severity**: High
**Likelihood**: Med
**Impact**: If `extract.rs` is not updated to select `query_log.phase`, every eval scenario has `current_phase = None`, causing `phase_explicit_norm = 0.0` for all candidates in all scenarios. AC-12 then measures pre-col-031 scoring behavior, not col-031 scoring behavior. The gate passes trivially — not because the feature is safe but because it was never activated. Unimatrix #3555 documents this gap; SCOPE.md Open Question 5 mandates the fix in-scope.

**Test Scenarios**:
1. Verification of AC-16 before AC-12 run: inspect generated scenario JSONL for `current_phase` field presence and non-null values for post-col-028 rows.
2. Spot-check: at least one eval scenario must have `current_phase != null`; if all are null, AC-16 is not complete and AC-12 must not be declared passing.
3. Eval harness run (AC-12): after AC-16 confirmed complete, run with phase-aware scenarios; assert MRR ≥ 0.35, CC@5 ≥ 0.2659, ICD ≥ 0.5340.

**Coverage Requirement**: AC-12 verdict must be explicitly conditional on AC-16 verification. Gate report should include a pre-check: "scenarios contain non-null `current_phase`? Y/N."

---

### R-05: AC-16 and scoring activation shipped as separate waves
**Severity**: High
**Likelihood**: Med
**Impact**: A delivery wave that ships `w_phase_explicit = 0.05` (scoring activated) before `extract.rs` is fixed delivers a live scoring change that cannot be regression-tested. A wave that ships only `extract.rs` without the scoring activation is harmless but makes AC-12 irrelevant for that wave. Either partial ship undermines the evaluation contract.

**Test Scenarios**:
1. Wave gate check: before any wave declares gate-3c PASS on AC-12, confirm `extract.rs` emits `current_phase` (AC-16 PASS) is already in the same wave or a preceding wave.
2. CI check: `grep -q 'w_phase_explicit.*0.05'` and `grep -q 'current_phase'` in the generated scenario JSONL must both pass before AC-12 gate is exercised.

**Coverage Requirement**: Delivery protocol must treat AC-12 and AC-16 as a single non-separable acceptance criterion pair. Gate-3c must not pass on AC-12 alone.

---

### R-06: Retention window subquery boundary behavior untested
**Severity**: Med
**Likelihood**: Med
**Impact**: The retention filter uses `LIMIT retention_cycles` over `ORDER BY query_id DESC` on distinct `feature_cycle` values. Edge cases: (a) fewer than `retention_cycles` distinct cycles in the log — must include all; (b) open cycle (NULL `feature_cycle`) is always included; (c) `retention_cycles = 0` — must return zero rows or all-open-cycle rows only, not error. Incorrect boundary behavior causes the table to be spuriously empty or to over-include stale data.

**Test Scenarios**:
1. Integration test: seed `query_log` with exactly 2 distinct completed cycles and 1 open cycle; call `query_phase_freq_table(20)`; assert all 3 cycles' rows are included.
2. Integration test: seed with 25 distinct completed cycles; call with `retention_cycles = 20`; assert only the 20 most recent cycles' rows are included.
3. Integration test: seed with only open-cycle rows (`feature_cycle = NULL`); call with any `retention_cycles`; assert rows are included (open cycle always included).
4. Edge case: call with `retention_cycles = 0`; assert no exception and an empty or open-cycle-only result.

**Coverage Requirement**: SQL boundary conditions must be covered in `unimatrix-store` integration tests, not deferred to manual verification.

---

### R-07: Linear scan on score-sorted Vec produces incorrect `phase_affinity_score` lookup
**Severity**: Med
**Likelihood**: Med
**Impact**: The Vec is stored sorted by descending score (ascending rank). Lookup by `entry_id` requires a linear scan. If the implementation mistakenly assumes the Vec is sorted by `entry_id` and uses binary search, entries at mid-bucket positions return wrong results or `1.0` (neutral fallback). This would cause incorrect scoring without any panic or test failure unless explicitly tested with mid-bucket entries.

**Test Scenarios**:
1. Unit test (AC-13): seed a 5-entry bucket with known rank order; assert rank-0 entry returns `1.0`, rank-2 entry returns `0.6`, rank-4 entry returns `0.2`; assert an entry_id not in the Vec returns `1.0`.
2. Unit test: seed a bucket where entry IDs are not monotonically ordered (e.g., `[300, 1, 50, 99, 7]` by rank); assert lookup by each entry_id returns its correct rank score regardless of ID order.
3. Property test: for any bucket of N entries, assert no `phase_affinity_score` call returns a value outside `[0.0, 1.0]` and no call panics.

**Coverage Requirement**: Test with entry IDs in non-monotonic order to expose any mistaken binary-search assumption.

---

### R-08: `w_phase_explicit = 0.05` default change breaks existing tests
**Severity**: Med
**Likelihood**: High
**Impact**: `test_inference_config_default_phase_weights` currently asserts `w_phase_explicit == 0.0`. The default change from `0.0` to `0.05` will cause this test to fail unless updated. If there are additional tests that embed a `0.0` assertion for this field, they will also fail. Unimatrix #3207 documents the `compute_fused_score` extension pattern; the test update is explicitly called out in SCOPE.md.

**Test Scenarios**:
1. Update `test_inference_config_default_phase_weights` to assert `0.05` (AC-09).
2. Grep the test suite for any hardcoded `w_phase_explicit.*0.0` or `0.0.*w_phase_explicit` assertions outside the updated test; update all found instances.
3. TOML round-trip test: deserialize a TOML section with no `w_phase_explicit` key; assert deserialized value is `0.05`.

**Coverage Requirement**: All existing assertions on `w_phase_explicit` default must be updated before the wave gates. This is a mechanical change with no ambiguity — CI failure is the detection mechanism.

---

### R-09: `FusionWeights` sum-check comment not updated
**Severity**: Low
**Likelihood**: Med
**Impact**: The comment currently documents the sum as `0.95 + 0.02 = 0.97` (or similar). After col-031 the correct total is `0.95 + 0.02 + 0.05 = 1.02`. A stale comment misleads future weight calibration work into believing the remaining additive budget is larger than it is.

**Test Scenarios**:
1. Code review: confirm `FusionWeights` sum-check comment updated to `0.95 + 0.02 + 0.05 = 1.02`.
2. No automated test required — this is a comment, not logic. Code review is the gate.

**Coverage Requirement**: Gate-3a code review checklist must include the `FusionWeights` comment update.

---

### R-10: Phase vocabulary mismatch silently degrades to neutral
**Severity**: Med
**Likelihood**: Low
**Impact**: A phase name mismatch between `current_phase` and `query_log.phase` (e.g., `"Delivery"` vs `"delivery"`, or a renamed phase after a protocol update) causes all entries in that phase to receive neutral score `1.0` with no logged warning. Operators diagnosing unexpected ranking behavior have no signal that the frequency table is being bypassed.

**Test Scenarios**:
1. Unit test: call `phase_affinity_score` with a phase string not present in the table; assert return is `1.0` (neutral) and no panic.
2. Observability check (NFR-08): confirm code comment in `phase_affinity_score` documents the silent-degradation contract.
3. Optional: confirm a `tracing::debug!` log line is emitted when `current_phase` has no match in the table (per OQ-03 decision).

**Coverage Requirement**: Silent degradation is acceptable behavior; the test verifies the behavior is intentional and documented, not absent.

---

### R-11: Single-entry bucket score equals absent-entry score
**Severity**: Low
**Likelihood**: Med
**Impact**: In a single-entry bucket (N=1), `score = 1.0 - (0/1) = 1.0`. An entry present as the sole member of its `(phase, category)` bucket receives `1.0` — identical to the neutral score for absent entries. This is correct per ADR-001 ("a bucket with one entry is 100% confident by revealed preference") but may confuse future weight-tuning work if not understood.

**Test Scenarios**:
1. Unit test: seed a single-entry bucket; assert `phase_affinity_score(entry_id, cat, phase) == 1.0`; assert `phase_affinity_score(other_id, cat, phase) == 1.0` (neutral); confirm both return `1.0` for different reasons — one is present, one is absent. This tests that the implementation does not special-case this scenario incorrectly.

**Coverage Requirement**: Document the single-entry behavior in a code comment on `phase_affinity_score`. Test confirms the arithmetic, not that the two `1.0` values are distinguishable (they are not, by design).

---

### R-12: Background tick rebuild failure silently persists stale state
**Severity**: Med
**Likelihood**: Low
**Impact**: If `Store::query_phase_freq_table` fails (e.g., connection issue, schema inconsistency), the old `PhaseFreqTable` is retained. After one failure this is correct behavior. But if the failure recurs across many ticks, the table could be arbitrarily stale with no operator-visible indication beyond `tracing::error!` log lines that may not be monitored.

**Test Scenarios**:
1. Unit/integration test (AC-04 error path): inject a store error into `rebuild`; assert the handle still contains the pre-failure state; assert `tracing::error!` was emitted.
2. Multi-failure scenario: inject store errors across 3 consecutive tick calls; assert the handle retains the last good state throughout.

**Coverage Requirement**: Error-path test must verify state retention under failure (not just that the error is logged).

---

### R-13: `PhaseFreqRow` public API surface growth
**Severity**: Low
**Likelihood**: Low
**Impact**: Exporting `PhaseFreqRow` from `unimatrix-store` is a minor public API expansion. Future changes to field types (e.g., adding a `weight` field) require coordinated changes in both crates.

**Test Scenarios**:
1. Code review: confirm `PhaseFreqRow` fields match the SQL column types exactly (`entry_id: u64`, `freq: i64` — sqlx maps SQLite INTEGER to `i64`) to avoid silent truncation on the `CAST(je.value AS INTEGER)` boundary.
2. No additional test needed — this is a compile-time contract; type mismatches fail build.

**Coverage Requirement**: Code review only.

---

### R-14: Tick wall-time budget exceeded by SQL aggregation
**Severity**: Med
**Likelihood**: Low
**Impact**: The `<5ms` estimate assumes a warm SQLite page cache at 20K rows. On cold cache (first tick after server restart) with a large `query_log`, the aggregation joins three tables (`query_log`, `json_each`, `entries`) with a subquery. This could consume a material fraction of `TICK_TIMEOUT`. If it exceeds the timeout, the entire tick is cancelled, including all other rebuilds.

**Test Scenarios**:
1. NFR-02 performance test: seed `query_log` with 20,000 rows across multiple phases and categories; measure `PhaseFreqTable::rebuild` wall time; assert < 10% of `TICK_TIMEOUT`.
2. Observability: confirm `tracing::debug!` timing log added for the rebuild step (per SR-07 ARCHITECTURE.md Open Question 2).

**Coverage Requirement**: Performance threshold is 10% of `TICK_TIMEOUT` at 20K rows. If this fails, the SQL query must be optimized (index on `phase`, `feature_cycle`) before shipping.

---

## Integration Risks

**Store-to-service boundary (R-01, R-06)**: The `Store::query_phase_freq_table` method crosses the `unimatrix-store` / `unimatrix-server` crate boundary. The `json_each` expansion and retention window subquery are complex enough that unit tests in `unimatrix-store` alone are necessary — the service layer cannot catch SQL failures with mocks.

**ServiceLayer wiring (AC-05)**: `PhaseFreqTableHandle` must be threaded to both `SearchService` and the background tick. If either path receives a different `Arc` clone (or none), the scoring loop reads stale/empty state permanently. The integration test must confirm that a rebuild in the tick is visible to a subsequent search call.

**`FusedScoreInputs` field wire-up (AC-06)**: The hardcoded `phase_explicit_norm: 0.0` replacement is a single-line change in `search.rs`. The risk is that the replacement is in a code path that is only reached when `params.current_phase` is `Some`, leaving the `None` path's `0.0` as a separate code path that must also be present.

**Eval harness cross-crate dependency (R-04, R-05)**: `eval/scenarios/extract.rs` is in the eval crate. The scoring change is in `unimatrix-server`. These are tested independently. The AC-12 gate bridges them — the risk is in the gate procedure, not the code.

---

## Edge Cases

- **Empty `query_log`**: No phase-tagged rows exist (all pre-col-028, all `phase = NULL`). Rebuild returns empty `Vec<PhaseFreqRow>`; `use_fallback` remains `false` but `table` is empty. `phase_affinity_score` finds no bucket — returns `1.0` (neutral). Equivalent to cold-start for scoring purposes.
- **Single-phase, single-category, single-entry log**: Degenerate bucket; N=1; score=1.0. Scores identical to cold-start for all candidates. No ranking signal emerges until multiple entries appear in a bucket.
- **Very large bucket (>1000 entries in one (phase, category))**: Linear scan is O(N). At 1000 entries, still sub-microsecond on modern hardware, but this should be verified or noted. The architecture notes "typical bucket size <50 entries" — this is an assumption worth validating.
- **`retention_cycles = 1`**: Only the single most recent completed cycle plus open cycles. The frequency table reflects only the last sprint's behavior. This is valid but produces aggressive recency bias. No error expected.
- **Phase string with special characters or Unicode**: Runtime string, no validation. A phase string containing `"delivery/wave-3"` or Unicode characters is a valid key. The HashMap lookup uses exact string equality — no normalization. If `current_phase` differs by case or encoding, silent neutral degradation (R-10).
- **Concurrent PhaseFreqTable reads during write swap**: Multiple search goroutines reading while the tick writes. The `Arc<RwLock>` provides the synchronization guarantee; the test in R-03 scenario 3 covers this.
- **`result_entry_ids = null` in `query_log`**: ADR-003 SQL includes `AND ql.result_entry_ids IS NOT NULL` — these rows are excluded. The `json_each` call on NULL would produce an error in some SQLite versions; the NULL guard prevents it.

---

## Security Risks

**Untrusted input surface**: `PhaseFreqTable` is an internal component; it does not accept external input directly. The risk surface is indirect:

- `query_log.phase`: Written by the MCP server's `context_search`/`context_briefing` tools from the `current_phase` parameter supplied by agent callers. An agent supplying a crafted `current_phase` value cannot inject SQL (the value is used only as a HashMap lookup key at query time, not interpolated into SQL). The SQL reads `query_log.phase` via parameterized query; no injection vector.
- `result_entry_ids`: Written by the MCP server at result-recording time from auto-increment entry IDs (integers). No external string is stored here. The `CAST(json_each.value AS INTEGER)` in the aggregation query handles the expansion.
- **Blast radius if `query_log` is compromised**: An attacker who can write arbitrary rows to `query_log` could poison the frequency table — causing specific entries to appear highly ranked for specific phases. The `entries` JOIN enforces referential integrity (only real entry IDs can appear in the result); phantom entry IDs in `result_entry_ids` produce no rows from the JOIN. Frequency inflation for real entry IDs is a data-integrity concern, not a memory-safety concern. No code execution path exists via this vector.
- **u64 overflow at i64 boundary** (ADR-003): `CAST(json_each.value AS INTEGER)` silently overflows for entry IDs > 2^63-1. Entry IDs are auto-increment starting from 1; this is not a practical concern but is documented.

**Assessment**: No code-execution, path-traversal, or injection risks identified. The primary security risk is data-integrity poisoning via `query_log` manipulation, which is bounded by the `entries` JOIN.

---

## Failure Modes

| Failure | Expected Behavior | Detectable By |
|---------|------------------|---------------|
| `query_phase_freq_table` SQL returns zero rows (json_each bug) | `use_fallback = false`, empty table; all scores `1.0` (neutral) — feature inert but not broken | AC-08 integration test |
| Rebuild throws `StoreError` | Old table retained; `tracing::error!` emitted; next tick retries | AC-04 error path test |
| Lock poisoned (thread panicked holding read lock) | `.unwrap_or_else(|e| e.into_inner())` recovers; scoring continues with last good state | Code review of all lock sites |
| `current_phase` not set in search params | `phase_explicit_norm = 0.0`; scoring identical to pre-col-031 | AC-11 unit test |
| Phase string mismatch (case/rename) | `phase_affinity_score` returns `1.0` (neutral); scoring unaffected; silent degradation | R-10 unit test; NFR-08 code comment |
| Tick timeout exceeded by rebuild | Tick cancelled; all handle states retain prior values; error logged | NFR-02 performance test |
| AC-16 incomplete at AC-12 gate | Eval gate passes trivially on pre-col-031 scoring behavior | AC-16 pre-check in gate procedure |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: `json_each` integer cast portability | R-01 | ADR-003 pins `CAST(json_each.value AS INTEGER)` form; AC-08 integration test validates against real `TestDb`. |
| SR-02: `w_phase_explicit = 0.05` not empirically grounded | R-08 (partially) | ADR-005 accepts risk explicitly; AC-12 is the safety net; default is configurable. |
| SR-03: eval harness fix and scoring activation non-separable | R-04, R-05 | Architecture treats AC-16 + AC-12 as a single deliverable; C-03 constraint documented in SPECIFICATION. |
| SR-04: phase vocabulary mismatch silently degrades | R-10 | NFR-08 requires silent-degradation contract documented in code comments; OQ-03 decides on debug log line. |
| SR-05: #398 PPR concurrency risk | — | Low/Low scope risk; delivery start check per C-08; no architecture risk identified beyond API contract stability. |
| SR-06: three-handle lock ordering on hot path | R-03 | ADR-004 documents mandatory acquisition order; architecture enforces structurally-separated scopes. |
| SR-07: tick wall-time budget risk | R-14 | NFR-02 sets 10% TICK_TIMEOUT performance threshold; SR-07 tracing::debug! instrumentation required. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-04, R-05) | 10 scenarios minimum |
| High | 4 (R-03, R-06, R-07, R-08) | 9 scenarios minimum |
| Medium | 4 (R-09, R-10, R-11, R-12) | 7 scenarios minimum |
| Low | 2 (R-13, R-14) | 3 scenarios minimum |

**Total identified risks**: 14
**Risks with no automated test scenario (code review only)**: R-09, R-13
**Risks requiring integration test against real SQLite TestDb**: R-01, R-06, R-12
**Risks requiring eval harness procedural gate**: R-04, R-05

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found #3579 (gate-3b: mandatory tests absent), #3580 (500-line limit violations at gate), #2758 (gate-3c non-negotiable test name grep). These elevate R-04/R-05 (vacuous gate) and R-08 (test update omission) severity based on observed delivery failures in nan-009.
- Queried: `/uni-knowledge-search` for "risk pattern scoring weight calibration regression" — found #3207 (compute_fused_score extension pattern), #3206 (FusionWeights additive field exemption), #2985 (differential profile tests require extreme values). These inform R-02 and R-08.
- Queried: `/uni-knowledge-search` for "SQLite json_each query_log background tick" — found #3678 (json_each integer array silent wrong-type risk) and #3681 (ADR-003 col-031). Confirms R-01 as Critical based on prior pattern documentation.
- Queried: `/uni-knowledge-search` for "RwLock Arc hot path lock ordering" — found #3682 (ADR-004 col-031 lock ordering). Confirms R-03 architecture decision is in place.
- Queried: `/uni-knowledge-search` for "eval harness extract scenario phase regression gate vacuous" — found #3555 (eval harness phase gap) and #3583 (ADR-005 col-031). Confirms R-04/R-05 are grounded in a known documented gap.
- Stored: nothing novel to store — R-01 pattern already stored as #3678; lock ordering already stored as #3682; vacuous gate pattern evident in #3579/#3580 which are feature-general. No new cross-feature pattern emerges from col-031 risk analysis that is not already captured.
