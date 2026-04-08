# Risk-Based Test Strategy: crt-050

GH Issue: #542

---

## SR-01 Conflict Resolution

The spawn prompt identifies a direct conflict between ADR-005 (architect) and the
spec writer's C-02/AC-SV-01 on whether `observations.input` is double-encoded on
the hook-listener write path. This was resolved by reading the actual source at
`crates/unimatrix-server/src/uds/listener.rs` lines 2686–2697.

**Verdict: Architect (ADR-005) is correct. The spec writer's C-02 is wrong.**

The write path for `PreToolUse` events:
```rust
let input = event.payload
    .get("tool_input")
    .map(|v| serde_json::to_string(v).unwrap_or_default());
```

`event.payload.get("tool_input")` returns a `&serde_json::Value`. For a
`context_get` call, `tool_input` is `Value::Object({"id": 42})`.
`serde_json::to_string(Value::Object{...})` serializes to the string `'{"id":42}'`.
This is stored as `Option<String>` in `ObservationRow.input` — a plain JSON object
string, NOT a double-encoded string.

The spec writer misidentified the source of double-encoding. The two-branch read
path in `knowledge_reuse.rs` lines 76–103 (`Some(Value::String(s)) =>
serde_json::from_str(s)`) is a **read-path** artifact: when the stored string
`'{"id":42}'` is loaded back into memory as `ObservationRecord.input`, the Rust
type is `Option<serde_json::Value>` and it is re-wrapped as
`Value::String(raw_json_string)` for in-memory compat. The stored bytes in SQLite
are never double-encoded.

**Consequence:** Pure-SQL `json_extract(o.input, '$.id')` is valid for all
hook-path rows. ADR-005 stands. The spec's FR-07, C-02, and AC-SV-01 constraints
requiring a pre-implementation ADR are already satisfied by ADR-005. The spec's
claim that these rows constitute "the majority of production observations" and are
silently excluded is incorrect — they are correctly included by the pure-SQL path.

**Action for spec writer:** C-02, FR-07 option language, and AC-SV-01 all describe
a problem that does not exist in the merged codebase. They should be updated or
superseded in a spec correction before the implementation agent begins work.

---

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Spec C-02/AC-SV-01 incorrectly blocks implementation pending a two-phase extraction decision already resolved by ADR-005 | High | Confirmed | Critical |
| R-02 | `outcome_weight()` private to `phase_freq_table.rs` duplicates `infer_gate_result()` substring logic; vocabulary drift creates silent semantic divergence | Med | Med | High |
| R-03 | Mixed-weight bucket ordering: rows within the same `(phase, category)` bucket from different-weight cycles break the rank-normalization invariant | High | Med | High |
| R-04 | `min_phase_session_pairs` threshold (FR-17/AC-14) set too low hides sparse-signal quality problems; set too high causes spurious `use_fallback` in dev environments | Med | Med | High |
| R-05 | `MILLIS_PER_DAY` constant binding produces correct arithmetic but the constant value itself could be set wrong (86_400 vs 86_400_000) | Med | Low | Med |
| R-06 | `query_log_lookback_days` struct-literal rename (SR-04) silently misses test fixtures not caught by grep; serde alias does not cover Rust syntax | Med | Low | Med |
| R-07 | `phase_category_weights()` normalized-bucket-size formula uses `bucket.len()` (entry count) rather than sum of weighted freqs; misrepresents outcome-weighted distribution | Med | Med | Med |
| R-08 | NULL `feature_cycle` on pre-col-022 sessions silently drops all outcome rows for those sessions; not an error, but creates unweighted noise in historical signal | Low | Med | Low |
| R-09 | W3-1 (ASS-029) may need `phase_category_weights()` from a different crate; `pub` on `PhaseFreqTable` in `unimatrix-server` is not cross-crate accessible | Low | Med | Low |
| R-10 | `hook_event` vs `hook` column name: SCOPE.md draft SQL uses `hook_event` (resolved in ADR-007, but the SCOPE.md text is the natural reference for implementers) | Med | Med | Med |
| R-11 | No index on `observations.hook` or `observations.phase`; PreToolUse filter + phase IS NOT NULL requires full-scan within ts_millis window | Low | High | Med |
| R-12 | Outcome vocabulary: unrecognized strings default to weight 1.0 silently; a new outcome string ("abandon", "skip") that should map to 0.5 is treated as a pass | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Spec C-02/AC-SV-01 incorrectly describes a double-encoding problem that does not exist

