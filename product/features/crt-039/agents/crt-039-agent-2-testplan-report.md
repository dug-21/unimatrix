# Agent Report: crt-039-agent-2-testplan

Phase: Test Plan Design (Stage 3a)
Component coverage: background.rs, nli_detection_tick.rs, infra/config.rs

---

## Deliverables

| File | Status |
|------|--------|
| `product/features/crt-039/test-plan/OVERVIEW.md` | Created |
| `product/features/crt-039/test-plan/background.md` | Created |
| `product/features/crt-039/test-plan/nli_detection_tick.md` | Created |
| `product/features/crt-039/test-plan/config.md` | Created |

---

## Risk Coverage Mapping

| Risk | Priority | Test Coverage |
|------|----------|--------------|
| R-01: Supports edges written without NLI | Critical | TC-02 (integration, real Store, zero Supports assertion) |
| R-02: Test coverage gap at Path A/B boundary | Critical | TR-01 removal grep + TC-01 + TC-02 (separate tests) |
| R-03: Mutual-exclusion gap at cosine 0.50 | Critical | TC-07 (explicit subtraction unit test + boundary variant) |
| R-04: Dead enum variants retained | Critical | `cargo build` + grep absence check (compile-time) |
| R-05: Floor raise eliminates candidate pool | High | AC-11 eval gate; AC-17 observability log from tick 1 |
| R-06: Stale NliScores arg at call sites | High | Compile-time (`cargo test` with warnings-as-errors) + grep |
| R-07: Phase 8b skipped when no Supports candidates | High | TC-01 (zero-Supports-candidates corpus — explicit setup requirement) |
| R-08: format_nli_metadata_informs dead code | High | Clippy + metadata field assertion in TC-01 or standalone unit |
| R-09: Contradiction scan behavioral change | Med | Existing tests (regression) + diff audit |
| R-10: Observability log at wrong pipeline point | Med | AC-17 grep check + code ordering inspection |
| R-11: Tick ordering invariant disturbed | Med | Code inspection + existing tick regression tests |
| R-12: Cosine floor `>=` inverted | Low | TC-05 (0.500 included) + TC-06 (0.499 excluded) |

All 12 risks from RISK-TEST-STRATEGY.md have mapped coverage.

---

## Integration Suite Plan

- `smoke`: Mandatory gate. Verifies server starts and basic MCP operations work post-refactor.
- `lifecycle`: Verifies multi-step flows unaffected by tick restructuring.
- No new infra-001 tests required: the observable behavior (Informs/Supports edge presence)
  is accessible only via internal Store API, not MCP JSON-RPC. In-crate `#[tokio::test]`
  integration tests (TC-01, TC-02) are the correct vehicle.

---

## Key Design Decisions Made

1. **TC-01 corpus setup is critical for R-07**: TC-01 must use a corpus where no pair exceeds
   `supports_candidate_threshold`. This ensures `candidate_pairs` is empty, which exercises
   the specific risk that Phase 8b is nested inside the Path B early-return block. If the
   test corpus has Supports candidates, R-07 would not be caught.

2. **TC-07 approach**: The test requires access to Phase 4b's internal subtraction logic.
   If the subtraction is implemented as inline code inside `run_graph_inference_tick`, the
   test will need to either: (a) use a full tick integration setup with a controlled vector
   corpus, or (b) the implementor must expose a small testable helper. The test plan describes
   both the assertion and the data structure pattern — Stage 3b implementor should expose
   the subtraction logic as a testable unit.

3. **`informs_passing_scores()` helper removal**: After the `NliScores` parameter is removed
   from `apply_informs_composite_guard`, this helper has no remaining call sites and must be
   deleted. The compiler will warn, but this must not be suppressed — it must be deleted.

4. **AC-17 log ordering constraint**: `informs_candidates_found` must be assigned before
   the `existing_informs_pairs` dedup filter runs, not after. This is a code ordering
   requirement that cannot be validated by a unit test — it requires code inspection at
   gate-3b. Added explicit note in the test plan.

---

## Open Questions

None blocking. One implementor note:

- TC-07 testability depends on whether Phase 4b's Supports-set subtraction is exposed as
  a standalone function or embedded inline. If inline, TC-07 becomes a full integration
  test requiring a controlled vector corpus. The test plan documents both approaches.
  Stage 3b implementor should note this and expose the subtraction as a unit-testable step
  if possible.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found ADRs #4017/#4018/#4019 (directly
  relevant), #3937 (NLI neutral-zone pattern being removed), #3949 (per-guard negative test
  pattern confirming TC-03/TC-04 approach), #3713 (threshold default lesson confirming R-05
  severity). Entries #3949 and #3713 were directly applied.
- Queried: `context_search` for crt-039 decisions — returned all 3 ADRs (confirmed).
- Queried: `context_search` for tick inference graph edge testing patterns — found #3822,
  #3656, #3675, #3913, #3981 (all background tick patterns, applied to integration test
  fixture guidance).
- Stored: nothing novel to store — all patterns applied here (guard simplification test
  structure, tick integration test setup with real Store, compile-time dead-code verification)
  are applications of existing patterns #3949 and #3713. No cross-feature novelty confirmed.
