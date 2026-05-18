# Risk-Based Test Strategy: vnc-016

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Integration test passes vacuously because `feature_entries` row for entry A is absent — SQL JOIN returns empty, rule never fires, assertion succeeds on empty hotspots list with a wrong `not in` check | High | High | Critical |
| R-02 | Integration test passes vacuously because `force=False` (default) hits stale memoized result, bypassing the detection pipeline entirely | High | Med | Critical |
| R-03 | Rust unit test (positive path) passes but does not detect a regression — assertion is structurally always-true (e.g., `!result.is_empty()` absent; result is compared to length only) | High | Med | Critical |
| R-04 | Rust negative-path companion test is absent or trivial — a broken JOIN that ignores feature_cycle scoping passes the positive test; without the negative companion, a "return all stale edges regardless of cycle" regression is undetectable | High | Med | Critical |
| R-05 | `unwrap_or_else` error-swallowing in `tools.rs:2169-2177` re-conceals a future SQL regression (column rename, schema drift) after vnc-016 ships — the Rust unit test is the sole regression guard | High | Med | High |
| R-06 | Trust level for `context_store` call for entry A is Restricted (unenrolled agent) — `UsageService.record_access` silently skips `feature_recording`, leaving `feature_entries` empty and producing a false negative indistinguishable from the pre-fix state | High | Med | High |
| R-07 | Observation cycle_id mismatch — `_seed_observation_sql` called with a different cycle_id than used in `context_store` / `context_cycle_review`, causing the "empty feature cycle" early-exit path rather than a detection report | Med | Med | High |
| R-08 | Negative-path test uses an incorrect absence assertion (e.g., asserts `hotspots == []` rather than rule-name absence) — misses an always-fires bug that returns other rules but not `dependency_on_deprecated` | Med | Low | Med |
| R-09 | `feature_cycle` key forwarded as explicit JSON `null` (Python `None` not guarded) rather than absent from `args` dict — serde deserialization contract differs from missing-key path if `StoreParams.feature_cycle` ever gains `#[serde(default)]` | Low | Low | Low |
| R-10 | Existing tests regress due to `client.py` signature change — `context_store()` call sites break if the new keyword arg shadows a positional arg or changes the `args` dict structure for callers that do not pass `feature_cycle` | Med | Low | Med |

---

## Risk-to-Scenario Mapping

### R-01: Integration test passes vacuously — `feature_entries` row absent for entry A

**Severity**: High
**Likelihood**: High
**Impact**: The positive integration test gives a green result while the production bug remains unfixed. No CI signal; the defect ships. This is the primary failure mode the entire feature exists to catch.

**Root cause chain** (from Phase 2a discovery): `UsageService.record_access` skips `feature_recording` when trust level is below Privileged. An unenrolled `agent_id` resolves to Restricted. Missing `feature_cycle` at `context_store` time cannot be back-filled.

**Test Scenarios**:
1. Positive test (`test_dependency_on_deprecated_e2e`) must call `context_store` for entry A with both `feature_cycle=cycle_id` AND `agent_id="human"`. Run the test; it must FAIL before the `read.rs` SQL fix and PASS after.
2. Deliberately omit `feature_cycle` from entry A's `context_store` call and verify the test fails (not vacuously passes) — this confirms the assertion is load-bearing.
3. Deliberately use a non-"human" agent_id (e.g., unenrolled string) and verify `feature_entries` is empty and the test fails — confirms the trust-level guard is exercised.

**Coverage Requirement**: The test must fail against the un-fixed `read.rs` (with `fe.feature_cycle`). If it passes before the SQL fix is applied, the test is vacuous. CI must gate on the test failing against the pre-fix commit or the implementer must manually verify the fail-first behavior.

---

### R-02: Integration test passes vacuously — memoized result returned