**Severity**: High
**Likelihood**: Confirmed (the error is in the committed spec document)
**Impact**: Implementation agent follows C-02, implements two-phase SQL+Rust extraction,
adds unnecessary complexity, and submits a PR that ADR-005 contradicts. Gate review
would reject for spec-architecture mismatch. Alternatively, the implementation agent
ignores C-02 citing ADR-005 and leaves the spec contradiction unresolved — the gate
would flag an unresolved spec inconsistency.

**Test Scenarios**:
1. Write a unit test that inserts a `PreToolUse` observation via `extract_observation_fields`
   (using the actual write path) and then queries it with `json_extract(input, '$.id')` —
   assert the ID is returned non-NULL. This test validates the write-path contract in CI and
   will catch any future regression that introduces double-encoding.
2. Integration test: insert an observation for `context_get` with `{"id": 42}` through the
   full hook-listener write path; run Query A; assert entry 42 is returned in the result.

**Coverage Requirement**: One write-path contract test in `uds/listener.rs` tests
asserting that `obs.input` for a `PreToolUse` `context_get` event is
`Some("{\"id\":42}")` (not `Some("\"{\\\\\\"id\\\\\\":42}\"")`) must exist and pass
before the spec inconsistency is closed.

---

### R-02: `outcome_weight()` duplicate of `infer_gate_result()` vocabulary

**Severity**: Medium
**Likelihood**: Medium — outcome vocabulary has been stable, but "unknown" and empty
strings are both legitimate and silently treated as weight 1.0 by `outcome_weight()`,
while `infer_gate_result()` would classify them as `Unknown` (which maps to no
outcome, not pass). A future outcome string like "partial" would be treated as weight
1.0 (no match → default) when a 0.5 weighting would be semantically correct.

**Impact**: Phase weights drift silently upward as new outcome vocabulary is
introduced, making rework cycles appear as clean-pass signal.

**Test Scenarios**:
1. Unit test `outcome_weight()` for all currently-known production outcome strings:
   "PASS", "pass", "Pass", "REWORK", "rework", "Rework", "FAIL", "fail",
   "FAILED", "abandoned", "unknown", "". Assert 1.0/0.5/0.5/0.5/0.5/1.0/1.0/1.0
   respectively (aligned with `infer_gate_result` priorities).
2. Cross-reference test: assert `outcome_weight("rework")` == 0.5 and
   `outcome_weight("FAIL")` == 0.5, and that the priority order (rework check before
   fail check) handles hypothetical "rework-and-fail" strings correctly.
3. Doc comment in `outcome_weight()` must reference `infer_gate_result()` in
   `tools.rs` (col-026 R-03) as the canonical vocabulary authority.

**Coverage Requirement**: `outcome_weight()` must have exhaustive tests against all
production outcome strings currently emitted by the protocol layer. Any new outcome
vocabulary addition must update both `infer_gate_result()` and `outcome_weight()`.

---

### R-03: Mixed-weight bucket ordering breaks rank-normalization invariant

**Severity**: High
**Likelihood**: Medium — occurs whenever observations from pass and rework cycles
both contribute to the same `(phase, category)` bucket.

