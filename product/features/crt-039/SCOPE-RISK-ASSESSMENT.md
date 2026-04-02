# Scope Risk Assessment: crt-039

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Phase 4b now writes Informs edges unconditionally on every tick. Previously it never ran. No burst-control or per-tick volume analysis exists for this new write path beyond `MAX_INFORMS_PER_TICK` (25). Graph edge table may accumulate at rates not anticipated by Group 3/4 design. | High | High | Architect must confirm that 25 edges/tick is a hard cap (not a soft warn) and that the dedup pre-filter (`query_existing_informs_pairs`) applies before the cap, not after. Trace the exact cap enforcement point in Phase 5. |
| SR-02 | Cosine floor raised 0.45 → 0.5. No empirical data on how many current entry pairs fall between 0.45–0.5 is cited in SCOPE.md. If the floor eliminates the majority of candidates in early graph-building, Group 3 (which depends on crt-039 producing edges) starts from a sparser graph than anticipated. | Med | Med | Spec writer should require a pre-condition check: scan HNSW index against current corpus and record candidate counts at both floors. Gate the default on that data, or document acceptable sparsity for Group 3. |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-03 | Candidate set separation is the sole mutual-exclusion guarantee after guards 4 and 5 are removed. SCOPE.md §"Change 2" asserts this is "handled by candidate set separation between Phase 4 and Phase 4b" without specifying the mechanism. If Phase 4 and Phase 4b share HNSW results via overlapping cosine ranges (Supports threshold vs. Informs floor), a pair could qualify for both. | High | Med | Spec writer must define the exact exclusion invariant: does Phase 4b explicitly subtract the Phase 4 candidate set, or do the thresholds produce disjoint sets by construction? This must appear as a named acceptance criterion, not an implied property. |
| SR-04 | Phase 1 guard split: `get_provider()` early-return must be moved to gate only Phase 8, not Phase 4b. This is the highest-risk structural change — if the split is incomplete, Phase 8 can execute without valid NLI scores (silent Supports edge corruption). SCOPE.md identifies this risk but defers the mechanism to implementation. | High | Med | Spec must explicitly define the control flow: after the split, Phase 8 entry requires a successful `get_provider()` call; Phase 4b entry requires none. Acceptance criteria must include a test asserting Phase 8 does NOT write edges when `get_provider()` returns Err. |
| SR-05 | `test_run_graph_inference_tick_nli_not_ready_no_op` semantics change: the test currently asserts zero edges on NLI-not-ready. After crt-039, Phase 4b still runs and may write Informs edges when NLI not ready. If this test is updated to pass vacuously (e.g., assertion removed), CI passes but the no-regression guarantee is lost. Referenced in lesson #3624 (eval gate validates no-op path only — suppression features need mandatory positive integration tests). | High | High | Spec must name this test explicitly in AC and require it be rephrased to assert: (a) Informs edges CAN be written when NLI not ready, (b) Supports edges are NOT written when NLI not ready. Two assertions, not one. |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-06 | Tick completion observability: Phase 4b introduces a new write path with no existing log coverage for edge counts per tick. Lesson #3723 documents that tick log absence makes threshold tuning a blind guess. With the cosine floor change and new write path active simultaneously, there is no signal to distinguish "floor too high — no candidates" from "dedup filtering all candidates" from "cap applying correctly." | Med | High | Spec writer should require a structured log line at Phase 4b completion: candidates found, dedup-filtered count, cap-applied count, edges written count. This is an observability AC, not a test AC. |
| SR-07 | Contradiction scan separation is comment/structure only (behavior unchanged per SCOPE.md §Goal 3). If the separation introduces any condition reordering or bracket change in `background.rs`, the ordering invariant (compaction → promotion → rebuild → structural_graph_tick → contradiction_scan) could be silently broken. The SCOPE.md notes this as a non-functional change but provides no verification mechanism. | Med | Low | Architect should treat the contradiction scan block as a zero-diff behavioral region: the only permitted changes are comment additions and whitespace. Spec AC must assert tick ordering via integration test or explicit invariant comment audit. |

## Assumptions

- **SCOPE.md §"Change 2" (Option B)**: Assumes candidate set separation between Phase 4 and Phase 4b produces disjoint pairs by construction. This assumption is unverified in the scope document — it is asserted, not proven. If the HNSW cosine ranges overlap, guards 4 and 5 removal is unsafe.
- **SCOPE.md §"Change 3" (cosine floor)**: Assumes raising to 0.5 "provides an equivalent structural filter." No corpus measurement is cited. Equivalence is asserted qualitatively.
- **SCOPE.md §"Non-Goals"**: Assumes `nli_detection_tick.rs` module rename deferral is safe because Phase 8 still uses NLI. This is sound for crt-039 but creates a misleading module name that any reviewer of Phase 4b code will encounter during Group 3 work.

## Design Recommendations

- **SR-01, SR-03**: The architect must define the dedup + cap enforcement sequence for Phase 4b explicitly — dedup pre-filter applied before cap, cap is a hard write limit. Document as an ordering invariant alongside the existing tick ordering comment.
- **SR-04, SR-05**: Spec must define Phase 1 split behavior as a named acceptance criterion with a concrete test for each branch (Phase 4b runs when NLI not ready; Phase 8 does not). The existing `no_op` test must become two targeted tests, not a rephrased single assertion.
- **SR-02, SR-06**: Spec should require a corpus baseline measurement (candidate counts at 0.45 vs 0.5) before accepting the floor change. Pair this with the observability log line (SR-06) so production deployment produces measurable data from tick 1.