**Severity**: High
**Likelihood**: Med
**Impact**: A cached result from a prior test run (possibly from a different cycle's data) is returned. Detection pipeline never executes. Green test, unverified wiring.

**Test Scenarios**:
1. Both `test_dependency_on_deprecated_e2e` and `test_dependency_on_deprecated_no_finding_without_stale_edge` must include `force=True` on every `context_cycle_review` call. Code inspection is the verification method (AC-07).
2. Verify unique `cycle_id` per test function — a reused cycle_id across test invocations means prior state may be cached. Pattern: `f"vnc016-{uuid.uuid4().hex[:8]}"`.

**Coverage Requirement**: Both test functions must contain `force=True` as a hard literal. A comment in the test body stating "force=True bypasses memoization — omitting this causes the test to pass vacuously on a cached result" should appear.

---

### R-03: Rust unit test positive path — assertion is structurally always-true

**Severity**: High
**Likelihood**: Med
**Impact**: The Rust unit test gives a false green against the unfixed SQL (with `fe.feature_cycle`). The column-name bug survives into delivery. Historical evidence: Unimatrix #4177 (tautological assertion, bugfix-505) — an assertion that exists but cannot structurally fail is equivalent to no assertion.

**Test Scenarios**:
1. `test_query_stale_prerequisite_edges_for_cycle_returns_pair` must assert: (a) `result` is `Ok`, (b) `result.unwrap().len() == 1`, (c) `result.unwrap()[0] == (A_id, B_id)`. All three sub-assertions must be present.
2. Verify the test fails against the unfixed SQL. Before applying the SQL fix, run `cargo test -p unimatrix-store test_query_stale_prerequisite_edges_for_cycle_returns_pair` — it must fail with a SQLite "no such column: fe.feature_cycle" error (propagated via `Result::Err`, not swallowed at the store layer).

**Coverage Requirement**: The assertion must not rely on `unwrap_or_default()` or `unwrap_or_else(|_| vec![])` — that would replicate the production bug inside the test. The function returns `Result<Vec<(u64, u64)>>`: the test must first assert `is_ok()` before unwrapping, to surface SQL errors rather than masking them.

---

### R-04: Rust negative-path companion absent — broken feature-cycle scoping undetectable

**Severity**: High
**Likelihood**: Med
**Impact**: An implementation of `query_stale_prerequisite_edges_for_cycle` that returns ALL stale Prerequisite edges regardless of cycle (e.g., JOIN without WHERE on `feature_id`) passes the positive test. Only the negative companion (`empty_without_feature_entry`) can detect this. Confirmed design requirement from Phase 2a: the negative test verifies the JOIN scoping, not the Prerequisite/Deprecated filters.

**Test Scenarios**:
1. `test_query_stale_prerequisite_edges_for_cycle_empty_without_feature_entry` must: seed entries A (Deprecated) and B (Active), insert the Prerequisite edge A→B into `graph_edges`, but NOT insert any row into `feature_entries` for any cycle. Call `query_stale_prerequisite_edges_for_cycle(<test_cycle>)`. Assert `result.unwrap().is_empty()`.
2. The negative companion must use a distinct cycle ID from the positive test to avoid accidental `feature_entries` row cross-contamination.

**Coverage Requirement**: The negative companion test must be present in the same `mod tests` block as the positive test. It is not optional — it is the only mechanism that validates the `JOIN feature_entries ... WHERE fe.feature_id = ?1` scoping clause.

---

### R-05: Future SQL regression re-concealed by `unwrap_or_else` in `tools.rs`

**Severity**: High
**Likelihood**: Med
**Impact**: A future schema migration renames `feature_entries.feature_id` (or any column referenced in the query). The handler silently returns `vec![]` with a `tracing::warn!`, the detection rule never fires, and the only observable symptom is a missing `dependency_on_deprecated` finding — the same silent failure mode vnc-016 is fixing. Historical evidence: Unimatrix #4445 (this exact pattern, stored from vnc-016 scope analysis).

**Test Scenarios**:
1. The Rust unit test at the store layer is the sole regression guard — it calls the function directly, bypasses `unwrap_or_else`, and surfaces SQL errors as test failures. The integration test is slower and harder to diagnose; both layers are required (ARCHITECTURE.md §Known Architectural Constraint).
2. The `unwrap_or_else` in `tools.rs:2169-2177` is out of scope for vnc-016 but should be flagged in a follow-up issue for hardening — logging at ERROR rather than WARN when `stale_edge_pairs` query fails.

**Coverage Requirement**: The Rust unit test must NOT use `unwrap_or_else(|_| vec![])` or `unwrap_or_default()` on the function result — it must surface errors. The integration test alone is insufficient; both test layers must pass.

---

### R-06: `context_store` called with unenrolled agent_id — trust gate silently skips `feature_entries`

**Severity**: High
**Likelihood**: Med
**Impact**: `UsageService.record_access` skips the `feature_recording` branch when `trust_level` is Restricted. `feature_entries` remains empty. The SQL JOIN returns nothing. The detection rule never fires. The test passes as a false negative. This is the most subtle failure mode: no error, no warn, the MCP call succeeds, `feature_cycle` was provided — but the analytics write path was silently gated. Source: ADR-007 (Unimatrix #103) — Restricted agents' `feature` parameter is silently ignored.

**Test Scenarios**:
1. Both integration tests must call `context_store` with `agent_id="human"` (Privileged) — this is a hard constraint, not a style choice. Using any unenrolled agent string causes Restricted resolution and empty `feature_entries`.
2. The `context_cycle_review` call must also use `agent_id="human"` per the 7-step scenario (ARCHITECTURE.md §Positive Test).

**Coverage Requirement**: Agent ID must be explicitly set to `"human"` in step 1 of the positive test. Code inspection is required — the default agent_id behavior of the harness client must not be assumed to be Privileged.

---

### R-07: Observation cycle_id mismatch — "empty feature cycle" early-exit triggered

**Severity**: Med
**Likelihood**: Med
**Impact**: `context_cycle_review` takes the early-exit path ("no observations for this cycle") rather than running the detection pipeline. The response is not a `RetrospectiveReport`; JSON parsing for `hotspots` fails or returns an unexpected structure. Test may fail at the JSON parse step rather than at the assertion, masking the root cause.

**Test Scenarios**:
1. Both tests must bind `cycle_id` as a single variable at the top of the function and reuse it for all setup calls: `context_store(feature_cycle=cycle_id)`, `_seed_observation_sql(db_path, [cycle_id])`, and `context_cycle_review(cycle_id, ...)`.
2. Both tests must call `_seed_observation_sql` with at least `num_records=20` (the default). Calling with `num_records=0` triggers the empty-cycle path.

**Coverage Requirement**: A single `cycle_id = f"vnc016-{uuid.uuid4().hex[:8]}"` binding must be visible at the top of each test function. No inline string literals for the cycle ID anywhere else in the same test body.

---

### R-08: Negative-path assertion is too broad — misses always-fires regression

**Severity**: Med
**Likelihood**: Low
**Impact**: If the negative test asserts `hotspots == []` (total absence of any hotspot) rather than `"dependency_on_deprecated" not in rule_names`, it will fail when other detection rules fire legitimately — causing false test failures — while also failing to catch an "always fires regardless of data" implementation of `DependencyOnDeprecatedRule`.

**Test Scenarios**:
1. The negative-path test assertion must be: `assert not any(h["rule_name"] == "dependency_on_deprecated" for h in data["hotspots"])`. Not `assert data["hotspots"] == []`.
2. The negative-path test must construct a scenario with no stale Prerequisite edges — two stored entries with no edge between them, neither deprecated. This is distinct from "no observations" (which triggers early-exit, not a detection run).

**Coverage Requirement**: The negative-path test must produce a valid `RetrospectiveReport` (observations seeded, force=True, JSON response with `hotspots` key) and then verify absence of the specific rule name.

---

## Integration Risks

**`feature_entries` write is async (tokio::spawn)**: The analytics write path launches `record_feature_entries` via `tokio::spawn`. The test issues sequential MCP calls with no explicit wait. If the spawn has not completed before `context_cycle_review` is called, `feature_entries` will be empty. ARCHITECTURE.md §Analytics Write Path notes this completes before the review call in sequential MCP call patterns — but test implementers must not insert artificial concurrency (e.g., `asyncio.gather`) between steps 1 and 6.

**`context_correct` successor entry in `feature_entries`**: `context_correct` creates a successor entry. That successor does NOT need to be in `feature_entries`. The SQL query joins on source_id (entry A), not the successor. Confusion here could lead to an extra unnecessary `feature_cycle` parameter on the `context_correct` call — which is harmless but indicates a misunderstanding of the query semantics.

**Shared live-server fixture scope**: The infra-001 server fixture is process-scoped. `feature_entries` rows persist across tests within the same pytest session. Unique cycle IDs (pattern: `uuid.uuid4().hex[:8]`) are the sole isolation mechanism. A non-unique cycle ID causes interference: another test's entries appear under the cycle, possibly producing stale edges that weren't seeded by the current test.

**`client.py` backward compatibility**: All existing `context_store` call sites must continue to work without modification. If the new `feature_cycle` parameter is inserted before an existing positional argument (rather than after `edges`), existing callers break silently if Python resolves positional args differently.

---

## Edge Cases

**Empty `hotspots` array vs. missing `hotspots` key**: If `context_cycle_review` returns JSON without a `hotspots` key (e.g., it returns a plain text acknowledgment on the empty-cycle path), `data["hotspots"]` raises `KeyError`. The test must handle this by first asserting `"hotspots" in data` before iterating — a `KeyError` here means the cycle had no observations, not that the rule didn't fire.

**`feature_id` string case sensitivity in SQLite**: SQLite TEXT columns are case-sensitive for `=` comparisons. `cycle_id` must be passed with the exact same string in `context_store(feature_cycle=cycle_id)` and `context_cycle_review(cycle_id, ...)`. Any normalization (e.g., `.lower()`, `.strip()`) that is not symmetric across both calls will cause a JOIN miss.

**Prerequisite edge relation_type casing**: The SQL query filters on `ge.relation_type = 'Prerequisite'` (exact string). `context_edge("add", ...)` with relation type `"Prerequisite"` must match this literal exactly. A different casing (e.g., `"prerequisite"`) would cause the query to return nothing even after the SQL fix.

**`status = 1` meaning**: The SQL query filters `e.status = 1` for Deprecated. `context_correct` is the correct deprecation mechanism — it must set `A.status = 1`. If `context_correct` is called incorrectly (e.g., wrong entry ID, wrong content format) and does not actually update status, the query returns nothing and the test becomes a false negative.

**`phase` column in `feature_entries`**: The `INSERT OR IGNORE INTO feature_entries (feature_id, entry_id, phase)` write path includes `phase`. The Rust unit test seeds `feature_entries` with `phase = NULL`. If a future schema change makes `phase NOT NULL`, the unit test seed SQL will fail. Current schema allows NULL; note for maintainers.

---

## Security Risks

**No new untrusted input surface**: vnc-016 introduces no new MCP tool parameters, no new external input paths, and no schema changes. The `feature_cycle` parameter on `context_store` already exists in `StoreParams`; only the Python harness client gains a new keyword argument.

**Trust-level enforcement on `feature_entries` write (ADR-007, Unimatrix #103)**: The existing trust-level gate (Restricted agents cannot write to `feature_entries`) is the relevant security surface. vnc-016 relies on this gate functioning correctly — the test uses `agent_id="human"` to be Privileged. A regression in the trust-level resolution (e.g., `"human"` is demoted in the registry) would cause `feature_entries` to be unpopulated silently, making the test a false negative without any security violation. Not a blast-radius concern for vnc-016, but confirms the test's dependency on the registry bootstrap state.

**SQL injection via `feature_cycle` string**: The SQL query uses a parameterized `?1` placeholder — not string interpolation. `cycle_id = f"vnc016-{uuid.uuid4().hex[:8]}"` produces alphanumeric input only. No injection risk.

---

## Failure Modes

**If the SQL fix is applied but the Rust unit test is omitted**: Future regressions (column rename, schema drift) will again be silently swallowed by `unwrap_or_else`. The detection rule will silently stop firing, no test will catch it, and the issue will recur in a future feature. Both test layers (Rust unit + Python integration) are required for ongoing protection.

**If the integration test positive path passes vacuously (R-01, R-02, R-06)**: The wiring defect is declared fixed, passes CI, and ships. The `dependency_on_deprecated` rule is silently non-functional in production. The only observable symptom is a missing finding in `context_cycle_review` responses — indistinguishable from "no stale edges exist."

**If the negative-path test is absent (R-04, R-08)**: An always-fires implementation of the detection rule (or a broken JOIN that returns all edges regardless of cycle) passes the positive test. Any entry added to the system with a Prerequisite edge would be flagged as a `dependency_on_deprecated` finding regardless of its actual status, polluting all cycle reviews.

**If `context_cycle_review` returns the empty-cycle acknowledgment path**: The JSON response does not contain a `hotspots` key. The test raises `KeyError` or catches an unexpected structure. This indicates `_seed_observation_sql` was not called with the matching cycle_id or was called with `num_records=0`. Failure is loud (exception), not silent — but the root cause is test setup, not the SQL fix.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (`unwrap_or_else` swallows SQL errors silently) | R-05 | Rust unit test (AC-09) is the regression guard; it calls the function directly and surfaces errors through `Result`. Integration test provides end-to-end coverage but not error-path visibility. `unwrap_or_else` pattern unchanged by vnc-016 — follow-up issue recommended. |
| SR-02 (`feature_entries` populated only at `context_store` write time) | R-01, R-06 | Spec C-01 makes `feature_cycle` on entry A's `context_store` call a hard constraint. R-06 adds the trust-level dimension (Phase 2a discovery): `agent_id="human"` is required. Both risks materialize the same symptom (empty `feature_entries`) via different paths. |
| SR-03 (memoization — `force=False` hits stale cache) | R-02 | AC-07 makes `force=True` a hard constraint in both test bodies. Code inspection is the verification method. |
| SR-04 (negative-path test cycle_id mismatch) | R-07, R-08 | Spec C-03 and C-05 require a single `cycle_id` binding per test function. R-08 adds the assertion-structure risk: the assertion must target `rule_name`, not total hotspot absence. |
| SR-05 (`feature_cycle` kwarg forwarded as explicit `null`) | R-09 | Architecture confirms `if feature_cycle is not None: args["feature_cycle"] = feature_cycle` guard — key is absent (not null) when not provided. Low priority; serde contract is clear. |
| SR-06 (shared live-server fixture, cycle ID uniqueness) | Integration Risks §Shared live-server fixture | Addressed by uuid-suffix pattern. Not elevated to a numbered risk; it is a constraint, not a failure mode unique to this feature. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-03, R-04) | Fail-first verification for positive integration test; `force=True` hard constraint; Rust assertion sub-structure (ok + len + value); negative companion present and scoped to cycle JOIN |
| High | 3 (R-05, R-06, R-07) | Store-layer unit test error propagation; `agent_id="human"` hard constraint; single `cycle_id` binding per test |
| Medium | 2 (R-08, R-10) | Negative assertion targets rule_name not total absence; `client.py` backward-compat verification on existing call sites |
| Low | 1 (R-09) | Serde missing-key vs null contract — verified by architecture analysis; no additional test scenario needed |
