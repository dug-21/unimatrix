# Gate 3c Report: crt-042

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-04-02
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof (R-01 to R-17) | PASS | All 17 risks have test evidence; R-04/R-07 eval gate deferred by design (flag=false ships) |
| Test coverage completeness | PASS | All 25 ACs verified; 47 new unit tests; zero failures across workspace |
| Specification compliance | PASS | FR-01 through FR-10 and NFR-01 through NFR-09 all implemented |
| Architecture compliance | PASS | Component boundaries, module placement, lock ordering, interface signatures match |
| Integration smoke gate | PASS | 22/22 smoke tests passed |
| xfail markers accounted for | PASS | GH#291, GH#303, GH#305 — all pre-existing, all unrelated to crt-042 |
| AC-25 cross-category behavioral proof | PASS | test_search_phase0_cross_category_entry_visible_with_flag_on present and passing |
| AC-24 tracing instrumentation | PASS | Uses #[traced_test] + logs_contain() — behavioral test, not compile-time check |
| AC-16 edges_of_type boundary | PASS | Zero .edges_directed()/.neighbors_directed() code-line matches in graph_expand.rs |
| AC-00 SR-03 prerequisite gate | PASS | GH#495 filed before Phase 0 code written; confirmed in RISK-COVERAGE-REPORT.md |
| Default flag (ppr_expander_enabled) | PASS | Default is false; eval gate is post-back-fill task, not a shipping gate |
| Code quality | PASS | Builds clean (0 errors); no todo!/unwrap in non-test code; file sizes under 500 lines |
| Knowledge stewardship | PASS | Queried: and Stored: entries present in RISK-COVERAGE-REPORT.md |

---

## Detailed Findings

### Check 1: Risk Mitigation Proof — R-01 through R-17

**Status**: PASS

All 17 risks from the Risk-Based Test Strategy have test evidence recorded in RISK-COVERAGE-REPORT.md.

**Critical risks (R-01, R-02)**:

- R-01 (Flag-off regression): `test_search_flag_off_pool_size_unchanged` (unit); smoke gate 22/22 passed. The Phase 0 guard at search.rs line 888 (`if self.ppr_expander_enabled`) is the first statement inside the `if !use_fallback` block (line 870). Code inspection at search.rs line 882–883 explicitly states "zero overhead — no BFS, no fetch, no Instant::now(), no debug! emission" when the flag is false. The `test_search_phase0_does_not_emit_trace_when_disabled` test using `#[traced_test]` confirms no Phase 0 events fire.

- R-02 (S1/S2 single-direction edges): AC-00 prerequisite gate PASS — RISK-COVERAGE-REPORT.md confirms GH#495 was filed before any Phase 0 implementation. `test_graph_expand_unidirectional_informs_from_higher_id_seed_misses` documents the failure mode; `test_graph_expand_bidirectional_informs_after_backfill` documents correct post-back-fill behavior. The eval gate (AC-23 MRR/P@5) is intentionally deferred until GH#495 is applied — this is a documented shipping decision since the flag defaults to false.

**High risks (R-03, R-04, R-05, R-06, R-07, R-08, R-16)**:

- R-03 (Quarantine bypass): `test_search_phase0_excludes_quarantined_direct`, `test_search_phase0_excludes_quarantined_transitive` (unit); `test_quarantine_excludes_endpoint_from_graph_traversal` (infra-001 lifecycle PASS); security suite 19/19 passed. Quarantine check at search.rs line 927 is correctly ordered after `entry_store.get()` and before `results_with_scores.push()`.
- R-04 (O(N) latency): Timing instrumentation confirmed present (AC-24 PASS). Eval gate P95 measurement deferred by design — flag=false ships.
- R-05 (Combined ceiling): `test_search_phase0_phase5_combined_ceiling` PASS; inline comment in search.rs documents 270-entry ceiling.
- R-06 (Back-fill race): Procedural gate confirmed in RISK-COVERAGE-REPORT.md; eval snapshot must follow GH#495.
- R-07 (Eval gate failure): AC-25 behavioral proof present and passing (see AC-25 section below). Eval gate deferred.
- R-08 (InferenceConfig hidden test sites): Grep scan confirms all `InferenceConfig {` literal sites use `..InferenceConfig::default()` spread syntax or include all three new fields. 7 test files verified.
- R-16 (Phase 0 insertion point): Phase 0 block confirmed at search.rs line 872 — first block inside `if !use_fallback` (line 870). Phase 1 `seed_scores` construction at line 969 is after Phase 0.

**Medium risks (R-09 through R-14, R-17)**:

