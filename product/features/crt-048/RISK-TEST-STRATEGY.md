# Risk-Based Test Strategy: crt-048

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `compute_lambda()` call sites mis-ordered after freshness param removal — all remaining params are `f64`, so a wrong argument order compiles silently | High | Med | Critical |
| R-02 | `mcp/response/mod.rs` fixture sites partially updated — a single missed field assignment causes compile failure that blocks all CI gate runs | High | High | Critical |
| R-03 | `DEFAULT_STALENESS_THRESHOLD_SECS` deleted by implementer reading Goal 7 literally without Implementation Notes, causing `run_maintenance()` to silently use a hardcoded literal or fail | High | Med | Critical |
| R-04 | `lambda_weight_sum_invariant` test uses exact `==` comparison on f64 sum of 0.46+0.31+0.23 — may spuriously fail or produce false pass depending on compiler/platform | Med | Med | High |
| R-05 | JSON output breaking change — `confidence_freshness_score` and `stale_confidence_count` removed without migration window — downstream operators using custom scripts are surprised | Med | Low | Medium |
| R-06 | `coherence_by_source` per-source loop has two `compute_lambda()` call sites updated inconsistently — one still passes freshness argument and silently uses wrong positional value | High | Med | Critical |
| R-07 | Re-normalization when `embedding: None` produces incorrect 2-of-2 lambda after 3-dimension base weights are changed — test expected values not updated | Med | Med | High |
| R-08 | `StatusReportJson` struct updated but `From<&StatusReport>` impl retains stale field assignments — compiles if field names match a different field, silently corrupts JSON output | Med | Low | Medium |
| R-09 | `generate_recommendations()` retains a stale reference to `stale_confidence_count` or `oldest_stale_age_secs` via indirect use (e.g., a recommendation string format) even after parameter removal | Low | Low | Low |
| R-10 | ADR-003 (entry #179) not superseded via `context_correct` before feature merges — downstream agents query old weights and use wrong 4-dimension rationale | Med | Med | High |

## Risk-to-Scenario Mapping

### R-01: `compute_lambda()` positional argument mis-ordering
**Severity**: High
**Likelihood**: Med
**Impact**: Lambda is silently computed from wrong values — graph quality score treated as contradiction density or vice versa. Lambda numerics appear plausible, tests pass, but the coherence gate fires on the wrong dimension. No compile error.

**Test Scenarios**:
1. Unit test `lambda_specific_three_dimensions`: supply distinct values for graph (0.8), contradiction (0.3), embedding Some(0.5) and assert the result matches the hand-computed weighted sum using exact weights (0.46, 0.31, 0.23). If arguments are transposed the result is detectably different.
2. Unit test `lambda_single_dimension_deviation`: hold two dimensions at 1.0, vary one dimension independently for each of the three positions; assert each independent deviation produces a different magnitude change. This triangulates that each argument lands in the correct weight slot.
3. Review-only: grep `crates/` for all `compute_lambda(` invocations and verify argument position semantically (not just syntactically).

**Coverage Requirement**: At least one unit test must supply three distinct f64 values and assert the exact weighted sum result; transposing any two arguments must produce a detectably different output.

---

### R-02: Partial `StatusReport` struct field removal in `mcp/response/mod.rs`
**Severity**: High
**Likelihood**: High
**Impact**: `cargo build --workspace` fails. All CI gate runs blocked until every fixture site is corrected. Partial removal is not a test failure — it is a compile error.

**Test Scenarios**:
1. Build gate: `cargo build --workspace` must succeed with zero errors. A compile error at any of the 8 fixture sites (16 field references at lines 614/618, 710/714, 973/977, 1054/1058, 1137/1141, 1212/1216, 1291/1295, 1434/1438) is the detection mechanism.
2. Grep verification (delivery pre-flight): `grep -rn "confidence_freshness_score\|stale_confidence_count" crates/unimatrix-server/src/mcp/` must return zero matches post-delivery (AC-06 verification).
3. `make_coherence_status_report()` helper at line 1434 sets non-default values (0.8200 / 15) — its removal must be verified explicitly, as it differs from the default-value fixtures and may be missed in a search-and-replace pass.

**Coverage Requirement**: Build success is the gate. Grep for removed field names in `mcp/` must return zero matches. Both helper functions (`make_status_report`, `make_coherence_status_report`) must be verified.

---

### R-03: `DEFAULT_STALENESS_THRESHOLD_SECS` incorrectly deleted
**Severity**: High
**Likelihood**: Med
**Impact**: `run_maintenance()` either fails to compile (if it references the removed constant by name) or silently uses a hardcoded literal (if implementer substituted 86400 inline). Background confidence refresh breaks silently in the second case with no test catching it. Evidence: SR-03 (High) in SCOPE-RISK-ASSESSMENT.md; ADR-002 encodes this constraint explicitly.

**Test Scenarios**:
1. AC-11 grep: `grep -n "DEFAULT_STALENESS_THRESHOLD_SECS" crates/unimatrix-server/src/infra/coherence.rs` must return exactly one definition line.
2. `cargo build --workspace` success implies `run_maintenance()` compiles with the constant reference intact.
3. Presence check: constant definition must include the doc comment "Used by run_maintenance() confidence refresh targeting — not a Lambda input."

**Coverage Requirement**: One grep assertion and one build assertion together constitute complete coverage. No dedicated functional test required — the constant's behavior is governed by `run_maintenance()` which is unmodified.

---

### R-04: `lambda_weight_sum_invariant` uses exact `==` comparison
**Severity**: Med
**Likelihood**: Med
**Impact**: ADR-001 states 0.46 + 0.31 + 0.23 = 1.00 exactly in IEEE 754. However, the SCOPE.md and SPECIFICATION.md (NFR-04) mandate epsilon comparison as a robustness guard. A test using `==` may pass on the authoring platform and fail on another, or the reverse — making the invariant unreliable as a safety net. Historical pattern #3829 (weight delta as module-private constant) shows that weight arithmetic deserves explicit guard forms.

**Test Scenarios**:
1. `lambda_weight_sum_invariant` test body must use `(sum - 1.0_f64).abs() < 0.001` (ADR-001) or `< f64::EPSILON` (NFR-04 stricter form). Inspect the test body to confirm no exact `==` comparison.
2. Compute sum as `DEFAULT_WEIGHTS.graph_quality + DEFAULT_WEIGHTS.contradiction_density + DEFAULT_WEIGHTS.embedding_consistency` — not from literals, so a weight value change is automatically caught.

**Coverage Requirement**: Test uses epsilon comparison. Test references the struct constants directly, not inline literals.

---

### R-05: Breaking JSON change surprises downstream operators
**Severity**: Med
**Likelihood**: Low
**Impact**: Operators with custom scripts parsing `context_status` JSON output for `confidence_freshness_score` or `stale_confidence_count` receive empty/error results silently after upgrade. OQ-2 confirmed zero live callers in `product/test/`, but external scripts are not in the test suite. Historical entry #325 (StatusReportJson backward-compat ADR) confirms this is a known risk surface for this struct.

**Test Scenarios**:
1. JSON output test: call `context_status` (or invoke the format_status_report JSON branch on a synthetic report) and assert the returned JSON does not contain `confidence_freshness_score` or `stale_confidence_count` keys.
2. PR description must contain the exact field names as a release-note item (C-07, NFR-06). This is a process check, not a code test.

**Coverage Requirement**: One test asserting key absence in JSON output. PR documentation serves as the operator communication channel.

---

### R-06: `coherence_by_source` per-source `compute_lambda()` call not updated
**Severity**: High
**Likelihood**: Med
**Impact**: Lambda is computed correctly on the main path but incorrectly on the per-source path. The per-source diagnostic breakdown (`coherence_by_source` in `StatusReport`) reports wrong values. The main Lambda passes, tests pass, but per-source breakdown is computed with stale argument mapping. No compile error if the old freshness value is still a local variable.

**Test Scenarios**:
1. Grep for `compute_lambda(` in `services/status.rs` and assert exactly two call sites exist, both with three dimension arguments (not four).
2. Unit or integration test for `coherence_by_source`: supply a synthetic set of entries grouped by trust_source and assert the per-source Lambda values are computed consistently with the main-path Lambda for inputs with the same structural dimensions.
3. AC-13 explicitly requires both call sites updated identically — code review verification is required in addition to tests.

**Coverage Requirement**: Both call sites must be verified by grep count and by a test that exercises the per-source path with known inputs.

---

### R-07: 2-of-3 re-normalization test expected values not updated
**Severity**: Med
**Likelihood**: Med
**Impact**: `lambda_renormalization_without_embedding` and similar tests pass wrong expected values from the 4-dimension era. The re-normalization formula is unchanged, but the weight values differ: 2-of-3 re-norm is now 0.46/(0.46+0.31)=0.5974 for graph and 0.31/0.77=0.4026 for contradiction. Tests with hardcoded expected values from the 4-dimension baseline will silently accept wrong results.

**Test Scenarios**:
1. `lambda_renormalization_without_embedding` (or equivalent): pass `compute_lambda(1.0, None, 1.0, &DEFAULT_WEIGHTS)` and assert result equals 1.0 (AC-08 criterion).
2. `lambda_renormalization_without_embedding` with non-trivial inputs: pass `compute_lambda(0.8, None, 0.6, &DEFAULT_WEIGHTS)` and assert result equals `0.8 * (0.46/0.77) + 0.6 * (0.31/0.77)` within f64 epsilon.
3. Any test referencing a hardcoded expected float for re-normalization must be re-derived against the new weights.

**Coverage Requirement**: AC-08 unit test (None embedding → result 1.0). At least one test with non-trivial inputs that would distinguish correct from incorrect 2-of-3 re-normalization.

---

### R-08: `From<&StatusReport>` impl for `StatusReportJson` retains stale assignments
**Severity**: Med
**Likelihood**: Low
**Impact**: If `StatusReportJson` fields are removed from the struct but the `From<&StatusReport>` impl is not updated, the build fails. However, a subtler failure exists: if the impl assigns to a field that still exists (e.g., reuses a variable name by coincidence) the JSON output is silently incorrect. Historical entry #2398 (API Extension Gap) documents that call-site audits for struct changes must be exhaustive.

**Test Scenarios**:
1. Build success: `cargo build --workspace` fails if the impl references removed fields.
2. JSON format test: verify JSON output does not contain removed fields (R-05 scenario 1 covers this).
3. Code review: confirm the `From<&StatusReport>` impl in `mcp/response/status.rs` contains no reference to `confidence_freshness_score` or `stale_confidence_count`.

**Coverage Requirement**: Build gate plus JSON output key-absence test. No additional test required.

---

### R-09: `generate_recommendations()` retains indirect stale confidence reference
**Severity**: Low
**Likelihood**: Low
**Impact**: If the stale-confidence recommendation branch is deleted but a recommendation string or match arm retains a reference to a removed variable, the build fails — caught at compile time. Risk is low and covered by the build gate.

**Test Scenarios**:
1. Build gate: compile error if any stale reference remains.
2. `recommendations_below_threshold_stale_confidence` test is deleted (FR-15); its absence from the test suite is the positive signal.

**Coverage Requirement**: Build gate is sufficient. No additional test required.

---

### R-10: ADR-003 (entry #179) not superseded before merge
**Severity**: Med
**Likelihood**: Med
**Impact**: Downstream agents (future architect, risk strategist, tester) query Unimatrix for Lambda weight rationale and find the deprecated 4-dimension ADR. They may apply wrong weights (0.35/0.30/0.20/0.15) in a future feature that re-derives or references Lambda. AC-12 requires `context_correct` supersession.

**Test Scenarios**:
1. AC-12 verification: `context_get` on the new ADR entry (post-delivery) returns all four required data points: exact weight literals (0.46, 0.31, 0.23), original ratio (2:1.33:1), rationale (crt-036 invalidation), GH #520 reference.
2. `context_get` on entry #179 returns status "deprecated" with a superseded-by link to the new entry.
3. Process check: delivery agent must execute `context_correct` as a required delivery step (not optional knowledge stewardship).

**Coverage Requirement**: Manual verification via `context_get` on both entries post-delivery. No code test covers this — it is a Unimatrix knowledge state check.

---

## Integration Risks

**`services/status.rs` Phase 5 dual call sites**: The primary integration risk is asymmetric update of the two `compute_lambda()` call sites. The main-path call (line 771) and the `coherence_by_source` loop call (lines 793–804) must both be updated. The compiler will catch any call site that still passes five arguments; the risk is a call site that passes four arguments with freshness substituted by a remaining variable in a wrong position (silently correct arity, wrong semantics). See R-01 and R-06.

**`generate_recommendations()` parameter reduction**: The call site at line 811–818 must drop two arguments. If only one is dropped and a variable happens to have the correct type at that position, the compiler accepts it. The remaining recommendation branches (graph stale ratio, embedding inconsistencies, quarantined) are unchanged — their inputs must not be disturbed by the parameter-list compaction.

**`load_active_entries_with_tags()` retained**: This store read serves only `coherence_by_source` post-crt-048. If it is accidentally removed along with the freshness scan, `coherence_by_source` breaks silently (empty per-source breakdown). FR-11 mandates retention; the `coherence_by_source` test scenario in R-06 detects this.

---

## Edge Cases

**Weight re-normalization with None embedding**: The 2-of-3 re-normalization produces irrational ratios (0.46/0.77 ≈ 0.5974, 0.31/0.77 ≈ 0.4026). Tests that previously used round fractions from the 4-dimension 2-of-3 re-normalization will have incorrect expected values. Every test that asserts a specific float result for `compute_lambda(_, None, _, _)` must be re-derived. See R-07.

**All-zero input**: `compute_lambda(0.0, Some(0.0), 0.0, &DEFAULT_WEIGHTS)` must return 0.0. With freshness gone this is mechanically equivalent to the 4-dimension all-zero case, but the test should use the 3-dimension signature.

**All-one input**: `compute_lambda(1.0, Some(1.0), 1.0, &DEFAULT_WEIGHTS)` must return 1.0. This is AC-07.

**Custom weight struct with zero embedding weight**: `lambda_custom_weights_zero_embedding` constructs a `CoherenceWeights` with `embedding_consistency: 0.0`. After the struct field removal, this test must remove `confidence_freshness` from the struct literal; the test logic is otherwise unchanged.

**`make_coherence_status_report()` non-default fixture values**: This helper at line 1434 sets `confidence_freshness_score: 0.8200` and `stale_confidence_count: 15` — non-zero, non-default values. A search-and-replace targeting `1.0` and `0` will not find it. It must be removed explicitly.

---

## Security Risks

**No new untrusted input surface introduced**: crt-048 is a pure deletion of computation logic and struct fields. `compute_lambda()` accepts only `f64` and `Option<f64>` — no external string parsing, no file paths, no deserialization of external data. The blast radius of a compromised Lambda computation is limited to an incorrect maintenance gate recommendation in `context_status` output.

**JSON output reduction**: Removing fields from JSON output cannot introduce injection or deserialization risk. The breaking change risk is operational (downstream script breakage), not security.

**`DEFAULT_STALENESS_THRESHOLD_SECS` retention**: The constant is a `pub const u64`. It is not user-configurable and not read from any external source. Retention poses no security risk.

---

## Failure Modes

**Build failure (compile error)**: The expected failure mode for any missed struct field removal in `mcp/response/mod.rs`. This is the desired behavior — a hard compile error prevents a partial removal from reaching runtime. Detected immediately by `cargo build --workspace`.

**Silent wrong Lambda value**: The risk if `compute_lambda()` arguments are transposed. Lambda remains in [0.0, 1.0] and passes range checks. Detection requires tests with distinct per-dimension values (R-01 scenario 1).

**Missing `run_maintenance()` constant**: If `DEFAULT_STALENESS_THRESHOLD_SECS` is deleted and `run_maintenance()` uses a hardcoded 86400, background confidence refresh continues to function but the documented constant guard is gone. Future contributors may change the hardcoded value inconsistently. Detection: grep for the constant after delivery.

**Per-source Lambda breakdown silently wrong**: If `coherence_by_source` loop is not updated, per-source Lambda uses 4-dimension computation while the main Lambda uses 3-dimension. The discrepancy is not reported as an error. Detection: R-06 scenario 2.

**Unimatrix knowledge divergence**: If AC-12 is not executed, future agents retrieve ADR-003 (entry #179) with 4-dimension weights. This is a knowledge-base failure mode, not a code failure. Detection: R-10 verification.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (f64 weight sum epsilon) | R-04 | Architecture mandates epsilon comparison in `lambda_weight_sum_invariant`; NFR-04 enforces `< f64::EPSILON`; ADR-001 confirms 1.00 is exactly representable but test uses epsilon guard for robustness |
| SR-02 (positional f64 arg ordering) | R-01, R-06 | Architecture explicitly declined named-struct refactor (low mis-ordering risk for 4 params with distinct types). Risk mitigated by tests with distinct per-dimension values and grep verification of all call sites |
| SR-03 (DEFAULT_STALENESS_THRESHOLD_SECS deleted) | R-03 | ADR-002 encodes the retention constraint; FR-10 / AC-11 in spec make it an explicit AC; comment on constant makes surviving caller visible |
| SR-04 (breaking JSON change, no migration window) | R-05 | OQ-2 resolution confirmed zero live callers in product/test/; NFR-06 and C-07 require PR release-note documentation; JSON key-absence test added |
| SR-05 (coherence_by_source per-source re-normalization) | R-06, R-07 | FR-12 / AC-13 explicitly cover the per-source call site; R-07 covers re-normalization expected value updates; both call sites verified by grep count |
| SR-06 (fixture sites in mod.rs, estimate vs. exact) | R-02 | Architecture enumerated exact 8 fixture sites and 16 field references (table in ARCHITECTURE.md §Component D); non-default-value `make_coherence_status_report()` helper called out as a special case |
| SR-07 (ADR-003 not superseded) | R-10 | AC-12 makes ADR supersession a required delivery step; R-10 coverage requires post-delivery `context_get` verification on both old and new entries |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-03, R-06) | 10 scenarios |
| High | 3 (R-04, R-07, R-10) | 6 scenarios |
| Medium | 3 (R-05, R-07, R-08) | 4 scenarios |
| Low | 1 (R-09) | 1 scenario (build gate) |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned failures gate rejection — entry #2398 (API Extension Gap, call-site audit) directly informed R-02 and R-08 severity ratings; entry #4177 (tautological assertion) informed R-04 test body inspection requirement
- Queried: `/uni-knowledge-search` for risk patterns (coherence, weight, lambda) — entry #3206 (FusionWeights sum-check exemption) confirmed epsilon guard pattern for weight sum invariants; no directly applicable new pattern found
- Queried: `/uni-knowledge-search` for StatusReport JSON serialization — entry #325 (StatusReportJson backward-compat ADR) confirmed this struct's history as a known breaking-change surface, elevating R-05 and R-08
- Stored: nothing novel to store — the call-site audit pattern (#2398) and weight epsilon pattern (#3829) already exist; this feature's risks are feature-specific, not cross-feature patterns
