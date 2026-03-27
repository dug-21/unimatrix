# Agent Report: crt-029-agent-2-testplan

Phase: Test Plan Design (Stage 3a)
Feature: crt-029 — Background Graph Inference (Supports Edges)

---

## Deliverables

| File | Lines | Description |
|------|-------|-------------|
| `product/features/crt-029/test-plan/OVERVIEW.md` | ~140 | Strategy, risk-to-test mapping, integration harness plan |
| `product/features/crt-029/test-plan/inference-config.md` | ~120 | InferenceConfig: defaults, TOML deser, validation boundary tests |
| `product/features/crt-029/test-plan/store-query-helpers.md` | ~120 | query_entries_without_edges (6 tests), query_existing_supports_pairs (6 tests), pool choice grep gate |
| `product/features/crt-029/test-plan/nli-detection-tick.md` | ~170 | select_source_candidates (7), write_inferred_edges_with_cap (7), run_graph_inference_tick guard (1), edge cases (7), R-09 grep gate |
| `product/features/crt-029/test-plan/background-call-site.md` | ~100 | Ordering/gate static checks, pub(crate) promotion grep gate, integration test references |

---

## Risk Coverage Mapping

| Risk | Priority | Coverage Type | Component File |
|------|----------|--------------|----------------|
| R-01 (eliminated by design) | High (residual) | Grep gate: `grep -n 'Contradicts' nli_detection_tick.rs` | nli-detection-tick.md |
| R-02 (cap before embedding) | Critical | Unit: `test_select_source_candidates_cap_enforced` + `test_write_inferred_edges_with_cap_cap_enforced` | nli-detection-tick.md |
| R-03 (threshold boundary) | High | Unit: 4 validation boundary tests | inference-config.md |
| R-04 (pre-filter scan) | Medium | Unit: 6 `query_existing_supports_pairs` tests | store-query-helpers.md |
| R-05 (rayon pool starvation) | Medium | Integration: single-dispatch structure + lifecycle suite | nli-detection-tick.md, OVERVIEW.md |
| R-06 (pool choice ambiguity) | High | Grep gate: `grep -n 'read_pool\|write_pool' read.rs` | store-query-helpers.md |
| R-07 (struct literal trap) | High | Grep gate: `grep -rn 'InferenceConfig {' crates/...` + cargo check | inference-config.md |
| R-08 (cap logic untestable) | High | Unit: `write_inferred_edges_with_cap` standalone tests (no ONNX) | nli-detection-tick.md |
| R-09 (rayon/tokio boundary) | Critical | Grep gate + independent code review (unit tests cannot catch) | nli-detection-tick.md |
| R-10 (W1-2 spawn_blocking) | High | Grep gate: `grep -n 'spawn_blocking' nli_detection_tick.rs` | nli-detection-tick.md |
| R-11 (pub(crate) promotions) | High | Grep gate + compile | background-call-site.md |
| R-12 (priority ordering) | Medium | Unit: `test_select_source_candidates_priority_ordering_combined` | nli-detection-tick.md |
| R-13 (stale pre-filter) | Low | Unit: `test_tick_idempotency` | nli-detection-tick.md |

---

## Integration Suite Plan

**Suites to run in Stage 3c**: `smoke` (mandatory), `lifecycle`, `tools`

**New tests to add to `suites/test_lifecycle.py`** (3 tests):
1. `test_graph_inference_tick_writes_supports_edges` — AC-13, FR-07
2. `test_graph_inference_tick_no_contradicts_edges` — AC-10a, AC-19†, R-01
3. `test_graph_inference_tick_nli_disabled` — AC-14, FR-06

**Suites NOT required**: `protocol`, `security`, `confidence`, `contradiction`, `volume`, `edge_cases`

---

## Key Test Design Decisions

1. **R-09 requires grep gates, not unit tests.** Unit tests run on the Tokio runtime and will not reproduce the rayon worker thread panic. The test plan explicitly names this limitation and specifies both grep commands plus the independent reviewer requirement.

2. **`write_inferred_edges_with_cap` is independently testable.** ADR-002's decision to make this a standalone function with scalar threshold parameters (not `InferenceConfig` dependency) is the direct enabler of R-08 coverage without a live ONNX model.

3. **`select_source_candidates` is pure.** Being a sync function with no external calls, it can be tested exhaustively with constructed slices — all priority ordering and cap enforcement tests are pure logic tests.

4. **Three integration tests planned.** The tick's effects are not directly observable through a single MCP tool call. The lifecycle suite additions observe accumulated edges over tick cycles, which is the correct verification surface for AC-13 and AC-14.

---

## Open Questions for Stage 3b / Stage 3c

1. **`query_existing_supports_pairs` pair normalization**: The test plan specifies that the normalization `(min(a,b), max(a,b))` must be verified. The implementation agent must decide whether normalization happens at SQL query time or in Rust code; the test must verify whichever approach is chosen.

2. **NLI availability in infra-001**: The three new lifecycle integration tests require the ONNX model to be loaded. The Stage 3c tester should verify whether the infra-001 environment has NLI enabled; if not, these tests need `@pytest.mark.skipif(not nli_available(), ...)` rather than failing outright.

3. **`write_nli_edge` INSERT OR IGNORE and `edges_written` counter**: The test `test_write_inferred_edges_insert_or_ignore_idempotency` needs to verify whether the counter increments or not for a duplicate `INSERT OR IGNORE`. The implementation agent must document this behaviour; the test plan assumes `edges_written` does NOT increment for silent duplicates (the cap should count successful inserts only).

4. **`background.rs` unit test infrastructure**: `test_background_tick_nli_disabled_skips_inference_tick` requires mocking all service handles. If the existing test infrastructure does not support this, the AC-14 no-op verification falls entirely on the integration test `test_graph_inference_tick_nli_disabled`. The Stage 3c report must document which method was used.

---

## Self-Check

- [x] OVERVIEW.md maps all 13 risks from RISK-TEST-STRATEGY.md to test scenarios
- [x] OVERVIEW.md includes integration harness section with suite selection and 3 new tests planned
- [x] Per-component files match architecture component boundaries (4 component files + OVERVIEW)
- [x] Every Critical and High risk has at least one specific test expectation with function name
- [x] Integration tests defined for component boundaries (store helpers → tick → background)
- [x] All output files written to `product/features/crt-029/test-plan/`

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 3 entries; entry #2728 (Rayon W1-2 test pattern) and #3655 (tick candidate-bound pattern) both relevant and incorporated. Entry #229 (tester duties) confirmed role scope.
- Queried: `context_search` for crt-029 ADRs — returned #3656–3659 (all four ADRs confirmed and incorporated into component test plans).
- Queried: `context_search` for NLI graph inference testing patterns — returned #3655 (tick two-bound pattern) and #2728 (W1-2 compliance test pattern), both incorporated.
- Queried: `/uni-knowledge-search` for grep gate as compile-invisible test coverage — confirmed no existing pattern covers this specific combination.
- Stored: entry #3660 "Grep Gate as Primary Test Coverage for Compile-Invisible Rayon/Tokio Boundary (R-09 Pattern)" via context_store — novel pattern: grep gates replacing (not supplementing) unit tests for compile-invisible async/concurrency risks, including the independent reviewer requirement.
