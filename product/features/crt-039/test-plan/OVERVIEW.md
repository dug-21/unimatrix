# Test Plan Overview: crt-039 — Tick Decomposition

## What Is Being Tested

crt-039 restructures `run_graph_inference_tick` so Phase 4b (structural Informs HNSW scan)
runs unconditionally while Phase 8 (NLI Supports) remains gated on `get_provider()` success.
The change affects three components: `background.rs` (outer gate removal), `nli_detection_tick.rs`
(two-path control flow, guard simplification, enum variant removal), and `config.rs` (cosine
floor default 0.45 → 0.50).

---

## Test Strategy

| Layer | Scope | Tools |
|-------|-------|-------|
| Unit | `apply_informs_composite_guard`, `phase4b_candidate_passes_guards`, `default_nli_informs_cosine_floor`, floor boundary semantics, enum variant removal | `cargo test --workspace` |
| Integration (in-crate) | `run_graph_inference_tick` end-to-end with real Store, real VectorIndex, `NliServiceHandle` in Loading state | `#[tokio::test]` in `nli_detection_tick.rs` |
| Integration (infra-001) | MCP smoke gate; tick liveness verification via lifecycle suite | `pytest -m smoke` (mandatory gate) |
| Static / grep | Deleted test names gone, dead variants gone, `nli_scores` references gone from guard, log fields present | Pre-merge grep assertions (AC-03, AC-09, AC-17, R-04) |

---

## Risk-to-Test Mapping

| Risk | Priority | Test(s) | Verification Layer |
|------|----------|---------|-------------------|
| R-01: Supports edges written without NLI (silent corruption) | Critical | TC-02 | Integration (in-crate), real Store |
| R-02: Old no-op test removed without replacement | Critical | TR-01 removal + TC-01 + TC-02 | grep absence + integration |
| R-03: Mutual-exclusion gap at cosine 0.50 boundary | Critical | TC-07 + boundary variant | Unit |
| R-04: Dead enum variants retained / partially removed | Critical | `cargo build` + grep | Compile-time + grep |
| R-05: Cosine floor raise eliminates candidate pool | High | AC-11 eval gate; AC-17 log | Eval harness |
| R-06: Stale `nli_scores` arg at call sites | High | `cargo test` warnings-as-errors | Compile-time |
| R-07: Phase 8b Informs loop skipped when no Supports candidates | High | TC-01 (zero-Supports-candidates corpus) | Integration (in-crate) |
| R-08: `format_nli_metadata_informs` dead code / NLI fields in metadata | High | Clippy + TC-01 metadata assertion | Clippy + integration |
| R-09: Contradiction scan behavioral change | Med | Existing contradiction scan tests + diff | Regression + code inspection |
| R-10: Observability log emitted at wrong pipeline point | Med | AC-17 grep + code ordering inspection | grep + code inspection |
| R-11: Tick ordering invariant disturbed | Med | Code inspection + existing tick tests | Code inspection + regression |
| R-12: Cosine floor `>=` inverted to `>` | Low | TC-05 + TC-06 | Unit |

---

## Test Removal Checklist (Pre-Merge, Mandatory)

Three tests MUST be deleted. Gate-3c grep checks verify absence:

| TR | Test Name | Reason |
|----|-----------|--------|
| TR-01 | `test_run_graph_inference_tick_nli_not_ready_no_op` | Asserts old no-op semantics; now invalid. Replaced by TC-01 + TC-02. |
| TR-02 | `test_phase8b_no_informs_when_neutral_exactly_0_5` | Tests neutral guard that is removed. |
| TR-03 | `test_phase8b_writes_informs_when_neutral_just_above_0_5` | Tests neutral guard that is removed. |

Grep command at gate-3c:
```bash
grep -n "test_run_graph_inference_tick_nli_not_ready_no_op\|test_phase8b_no_informs_when_neutral_exactly_0_5\|test_phase8b_writes_informs_when_neutral_just_above_0_5" \
  crates/unimatrix-server/src/services/nli_detection_tick.rs
```
Must return empty.

---

## Test Update Checklist

| Test | Required Change |
|------|----------------|
| `test_inference_config_default_nli_informs_cosine_floor` | Assert `0.5_f32` (was `0.45_f32`) |
| `test_validate_nli_informs_cosine_floor_valid_value_is_ok` | Use `0.5` as nominal valid value |
| `test_phase4b_uses_nli_informs_cosine_floor_not_supports_threshold` | Band changes from `[0.45, 0.50)` to `[0.50, supports_threshold)`; cosine = 0.50 proves inclusive floor |
| All `apply_informs_composite_guard` call sites in tests | Remove `nli_scores` / `NliScores` argument — down to single-argument `(candidate)` |
| `informs_passing_scores()` helper | Remove if no longer used by any test (no remaining call sites after NliScores param removed) |
| Tests calling `format_nli_metadata_informs` | Update to `format_informs_metadata(cosine, src_cat, tgt_cat)` |