- R-09: AC-16 grep PASS — zero `.edges_directed()`/`.neighbors_directed()` code-line matches.
- R-10: `test_search_phase0_emits_debug_trace_when_enabled` and `test_search_phase0_does_not_emit_trace_when_disabled` both use `#[traced_test]`. `tracing::debug!` at search.rs line 951 confirmed (not `info!`).
- R-11: `test_graph_expand_bidirectional_terminates`, `test_graph_expand_triangular_cycle_terminates` — both terminate; visited set confirmed.
- R-12: `test_graph_expand_seeds_excluded_from_result`, `test_graph_expand_self_loop_seed_not_returned`.
- R-13: `test_graph_expand_deterministic_across_calls` — budget-boundary exercised at max=3; lowest IDs 2,3,4 returned deterministically.
- R-14: Four config validation tests (AC-18/19/20/21) all with `ppr_expander_enabled=false` — unconditional validation confirmed.
- R-17: `test_graph_expand_s8_coaccess_unidirectional_from_higher_id_misses` — gap documented; crt-035 tick provides coverage for promoted pairs.

**Low risk (R-15)**:

- `test_search_phase0_skips_entry_with_no_embedding` — None embedding → silent skip confirmed.

**Evidence**: RISK-COVERAGE-REPORT.md table accounts for all 17 risks with named test cases and result statuses. Unit test suite: 4130 total, 4099 passed, ~28 ignored (pre-existing), 0 failed.

---

### Check 2: Test Coverage Completeness

**Status**: PASS

All 25 acceptance criteria (AC-00 through AC-25) are verified in RISK-COVERAGE-REPORT.md's AC verification table. The RISK-TEST-STRATEGY.md's non-negotiable mandatory tests are all present:

| Mandatory Test | Status |
|---|---|
| AC-01 flag-off regression (existing suite unchanged) | PASS — 22/22 smoke, targeted search tests |
| AC-14 quarantine bypass (explicit fixture required) | PASS — unit tests in search.rs mod phase0 |
| AC-24 timing instrumentation (tracing subscriber required) | PASS — #[traced_test] + logs_contain() |
| AC-25 cross-category behavioral regression (core feature proof) | PASS — orthogonal embedding construction |
| AC-18/19/20/21 config validation (unconditional, 4 tests) | PASS — all 4 present with flag=false |

