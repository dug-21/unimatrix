# Risk-Based Test Strategy: bugfix-523

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Path A or Path C is accidentally gated by the Item 1 insertion — edges stop accumulating silently | High | Low | Critical |
| R-02 | NLI gate inserted at wrong structural boundary (before `run_cosine_supports_path` rather than after) — ADR-001 violation | High | Low | Critical |
| R-03 | All 19 NaN fields not individually tested — a missed field passes NaN silently into scoring | High | Med | Critical |
| R-04 | `sanitize_session_id` guard inserted after `event.session_id` is first used — injection window remains open | High | Low | Critical |
| R-05 | Wrong `warn!` site downgraded in Item 2 — the non-finite cosine warn changed instead of (or in addition to) the category_map miss warns | Med | Med | High |
| R-06 | Entire test module omitted by implementation wave — production code ships, tests absent at Gate 3b | Med | Med | High |
| R-07 | NaN tests use wrong field name string in `assert_validate_fails_with_field` — test passes vacuously because error message does not match | Med | Med | High |
| R-08 | AC-29 regression — valid `post_tool_use_rework_candidate` events rejected after guard is added | Med | Low | High |
| R-09 | AC-03 regression — NLI-enabled path broken by Item 1 change | Med | Low | Med |
| R-10 | AC-27 regression — existing boundary-value tests for the 19 fields break after `!v.is_finite()` prefix is added | Med | Low | Med |
| R-11 | AC-04/AC-05 log-level gap unacknowledged at Gate 3b — reviewer rejects behavioral-only coverage without gate documentation | Low | Med | Med |
| R-12 | Item 3 NaN propagation through cross-field invariant checks (field individually passes, cross-field check silently passes NaN) | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Path A or Path C accidentally gated
**Severity**: High
**Likelihood**: Low
**Impact**: Informs edges and cosine Supports edges stop being written on every tick when `nli_enabled=false` — the production default. Graph knowledge accumulation halts silently. No runtime error surfaces; the failure mode is data absence, not a crash.

**Test Scenarios**:
1. `test_nli_gate_path_a_informs_edges_still_written_nli_disabled`: run `run_graph_inference_tick` with `nli_enabled=false` and candidate pairs that would produce Informs edges in Path A. Assert that Informs edges are present in the output graph store after the tick completes.
2. `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled`: run `run_graph_inference_tick` with `nli_enabled=false` and candidate pairs that pass the cosine threshold. Assert that cosine Supports edges are written by `run_cosine_supports_path`.

**Coverage Requirement**: Both tests are non-negotiable. Passing AC-01 (Path B skipped) is insufficient without passing AC-02 (Path A and Path C continue). The gate check is only correct if both sides of the predicate are verified.

---