---

## Cross-Component Test Dependencies

- `background.rs` changes (outer gate removal) are not directly unit-testable — verified by TC-01
  which exercises the full tick without the outer `if nli_enabled` guard.
- `config.rs` floor default (0.50) flows into `phase4b_candidate_passes_guards` via the
  `InferenceConfig` parameter; TC-06 verifies the correct value is picked up at the boundary.
- `nli_detection_tick.rs` enum variant removal (`NliCandidatePair::Informs`, `PairOrigin::Informs`)
  is a compile-time guarantee — no unit test can directly assert absence; `cargo build` with
  `#![deny(dead_code)]` is the verification mechanism.

---

## Integration Harness Plan (infra-001)

### Suite Selection

crt-039 touches internal tick logic — no new MCP tools, no schema changes, no security
boundaries changed. The harness does not have a "graph inference tick" suite. The relevant
suites are:

| Suite | Reason to Run |
|-------|--------------|
| `smoke` | Mandatory minimum gate — verifies server starts and basic MCP operations work post-refactor |
| `lifecycle` | Verifies multi-step flows (store→search) are unaffected by tick restructuring; tick liveness test in `test_tick_liveness` (availability marker) exercises the tick directly |

Suites NOT required for this feature: `tools`, `security`, `confidence`, `contradiction`,
`volume`, `edge_cases`, `adaptation` — none of these are affected by tick control flow
restructuring.

### Existing Suite Coverage

The `lifecycle` suite exercises `store → search` flows and restart persistence, which
indirectly validates that the tick runs cleanly without panicking. The `smoke` suite
verifies the server is functional end-to-end.

The specific Path A / Path B split and Informs edge accumulation are **not** observable
through the MCP JSON-RPC interface — there is no tool that exposes `GRAPH_EDGES` to clients.
This behavior is validated entirely through in-crate integration tests (TC-01, TC-02) that
use `store.query_graph_edges()` directly.

### New infra-001 Tests: None Required

The integration behavior that matters for this feature (Informs edges written / Supports
edges absent when NLI not ready) is only observable via internal Store API, not via the
MCP interface. The existing in-crate `#[tokio::test]` tests (TC-01, TC-02) are the correct
vehicle. No infra-001 additions are needed.

### Harness Command (Stage 3c)

```bash
# Mandatory smoke gate
cd product/test/infra-001
python -m pytest suites/ -v -m smoke --timeout=60

# Lifecycle suite (verify tick does not corrupt multi-step flows)
python -m pytest suites/test_lifecycle.py -v --timeout=60
```

---

## AC Verification Summary

| AC-ID | Verification Method |
|-------|---------------------|
| AC-01 | Code inspection: `run_single_tick` has no `if nli_enabled` guard around `run_graph_inference_tick` |
| AC-02 | TC-01 (Informs written) + TC-02 (zero Supports) |
| AC-03 | grep: `apply_informs_composite_guard` has no `nli_scores` in body or signature |
| AC-04 | `test_inference_config_default_nli_informs_cosine_floor` asserts `0.5_f32` |
| AC-05 | TC-05 (0.500 included) + TC-06 (0.499 excluded) |
| AC-06 | Existing contradiction scan tests pass; diff shows only comment changes |
| AC-07 | Code inspection: ordering invariant comment present and correct |
| AC-08 | grep: no `0.45` in config.rs test section asserting the floor default |
| AC-09 | Grep absence of TR-01/TR-02/TR-03 test names |
| AC-10 | `cargo test --workspace` green |
| AC-11 | Eval harness MRR >= 0.2913 |
| AC-12 | Code ordering inspection: Phase 2 dedup → Phase 4b → Phase 5 truncate → Phase 8b write |
| AC-13 | TC-07: pair at 0.68 in `candidate_pairs` absent from `informs_metadata` |
| AC-14 | TC-02: zero Supports edges in real Store when NLI not ready |
| AC-15 | TC-01: at least one Informs edge written when NLI not ready |
| AC-16 | TC-01 and TC-02 are two separate tests; TR-01 grep returns empty |
| AC-17 | grep: `informs_candidates_found` present in nli_detection_tick.rs Phase 4b/5 region |
| AC-18 | Clippy clean: no dead-code warning for `format_nli_metadata_informs` (or it is deleted) |