Integration test counts (from RISK-COVERAGE-REPORT.md):
- Smoke: 22/22 passed
- Security: 19/19 passed
- Lifecycle (targeted 9): 6 passed, 2 xfailed (GH#291, pre-existing), 1 xpassed
- Tools (targeted 6): 5 passed, 1 xfailed (pre-existing)

The test plan contingency for AC-14 and AC-25 is correctly applied: MCP harness does not support per-test config override, so both are unit tests in `search.rs` mod phase0. This is the intended resolution documented in the test-plan OVERVIEW.

---

### Check 3: Specification Compliance

**Status**: PASS

All functional requirements verified:

- FR-01 (graph_expand function): Implemented in `unimatrix-engine/src/graph_expand.rs`; signature matches specification exactly; re-exported from `graph.rs`.
- FR-02 (BFS traversal contract): Four positive edge types traversed; behavioral contract (forward from seeds) confirmed by AC-03/AC-04 unit tests.
- FR-03 (degenerate cases): AC-10/AC-11/AC-12 unit tests all pass.
- FR-04 (edges_of_type boundary): AC-16 grep confirms zero violations.
- FR-05 (Phase 0 integration in search.rs): Phase 0 block at line 872 confirmed with all 5 sub-steps (seed collection, graph_expand call, dedup, quarantine check, embedding lookup, cosine similarity, push).
- FR-06 (quarantine caller responsibility): Module-level doc comment in graph_expand.rs lines 12–18 documents the obligation explicitly.
- FR-07 (InferenceConfig additions): Three fields present with correct serde defaults, types, and ranges.
- FR-08 (unconditional validation): Validation at config.rs lines 1307–1325 confirmed unconditional (no `if self.ppr_expander_enabled` guard).
- FR-09 (SearchService struct and wiring): Three fields at search.rs lines 384–388; wired at lines 541–543.
- FR-10 (eval profile): `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` confirmed present with correct content matching the specification.

All non-functional requirements (NFR-01 through NFR-09) verified:
- NFR-01 (latency instrumentation): `tracing::debug!` at search.rs line 951 with all 6 mandatory fields.
- NFR-02 (flag-off bit-identical): Confirmed via smoke gate and `test_search_flag_off_pool_size_unchanged`.
- NFR-03 (quarantine safety): Confirmed via R-03 tests; silent skip (no warn/error).
- NFR-04 (determinism): `neighbors.sort_unstable()` at graph_expand.rs line 139; `sorted_expanded.sort_unstable()` in Phase 0.
- NFR-05 (synchronous and pure): graph_expand.rs has no async, no I/O.
- NFR-06 (lock order): `typed_graph` is pre-cloned before Step 6d begins; no lock held during Phase 0.
- NFR-07 (no per-query store reads in graph_expand): graph_expand.rs has no store access.
- NFR-08 (270-entry ceiling): Documented in graph_expand.rs lines 27–32 and search.rs lines 874–876.
- NFR-09 (file size): graph_expand.rs is 178 lines; graph_expand_tests.rs is 415 lines — both under 500.

---

### Check 4: Architecture Compliance

**Status**: PASS

All architectural decisions followed:

- **ADR-001** (graph_expand.rs as #[path] submodule): Confirmed — `#[path = "graph_expand.rs"] mod graph_expand;` pattern established; tests split to graph_expand_tests.rs via `#[path = "graph_expand_tests.rs"] mod tests;`.
- **ADR-002** (Phase 0 insertion before Phase 1): Phase 0 at line 872, Phase 1 at line 969 in search.rs.
- **ADR-003** (true cosine similarity): `cosine_similarity(&embedding, &emb)` at search.rs line 942 — not a floor constant.
- **ADR-004** (unconditional config validation): Confirmed at config.rs lines 1307–1325.
- **ADR-005** (debug! timing instrumentation): `tracing::debug!` at search.rs line 951; not `info!`.
- **ADR-006** (direction behavioral spec): All ACs expressed behaviorally; `Direction::Outgoing` used in implementation but behavioral contracts in doc comments cite entry #3754.

Component boundaries match the architecture decomposition:
- Component 1 (`graph_expand`): pure BFS in engine crate, no I/O.
- Component 2 (Phase 0 in search.rs): async orchestration, calls Component 1.
- Component 3 (InferenceConfig): three fields in config.rs with unconditional validation.
- Component 4 (eval profile): TOML file confirmed at correct path.

Interface signatures match the Integration Surface table in ARCHITECTURE.md exactly.

The S1/S2 directionality pre-implementation gate (SR-03) was observed: GH#495 filed before Phase 0 code written, per ARCHITECTURE.md's "Blocking gate" requirement.

---

### Check 5: Integration Smoke Gate (Mandatory)

**Status**: PASS

RISK-COVERAGE-REPORT.md documents: `pytest -m smoke` → 22 passed, 237 deselected, in 191.45s.

The smoke gate is the R-01 regression confirmation. Zero failures.

---

### Check 6: xfail Markers

**Status**: PASS

All xfail markers are pre-existing and confirmed unrelated to crt-042:

| Marker | GH Issue | Root Cause | Relates to crt-042? |
|--------|----------|------------|---------------------|
| test_inferred_edge_count_unchanged_by_s1_s2_s8 | GH#291 | Background tick interval exceeds test timeout | No |
| test_s1_edges_visible_in_status_after_tick | GH#291 | Same | No |
| import::tests pool timeout | GH#303 | Concurrent test pool issue in Rust | No |
| test_retrospective_baseline_present | GH#305 | Null baseline_comparison with synthetic features | No |
| test_deprecated_visible_in_search_with_lower_confidence | GH#405 | Background scoring timing | No |

No new xfail markers were added by crt-042. No tests were deleted or commented out. This was confirmed in RISK-COVERAGE-REPORT.md ("No new xfail markers were added. No tests were deleted or commented out.").

Note: One xpass occurred (`test_inferred_edge_count_unchanged_by_s1_s2_s8`) — GH#291 marker cleared incidentally. This is benign.

---

### Check 7: AC-25 Cross-Category Behavioral Proof

**Status**: PASS

`test_search_phase0_cross_category_entry_visible_with_flag_on` in `search.rs` mod phase0 constructs:
- Query embedding Q = [1.0, 0.0] (unit x-axis)
- Seed S (id=1): embedding [1.0, 0.0], cosine_sim(Q, S) = 1.0 — HNSW seed
- Entry E (id=2): embedding [0.0, 1.0], cosine_sim(Q, E) ≈ 0.0 — orthogonal, would not appear via HNSW
- Graph: S → E (Supports edge)

With `ppr_expander_enabled=true`: `added=1`, E present in pool, cosine_sim(Q,E) ≈ 0.0 confirmed.
With `ppr_expander_enabled=false`: `added=0`, E absent from pool.

This is the mandatory core behavioral proof (entry #3579 / #2758 pattern). Present and passing.

---

### Check 8: AC-24 Tracing Instrumentation

**Status**: PASS

`test_search_phase0_emits_debug_trace_when_enabled` uses `#[traced_test]` (from `tracing-test = "0.2"` in Cargo.toml) and `logs_contain()` to verify all 6 required fields:
- "Phase 0 (graph_expand) complete" (message)
- "expanded_count"
- "elapsed_ms"
- "seeds"
- "fetched_count"
- "expansion_depth"
- "max_expansion_candidates"

This is a behavioral test using a real tracing subscriber, not a compile-time check. This directly addresses the trap documented in entry #3935 (tracing test deferral at Gate 3b).

The trace is emitted at `tracing::debug!` level (search.rs line 951) — confirmed not `info!` or `warn!`.

---

### Check 9: AC-16 — edges_of_type Boundary

**Status**: PASS

Grep result from RISK-COVERAGE-REPORT.md (reproduced):
```
grep -n "edges_directed\|neighbors_directed" graph_expand.rs | grep -v "//"
```
Zero code-line matches. The pattern appears only in doc comment lines 9, 57, 58, 114.

Actual traversal at graph_expand.rs lines 121, 124, 127, 130–131 uses `graph.edges_of_type()` for each of the four positive edge types (CoAccess, Supports, Informs, Prerequisite). SR-01 invariant (entry #3627) is preserved.

---

### Check 10: Code Quality

**Status**: PASS

- **Build**: Clean build — `Finished dev profile` with 0 errors. 17 warnings in unimatrix-server (pre-existing, not introduced by crt-042).
- **No todo!/unimplemented!**: Grep confirms zero occurrences in graph_expand.rs.
- **No .unwrap() in non-test code**: All `.unwrap()` calls in search.rs (lines 3455, 4822, 4827, 4864, 5301, 5605) are within test modules (#[cfg(test)] blocks). graph_expand.rs has zero `.unwrap()`.
- **File sizes**: graph_expand.rs (178 lines), graph_expand_tests.rs (415 lines) — both under 500. search.rs is 5642 lines but this is the pre-existing file; crt-042 added Phase 0 block (~90 lines) and Phase 0 tests (~570 lines) within the existing mod.
- **`cargo audit`**: Not installed in this environment — unable to verify. This is a pre-existing environment limitation, not introduced by crt-042.

---

### Check 11: Security

**Status**: PASS

- No hardcoded secrets.
- Input validation: `graph_expand` receives `seed_ids` derived from HNSW (entry IDs from database, not user-supplied values). No injection risk.
- Quarantine enforcement: confirmed at single mandatory enforcement point in search.rs Phase 0 (R-03 coverage).
- FR-06 caller obligation documented in graph_expand.rs module doc comment (future caller security contract).
- Security suite: 19/19 tests passed (includes quarantine and capability enforcement tests).

---

### Check 12: Knowledge Stewardship

**Status**: PASS

RISK-COVERAGE-REPORT.md (the tester agent report) contains a `## Knowledge Stewardship` section with:
- `Queried:` entry — `mcp__unimatrix__context_briefing` returning entries #3806, #3935, #2758, #2577, #3579.
- `Stored:` entry — "nothing novel to store" with a documented reason (the contingency pattern is a specific instance of an existing pattern, not a new reusable technique).

---

## Deferred Items (Not Gate Blockers)

Two items are intentionally deferred per the spawn prompt and ARCHITECTURE.md design decisions:

1. **AC-23 / R-07 eval gate** (MRR >= 0.2856, P@5 measurement): Requires GH#495 (S1/S2 back-fill) to be applied to the deployment database first. `ppr_expander_enabled` defaults to `false`, so the eval gate is a pre-condition for default enablement, not a shipping gate.

2. **R-04 P95 latency gate** (delta <= 50ms over baseline): Requires eval harness run with RUST_LOG=..search=debug. Also dependent on GH#495 for meaningful measurement. Deferred to same eval run.

Both deferrals are explicitly designed into the feature (ARCHITECTURE.md Latency Profile section, SPECIFICATION.md NFR-01, RISK-TEST-STRATEGY.md R-04/R-07 coverage notes). The feature ships with `ppr_expander_enabled = false`; enabling by default requires a separate decision after these gates are satisfied.

---

## Rework Required

None.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` for validation context — confirmed entries #3935 (tracing test deferral trap), #2758 (grep non-negotiable tests), #3579 (zero test modules Gate 3b) informed the mandatory check set.
- Stored: nothing novel to store — the deferred eval gate pattern (flag-off ship with post-back-fill eval requirement) is feature-specific to crt-042. No recurring cross-feature lesson emerges from this gate.