### R-02: Gate at wrong structural position — ADR-001 violation
**Severity**: High
**Likelihood**: Low
**Impact**: If the gate is placed before `run_cosine_supports_path` (e.g., at the function entry or after Phase 4b), Path C is gated along with Path B. Cosine Supports edges stop accumulating in production. This violates crt-039 ADR-001 (entry #4017) — a structural invariant, not a style preference.

**Test Scenarios**:
1. R-02 is covered by the same tests as R-01 (scenario 2 above). Path C producing edges with `nli_enabled=false` proves the gate is placed after `run_cosine_supports_path`, not before.

**Coverage Requirement**: The tester must verify the insertion position in the source against the structural landmark `// === PATH B entry gate ===` as part of code review (not just test pass). The test is the runtime confirmation; the landmark is the structural confirmation.

---

### R-03: 19-field NaN coverage — omission of any individual field
**Severity**: High
**Likelihood**: Med
**Impact**: A field without an `!v.is_finite()` guard silently accepts NaN at server startup. NaN propagates into the scoring pipeline (search scoring, graph weight writes, rayon batch inputs) until server restart. For fusion weight fields, the sum check also passes NaN silently (IEEE 754: `NaN > 1.0` is false). The error produced would be `FusionWeightSumExceeded` (misleading) or no error at all.

**Test Scenarios** (all 19 are non-negotiable — not a sample):
1. AC-06 through AC-24: one test per field, each setting exactly one field to `f32::NAN` or `f64::NAN` and calling `assert_validate_fails_with_field(&c, "<field_name>")`.
2. AC-25: `nli_entailment_threshold = f32::INFINITY` — representative f32 Inf test.
3. AC-26: `ppr_alpha = f64::INFINITY` — representative f64 Inf test.

**Coverage Requirement**: All 19 NaN tests must be present and individually named. The tester must verify the count (19) matches the field checklist in SPECIFICATION.md, not infer it from passing tests. A count mismatch at Gate 3a is the most likely source of a gap (lesson #1203 — gate validators must check all files in one pass).

---

### R-04: Guard inserted after `event.session_id` first use
**Severity**: High
**Likelihood**: Low
**Impact**: If the guard is placed after `event.payload.get("tool_name")` or after any other statement that touches `event.session_id`, the sanitization window is open for that code to execute with an unsanitized value. Even if neither `tool_name` extraction nor any other line before the misplaced guard uses `event.session_id` directly, the structural contract (guard is the first `session_id` consumer) is violated and creates a maintenance trap for future edits.

**Test Scenarios**:
1. AC-28: dispatch `post_tool_use_rework_candidate` with `event.session_id = "../../etc/passwd"`. Assert `HookResponse::Error { code: ERR_INVALID_PAYLOAD }` is returned and `record_rework_event` is never called.
2. Tester must confirm via code inspection that the guard block precedes `event.payload.get("tool_name")` at line ~666 — i.e., no use of `event.session_id` appears between the capability check and the guard.

**Coverage Requirement**: AC-28 is the runtime guard. Code review confirming insertion order is the structural guard. Both are required (SR-05).

---

### R-05: Wrong `warn!` site downgraded in Item 2
**Severity**: Med
**Likelihood**: Med
**Impact**: `run_cosine_supports_path` contains three `warn!`/`debug!` sites: two category_map miss sites (should become `debug!`) and one non-finite cosine site (must remain `warn!`). If the implementor changes the wrong site — downgrading the non-finite cosine warn instead of (or in addition to) the category_map warns — a structural anomaly (NaN from HNSW) silently becomes a debug-level log. Operators lose the signal that HNSW vector data may be corrupted.

**Test Scenarios**:
1. AC-04: call `run_cosine_supports_path` with a candidate pair where `src_id` is absent from `category_map`. Assert the pair is skipped and function returns without panic. This tests the behavioral invariant at the category_map miss site.
2. AC-05: call `run_cosine_supports_path` with a candidate pair that has a non-finite cosine value. Assert the pair is skipped. This confirms the non-finite cosine path is still handled — the tester must code-review that the site is still `warn!` since log level is not asserted in tests (ADR-001(c) decision).

**Coverage Requirement**: Behavioral tests for AC-04 and AC-05. Code review is the only mechanism for verifying the non-finite cosine site remains `warn!`. Per ADR-001(c), this is explicitly acknowledged. The gate report must document: "log level for non-finite cosine site verified by code review, not test assertion."

---

### R-06: Entire test module absent at Gate 3b
**Severity**: Med
**Likelihood**: Med
**Impact**: Lessons #4076 (nan-009, crt-042) and #3935 (crt-036) document the recurring pattern: implementation agent delivers production code correctly but writes zero tests. Gate 3b fails; a rework wave writes tests from scratch. For this batch, the 19 NaN tests and the dispatch-arm test are the highest-risk omissions.

**Test Scenarios**:
1. Gate 3a must verify presence of all test functions by name before marking delivery complete: `test_nan_guard_nli_entailment_threshold` through `test_nan_guard_w_phase_explicit` (19 tests), `test_nli_gate_path_a_informs_edges_still_written_nli_disabled`, `test_nli_gate_path_c_cosine_supports_edges_still_written_nli_disabled`, `test_cosine_supports_path_skips_missing_category_map_src`, `test_cosine_supports_path_skips_missing_category_map_tgt`, `test_dispatch_rework_candidate_invalid_session_id_rejected`, `test_dispatch_rework_candidate_valid_path_not_regressed`.
2. Tester must count the NaN tests present and confirm count = 19 before marking AC-06 through AC-24 complete.

**Coverage Requirement**: Gate 3a checklist-style verification. Count-by-presence, not count-by-pass. A test that fails to compile does not satisfy this requirement.

---

### R-07: NaN test passes vacuously — wrong field name string
**Severity**: Med
**Likelihood**: Med
**Impact**: `assert_validate_fails_with_field(c, "field_name")` checks `err.to_string().contains(field_name)`. If the test uses a wrong or abbreviated field name (e.g., `"w_sim_weight"` instead of `"w_sim"`), the assertion passes as long as the error string happens to contain the substring — or fails for the wrong reason. For loop-based fusion/phase weight fields, the field name in the error comes from the `&'static str` in the check array; a mismatch between the test string and the array entry causes the test to pass vacuously even if the guard is missing.

**Test Scenarios**:
1. Each of the 19 NaN tests must use the exact field name string from the SPECIFICATION.md field checklist. The tester must spot-check a sample of the fusion weight tests (e.g., `w_sim`, `w_coac`, `w_prov`) to confirm the error string matches by running with a deliberately wrong field name and verifying the assertion fails.

**Coverage Requirement**: Spot-check verification of field name strings for the loop-based group fields (AC-17 through AC-24). Document in gate report.

---

### R-08: AC-29 regression — valid rework-candidate events rejected
**Severity**: Med
**Likelihood**: Low
**Impact**: If the `sanitize_session_id` call incorrectly validates a valid session_id format, or if the guard's return position blocks the rest of the arm for well-formed events, rework-candidate signals from all PostToolUse hooks stop reaching the session registry. Rework detection silently fails.

**Test Scenarios**:
1. AC-29: dispatch `post_tool_use_rework_candidate` with a valid session_id (e.g., `"session-abc123"`). Assert `record_rework_event` is called and the function returns `HookResponse::Ok` or equivalent success variant.

**Coverage Requirement**: AC-29 is a non-negotiable regression guard. It must be present alongside AC-28.

---

### R-09: AC-03 regression — NLI-enabled path broken
**Severity**: Med
**Likelihood**: Low
**Impact**: The `if !config.nli_enabled { return; }` guard must be an early-return only when `nli_enabled=false`. If the condition is inverted, or if the guard's placement somehow affects the provider call, Phases 6/7/8 stop executing when NLI is enabled. NLI Supports edges stop accumulating in NLI-enabled deployments.

**Test Scenarios**:
1. AC-03: run `run_graph_inference_tick` with `nli_enabled=true` and a mock provider available. Assert that `get_provider()` is called and rayon dispatch executes (or that NLI Supports edges are written, as a behavioral proxy).

**Coverage Requirement**: AC-03 is a correctness regression test for the enabled path. The existing `test_path_c_runs_unconditionally_nli_disabled` tests are insufficient — they cover `nli_enabled=false`. A corresponding `nli_enabled=true` test is required.

---

### R-10: AC-27 regression — existing boundary-value tests break
**Severity**: Med
**Likelihood**: Low
**Impact**: Adding `!v.is_finite() || ` prefix to 19 guards is mechanical but could shift the loop-body guard from a dereference `*value` to a plain `value` (or vice versa) for the fusion/phase weight loop fields. A type mismatch or incorrect dereference would cause a compile error or logic error that breaks existing boundary tests.

**Test Scenarios**:
1. AC-27: run the full `InferenceConfig::validate()` test suite (all pre-existing tests) after applying Item 3 changes. Assert all pass with no new failures.
2. Specifically verify boundary-value tests for `w_sim` (valid: 0.0, 0.5; invalid: -0.1, 1.1) continue to pass — these are the most likely to break if the loop-body dereference is changed incorrectly.

**Coverage Requirement**: `cargo test` clean run with no pre-existing test regressions. Gate 3a must confirm this explicitly.

---

### R-11: AC-04/AC-05 behavioral-only coverage unacknowledged at gate
**Severity**: Low
**Likelihood**: Med
**Impact**: Gate 3b reviewer may reject the test delivery if AC-04 is described as "PASS" without noting that log level is verified by code review only (not by test assertion). Lesson #3935 shows this creates a Gate 3b WARN or FAIL that requires a rework wave. The risk is process friction, not functional incorrectness.

**Test Scenarios**:
1. Gate report must include explicit statement: "AC-04 and AC-05 log-level assertions are behavioral-only per ADR-001(c) (entry #4143). Log level verified by code review. No `tracing-test` harness used."
2. AC-04 behavioral test: `run_cosine_supports_path` with absent `src_id` — pair is skipped, no panic, function returns.
3. AC-05 behavioral test: `run_cosine_supports_path` with non-finite cosine — pair is skipped, no panic, function returns.

**Coverage Requirement**: Gate report must document the deliberate choice. Reviewers must be able to reference ADR-001(c) (Unimatrix entry #4143) as the authority for behavioral-only coverage.

---

### R-12: Cross-field invariant NaN pass-through
**Severity**: Low
**Likelihood**: Low
**Impact**: Cross-field checks (e.g., `nli_auto_quarantine_threshold <= nli_contradiction_threshold`) compare two fields that could both be NaN. A NaN comparison returns false, so the invariant check would not fire. However, the per-field `!v.is_finite()` guard from Item 3 catches each field individually before the cross-field check is reached. This risk is mitigated by the fix itself.

**Test Scenarios**:
1. AC-08 (NaN on `nli_auto_quarantine_threshold`) and AC-07 (NaN on `nli_contradiction_threshold`) each trigger per-field errors before the cross-field check runs. No additional cross-field NaN test is required.

**Coverage Requirement**: AC-07 and AC-08 together are sufficient. No additional test needed — document in gate report that cross-field NaN is caught upstream.

---

## Integration Risks

**Items 1 and 2 share `nli_detection_tick.rs`**: Both edits land in the same file. A merge conflict between two implementation agents would corrupt both changes. SR-06 mitigation — both items must be assigned to the same agent/wave. The tester should verify that the final diff for `nli_detection_tick.rs` contains both the gate insertion (Item 1) and the two warn→debug changes (Item 2) and no extraneous changes.

**Item 4 and `background.rs` unconditional call**: The gate in Item 1 is inside `run_graph_inference_tick`. The caller in `background.rs` must remain unconditional. If the implementor accidentally also adds a guard in `background.rs`, Phase A and Path C would be skipped in production. The tester must verify `background.rs` is unchanged in the diff.

**Item 3 loop guards vs. inline guards**: The 19 fields split across two implementation patterns (inline `let v = self.<field>` for Group A; loop-body `!value.is_finite() || *value` for Groups B and C). A test that passes for Group A does not verify Group B or C. The three groups must be tested independently.

---

## Edge Cases

**Item 1**: `candidate_pairs.is_empty()` fast-exit and `nli_enabled=false` gate are both early returns in the same region. The tester must verify the gate works when `candidate_pairs` is non-empty (the default path) — not just when `candidate_pairs` is empty (the trivial early-exit that already existed).

**Item 2**: A candidate pair where both `src_id` AND `tgt_id` are absent from `category_map`. The function should skip the pair after the first miss (on `src_id`); the `tgt_id` path is not reached. Test both the src-absent and tgt-absent cases independently to confirm both branch arms are covered.

**Item 3**: `f32::NEG_INFINITY` and `f64::NEG_INFINITY` are also non-finite. The two representative Inf tests (AC-25, AC-26) use positive infinity. The `!v.is_finite()` check catches both signs — no additional negative-infinity tests are required, but the tester should note this in the gate report.

**Item 4**: Session_id at exactly the maximum length (128 characters of valid chars) must pass the guard. Session_id at 129 characters must fail. These boundary cases already exist in the `sanitize_session_id` unit tests; the dispatch-arm test need not repeat them — it only needs to verify the guard is called (R-04 / AC-28).

---

## Security Risks

**Item 4 — session_id injection in `post_tool_use_rework_candidate`**: This arm accepts untrusted input from UDS hook clients. `event.session_id` is used as a key in `session_registry.record_rework_event` and `record_topic_signal`. Without sanitization, a crafted session_id (e.g., path traversal, SQL-like injection, Unicode control characters) could corrupt registry state or cause unexpected behavior in downstream consumers. The guard allowlist (`[a-zA-Z0-9\-_]+`, max 128 chars) is the established contract. Blast radius without the guard: any hook client with `SessionWrite` capability can inject an arbitrary string as a session key. With the guard: rejected at the gate with `ERR_INVALID_PAYLOAD`; no registry call made.

**Item 3 — NaN in config as a denial-of-service vector**: A maliciously crafted config file supplying `NaN` to a fusion weight field currently causes the server to start successfully but compute undefined scoring results on every query until restart. Post-fix, the server fails fast at startup — which is strictly better, but means a misconfigured config can prevent the server from starting. The blast radius shrinks from "silent corruption of all search results" to "server refuses to start with a diagnostic error."

**Items 1 and 2 — no untrusted input**: Both are internal background tick changes. No external input is consumed. No security risk beyond correctness.

---

## Failure Modes

**Item 1 — gate fires but Path C did not run**: If the gate is placed before `run_cosine_supports_path`, the function returns silently. No error, no log at the normal severity. Operators would observe diminishing graph edge counts over time. Detection: monitoring of edge write rates in production, or behavioral test AC-02 catching it in CI.

**Item 2 — wrong site changed**: Non-finite cosine from HNSW becomes a silent debug log. Operators lose the signal of HNSW vector corruption. Detection: code review only (tests do not assert log level per ADR-001(c)).

**Item 3 — NaN guard missing on one field**: Server starts successfully, NaN propagates into that field's scoring path. Detection: test AC-06 through AC-24 in CI. If a test is missing (R-03/R-06), detection falls to production observation of scoring anomalies.

**Item 4 — guard present but wrong error code returned**: If `ERR_INVALID_PAYLOAD` is not used (e.g., a different code constant is substituted), hook clients receive an unexpected error code and may not handle it correctly. Detection: AC-28 asserts the specific code value.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 — Gate at wrong boundary violates ADR-001 | R-01, R-02 | Architecture specifies structural landmark (`// === PATH B entry gate ===`). R-01/R-02 tests verify Path A and Path C run unconditionally when `nli_enabled=false`. |
| SR-02 — 19-field edit in ~8000-line file, omission risk | R-03, R-06, R-07 | Specification enumerates all 19 fields with types and guard forms. R-03 requires all 19 tested individually. R-06 requires presence-count verification at Gate 3a. R-07 requires field-name string spot-check. |
| SR-03 — Tracing-level ACs historically block Gate 3b | R-11 | ADR-001(c) resolves: behavioral-only coverage. R-11 requires gate report to document this explicitly, citing entry #4143. Reviewer must accept behavioral coverage per ADR. |
| SR-04 — NaN tests ship without helper pattern, silent pass | R-07 | R-07 maps directly. Spot-check of field name strings in loop-group tests (AC-17 through AC-24) is the mitigation. |
| SR-05 — Guard inserted after session_id first use | R-04 | R-04 requires code inspection of insertion order in addition to AC-28 runtime test. Both are required. |
| SR-06 — Items 1 and 2 in same file, merge conflict risk | Integration Risks section | Architectural constraint: same agent/wave. Tester verifies diff for `nli_detection_tick.rs` contains both changes and `background.rs` is unchanged. |

---

## Coverage Summary

| Priority | Risk Count | Required Test Functions |
|----------|-----------|------------------------|
| Critical | 4 (R-01, R-02, R-03, R-04) | AC-01/AC-02 (Path A+C unconditional), AC-06 through AC-24 (all 19 NaN), AC-25/AC-26 (Inf), AC-28 |
| High | 4 (R-05, R-06, R-07, R-08) | AC-04/AC-05 behavioral, Gate 3a presence count, field-name spot-check, AC-29 |
| Med | 3 (R-09, R-10, R-11) | AC-03 (NLI-enabled regression), AC-27 (boundary-value regression), gate report log-level acknowledgment |
| Low | 1 (R-12) | Covered by AC-07 + AC-08; no additional test |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for `"lesson-learned failures gate rejection"` — found #3935 (tracing-test deferral pattern, crt-036), #4076 (zero-test-module Gate 3b failure, nan-009/crt-042), #2758 (Gate 3c non-negotiable test name verification), #1203 (gate validators must check all files in one pass). All four directly inform R-06, R-11, and the Coverage Summary.
- Queried: `/uni-knowledge-search` for `"risk pattern test coverage omission NaN guard"` — found #4133 (NaN guard pattern for InferenceConfig, active), #3949 (per-guard negative test for composite guards). Both confirm the established pattern and inform R-03.
- Queried: `/uni-knowledge-search` for `"sanitize session_id UDS dispatch guard injection"` — found #3902 (UDS dispatch session audit lesson), #4141 and #3921 (sanitize_session_id consistency rule). Informs R-04 and the Security Risks section.
- Stored: nothing novel to store — risks are feature-specific. The tracing-test behavioral-only pattern is already captured in #3935. The NaN guard pattern is already in #4133. No new cross-feature pattern visible from this batch alone.