**Impact**: The rank-normalization formula (col-031 ADR-001 #3685) depends on rows
being sorted descending by `freq` within a bucket. If outcome weighting is applied
per-row before sorting, but the weight map is aggregated to per-phase mean (ADR-001
ADR-003), then all rows in a bucket share the same per-phase weight — the multiplied
freqs preserve relative ordering and the invariant holds. If, however, the
implementation accidentally applies per-cycle weights (keyed by `(phase, feature_cycle)`)
rather than per-phase mean weights, rows in the same bucket could have different
multipliers, scrambling the ordering.

**Concrete scenario**: Phase "delivery" has two cycles: cycle-A (pass, weight 1.0)
and cycle-B (rework, weight 0.5). Entry X was read 10 times in cycle-A and 8 times
in cycle-B. Entry Y was read 6 times in cycle-A and 9 times in cycle-B.
- Correct (per-phase mean weight = 0.75): X weighted freq = 18×0.75=13.5,
  Y = 15×0.75=11.25 — X ranks higher, correct.
- Incorrect (per-cycle weights): X = 10×1.0 + 8×0.5 = 14, Y = 6×1.0 + 9×0.5 = 10.5
  — same ordering here, but edge cases exist where per-cycle weights invert rank order
  relative to per-phase mean.

**Test Scenarios**:
1. Unit test with two cycles for the same phase at different weights: verify that
   `apply_outcome_weights` produces per-phase mean weights (not per-cycle weights) and
   that weighted freqs preserve the natural ranking from total read counts.
2. Unit test with a known ordering inversion scenario under incorrect per-cycle
   weighting: confirm the correct per-phase mean approach does not invert the ranking.

**Coverage Requirement**: `apply_outcome_weights()` must be tested with mixed-weight
cycles sharing a phase, asserting the per-phase mean aggregation strategy is used and
the output `Vec<PhaseFreqRow>` freqs are consistently ordered within each
`(phase, category)` group.

---

### R-04: `min_phase_session_pairs` threshold — sparse-signal masking vs spurious fallback

**Severity**: Medium
**Likelihood**: Medium — dev/test environments have few observations; production
environments may have sparse phase coverage if agents run few explicit reads per
phase.

**Impact**: Threshold too low (e.g., 1): sparse-data phase weights with 1 session
feed into scoring pipeline with no warning, producing degenerate category affinities.
Threshold too high (e.g., 50): dev environments and first-run production always
trigger `use_fallback = true`, hiding bugs and making the table appear cold when data
exists.

The architecture (ADR-003 OQ-3) sets default to 5; the spec (FR-17/NFR-04) suggests
10. There is a disagreement between architecture and spec on the default value — the
implementer will face conflicting guidance.

**Test Scenarios**:
1. AC-14 scenario: Set threshold = N. Insert N-1 distinct `(phase, session_id)` pairs;
   assert `use_fallback = true` and warning emitted.
2. AC-14 scenario: Insert exactly N pairs; assert `use_fallback = false` (assuming
   non-empty observations above the pair threshold).
3. Edge case: threshold = 1 with 1 pair; assert normal operation (not spurious fallback).
4. Edge case: all observations belong to a single session (1 pair regardless of
   observation count); assert threshold behavior applies.

**Coverage Requirement**: Spec and architecture must align on the default value before
implementation. The threshold must be tested at boundary ±1. Advisory warning emission
must be tested separately from `use_fallback` gate behavior.

---

### R-05: `MILLIS_PER_DAY` constant value correctness

**Severity**: Medium
**Likelihood**: Low — the constant is `86_400 * 1_000` which is self-evidently
correct from the names. However, the ADR-006 pattern moves arithmetic from SQL into
Rust where it is testable but not type-checked.

**Impact**: If the constant is set to `86_400` (seconds) instead of
`86_400_000` (milliseconds), the lookback window becomes 1000× wider — approximately
82 years of history for a 30-day window. No error is logged. All observations pass
the filter. The symptom is an implausibly large rebuild result.

**Test Scenarios**:
1. Unit test the lookback boundary computation directly: for lookback_days = 30,
   assert `cutoff_millis` equals `now_millis - 2_592_000_000i64` (within 1 second
   margin for test timing).
2. Boundary test: insert observation at exactly `cutoff + 500ms` (inside) and
   `cutoff - 500ms` (outside); assert only the inside observation returns.
3. Assert `MILLIS_PER_DAY == 86_400_000i64` as a compile-time constant test.

**Coverage Requirement**: The constant must have a named-value assertion test. The
lookback boundary formula must have a boundary-condition test with ms-precision.

---

### R-06: Config field rename — struct-literal sites not covered by serde alias

**Severity**: Medium
**Likelihood**: Low — the compiler will reject struct literals using the old field
name. However, if the grep audit misses a test file that uses `..Default::default()`
with the old field name in a way that compiles due to default, the test would silently
stop exercising the renamed field.

**Impact**: Tests using `InferenceConfig { query_log_lookback_days: 30, ..Default::default() }`
become compile errors (caught). Tests using partial struct update syntax could
theoretically pass with wrong values if the default matches what was intended.

**Test Scenarios**:
1. AC-10 serde alias test: deserialize `{"query_log_lookback_days": 30}` and assert
   `config.phase_freq_lookback_days == 30`.
2. AC-10 serde new name test: deserialize `{"phase_freq_lookback_days": 30}` and
   assert `config.phase_freq_lookback_days == 30`.
3. Grep-based gate: CI `grep -r 'query_log_lookback_days' --include='*.rs'` should
   return only the `#[serde(alias)]` annotation and no other references after the rename.

**Coverage Requirement**: Both serde alias and direct field name deserialization must
be tested. All struct-literal sites must be updated before merge (compiler enforces).

---

### R-07: `phase_category_weights()` formula uses entry count, not weighted-freq sum

**Severity**: Medium
**Likelihood**: Medium — ADR-008 specifies normalized bucket size as
`bucket.len() / total_entries_for_phase`. This counts entries per category, not
reads per category. If entry X was read 10 times (weighted freq 10) and entry Y was
read 1 time (weighted freq 1), both are in the same bucket and both contribute `1`
to `bucket.len()`. The weight map does not reflect actual read frequency within
the category — it reflects category breadth (distinct entries accessed), not depth
(how often entries were accessed).

**Impact**: A phase that accessed 1 entry in category A 100 times and 10 entries in
category B each once would compute weights A=1/11, B=10/11 — inverting the signal.
W3-1 cold-start would incorrectly prioritize category B.

**Test Scenarios**:
1. Unit test with unequal read frequencies: one entry in category A with freq=10,
   ten entries in category B with freq=1 each. Assert `phase_category_weights()`
   returns A-weight < B-weight (current ADR-008 behavior) and document this
   explicitly as "categorical breadth", not "read intensity".
2. Verify the formula is documented in the method doc comment so W3-1 implementers
   understand they are receiving a breadth-based distribution.

**Coverage Requirement**: The formula choice (breadth vs. weighted-sum) must be
explicitly tested and documented. If W3-1 needs a weighted-sum projection instead,
this is a known design decision requiring a separate accessor or ADR amendment.

---

### R-08: NULL `feature_cycle` sessions produce unweighted signal without diagnostic

**Severity**: Low
**Likelihood**: Medium — pre-col-022 sessions have NULL `feature_cycle`. ADR-001
handles this via `s.feature_cycle IS NOT NULL` in Query B, producing zero outcome
rows for those sessions, defaulting to weight 1.0.

**Impact**: Historical observations from before col-022 contribute unweighted (1.0)
to phase frequency counts. This is a silent, correct degradation — but there is no
diagnostic indicating how many observations are affected.

**Test Scenarios**:
1. FR-10/AC-15 test: populate sessions with NULL feature_cycle; insert observations;
   assert rebuild completes, returns non-empty table, all rows weighted 1.0,
   `use_fallback = false`.
2. Mixed test: some sessions with feature_cycle (weighted), some without (unweighted);
   assert the weighted sessions produce correct weights and the unweighted sessions
   contribute 1.0, without error.

**Coverage Requirement**: AC-15 integration test is sufficient. A diagnostic warning
when NULL feature_cycle sessions are present is not required (this is expected
historical data), but the test must confirm graceful handling.

---

### R-09: `phase_category_weights()` visibility deferred to W3-1

**Severity**: Low
**Likelihood**: Medium — W3-1 (ASS-029) will consume this method. If W3-1 is
implemented in a separate crate or spawn context, `pub` on `PhaseFreqTable` in
`unimatrix-server/src/services/` is not accessible cross-crate.

**Impact**: W3-1 discovery of the visibility problem causes a rework loop at W3-1
implementation time. The method exists but cannot be called without a visibility
change (re-export, public crate API, or crate restructuring).

**Test Scenarios**:
1. AC-08 test validates the method returns correct values — visibility is confirmed
   within `unimatrix-server`.
2. At W3-1 scoping time: explicitly check whether W3-1 needs `phase_category_weights()`
   from outside the `unimatrix-server` crate and add a tracked issue if so.

**Coverage Requirement**: Document the visibility limitation as a tracked open item
(SR-07 / C-10 in spec) with a W3-1 issue reference. No blocking test required now.

---

## Integration Risks

### Query A × entries JOIN: CAST and json_extract correctness

The JOIN predicate `CAST(json_extract(o.input, '$.id') AS INTEGER) = e.id` has three
independent failure modes, each silent:
- Missing CAST: text-integer mismatch returns zero rows (col-031 R-05, pattern #3692)
- Missing `json_extract` IS NOT NULL guard: filter-based lookups with no `id` field
  produce NULL JOINs that match no entry rows (not an error, just excluded)
- Wrong column name `hook_event` instead of `hook`: runtime SQL error (ADR-007)

All three must be tested independently (AC-02, AC-03, AC-14 in spec; FR-02–FR-05
in spec).

### Two-query temporal consistency

Query A and Query B execute in separate async calls. Between calls, a background
tick could write new `cycle_events` rows. In practice the tick runs sequentially and
`rebuild()` holds no lock during the two queries — this is a negligible TOCTOU
window given tick cadence. No mitigation required, but the implementer should not
wrap both queries in a transaction unless the store layer explicitly supports it.

### Background tick timeout

`PhaseFreqTable::rebuild()` runs inside `run_single_tick`. Adding two SQL queries
plus a Rust post-process adds latency. Query A aggregates across the entire lookback
window (potentially years of observations). NFR-01 requires no tick latency
regression. An unindexed `observations.hook` column (ADR-007 notes no index on
`hook`) means the PreToolUse filter is applied post-index-scan on `ts_millis`. This
is acceptable given the ts_millis index narrows the window first, but must be
validated at staging scale.

---

## Edge Cases

- **Empty observations, non-empty cycle_events**: Query A returns empty vec →
  `use_fallback = true` before Query B is evaluated. Query B result is irrelevant.
  Test: AC-01 scenario (b).

- **Non-empty observations, empty cycle_events**: Query B returns empty vec →
  weight map empty → all rows weighted 1.0. `use_fallback` depends on pair count
  threshold. Test: AC-05.

- **All observations from the same session, all same phase**: coverage count = 1
  distinct `(phase, session_id)` pair. If threshold = 5, this triggers
  `use_fallback`. Test: R-04 edge case 4.

- **Phase string casing**: "Delivery" vs "delivery" vs "DELIVERY" are distinct keys
  in the HashMap. If the protocol emits mixed case, the table is fragmented. This is
  existing behavior inherited from the current `query_log` path — not introduced by
  this feature — but worth documenting as a known operational concern.

- **Entry deleted between observation write and rebuild**: `CAST(json_extract(input, '$.id') AS INTEGER) = e.id` JOIN silently excludes deleted entries (they have no row in `entries`). This is correct behavior — orphaned observations produce no weight row — but should be noted for debuggability.

- **Outcome strings with both "rework" and "fail" substrings**: `outcome_weight()`
  checks rework before fail (ADR-003 priority order). A hypothetical string like
  "rework_fail" returns 0.5 from the rework branch, not double-penalized. Test:
  included in R-02 scenario 2.

- **`phase_category_weights()` called with a single-category phase**: weight = 1.0
  for that category. Formula: `1 / 1 = 1.0`. Test: AC-08 edge case.

---

## Security Risks

- **Untrusted input in `observations.input`**: `json_extract(o.input, '$.id')` is a
  read-only SQL expression on stored data. The data was written by the hook-listener
  from `tool_input` payloads originating from the Claude agent's tool calls. An agent
  could craft a `context_get` call with `{"id": 99999999}` to attempt to inject a
  non-existent entry ID — the JOIN on `entries.id` silently excludes it. No injection
  risk, no blast radius beyond a spurious zero-row result for that observation.

- **`observations.input` is not sanitized**: `serde_json::to_string(v)` on arbitrary
  tool_input is sound — it produces valid UTF-8 JSON strings. No SQL injection vector
  exists because the value is stored as a column value and extracted via `json_extract`,
  not interpolated into SQL. No risk.

- **`phase_category_weights()` leaks category distribution**: This method is only
  called at GNN initialization time within the same process. No external API surface.
  No risk.

- **Query B joins across sessions/cycle_events**: No external input feeds these
  tables directly in the rebuild query context. No injection risk.

---

## Failure Modes

| Failure | Expected Behavior |
|---------|------------------|
| Query A store error | `rebuild()` returns `Err`; caller retains previous table (retain-on-error); `tracing::error!` emitted |
| Query B store error | Same as Query A — both queries' errors must propagate to caller as `Err`, not silently drop |
| `PhaseFreqTable` RwLock poisoned | `.unwrap_or_else(|e| e.into_inner())` on all acquisitions; poison recovered |
| `use_fallback = true` at search time | Fused scoring sets `phase_explicit_norm = 0.0`; PPR receives `1.0` neutral score; no search disruption |
| DB unavailable during tick | retain-on-error; previous table serves search path; tick logs error; next tick retries |
| Observations coverage below threshold | `use_fallback = true`; `tracing::warn!` with count and threshold; rebuild stops early; search degrades gracefully |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 | R-01 | RESOLVED — ADR-005 confirms no double-encoding on write path. Spec C-02 / AC-SV-01 contains an incorrect assertion that must be corrected before implementation. Residual risk: the spec correction must happen before delivery to prevent implementation agent confusion. |
| SR-02 | R-05 | MITIGATED — ADR-006 extracts the scaling into `MILLIS_PER_DAY: i64 = 86_400 * 1_000` constant with doc comment; arithmetic moved to Rust where it is testable. Residual risk: constant value still requires a named-value assertion test. |
| SR-03 | R-04 | PARTIALLY MITIGATED — AC-14/FR-17 introduce a coverage threshold gate. Architecture default (5) and spec suggestion (10) are misaligned; implementer needs a single authoritative value. Residual risk: threshold choice is advisory, not calibrated against production observation density. |
| SR-04 | R-06 | MITIGATED — ADR-004 confirms rename + serde alias approach. Compiler enforces struct-literal updates. Residual risk: low; grep gate in CI provides defense in depth. |
| SR-05 | R-08 | MITIGATED — ADR-001 Query B includes `s.feature_cycle IS NOT NULL` predicate; NULL sessions silently contribute weight 1.0. AC-15 integration test covers this. |
| SR-06 | R-02 | ACCEPTED WITH CONDITIONS — ADR-003 chose inline `outcome_weight()` with priority ordering consistent with `infer_gate_result()`. Residual risk: vocabulary drift. Mitigation: doc comment cross-reference + exhaustive vocabulary test. |
| SR-07 | R-09 | DEFERRED — visibility limitation acknowledged in ADR-008 / spec C-10. No blocking risk for crt-050; tracked open item for W3-1 scoping. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 1 (R-01) | Spec correction + write-path contract test + integration smoke test |
| High | 2 (R-02, R-03) | 5 unit tests (vocabulary coverage, mixed-weight ordering) |
| Medium | 5 (R-04–R-07, R-10/R-11) | 12 unit/integration tests (boundary, formula, rename, index) |
| Low | 4 (R-08, R-09, R-12, runtime failures) | 3 integration tests + failure mode verification |

**Total required test scenarios across all risks: ~20 distinct test cases** (many
overlap with AC-13 sub-items in the specification).

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `lesson-learned failures gate rejection` — found entries #3935, #4147, #3580, #3347, #4155 (gate failures around tracing tests, file size, silent write failures). None directly applicable to crt-050 domain.
- Queried: `/uni-knowledge-search` for `risk pattern PhaseFreqTable observations json_extract` — found entry #4222 (observations→PhaseFreqTable SQL mandatory constraints, directly applicable). Also found #3685 (rank-based normalization formula, used for R-03 analysis).
- Queried: `/uni-knowledge-search` for `outcome weight infer_gate_result rework` — found #4225 (ADR-003 crt-050 outcome weighting decision, confirmed R-02 residual risk characterization).
- Stored: nothing novel to store — R-02 (duplicate outcome vocab drift pattern) is specific to the crt-050 two-site problem; not yet visible as a cross-feature pattern.
