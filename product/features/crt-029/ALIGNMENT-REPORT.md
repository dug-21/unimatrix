# Alignment Report: crt-029

> Reviewed: 2026-03-27
> Artifacts reviewed:
>   - product/features/crt-029/architecture/ARCHITECTURE.md
>   - product/features/crt-029/specification/SPECIFICATION.md
>   - product/features/crt-029/RISK-TEST-STRATEGY.md
> Vision source: product/PRODUCT-VISION.md
> Scope source: product/features/crt-029/SCOPE.md
> Scope risk source: product/features/crt-029/SCOPE-RISK-ASSESSMENT.md

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Vision Alignment | PASS | Feature directly advances the W1-4 / Wave 1A intelligence foundation by densifying the typed relationship graph |
| Milestone Fit | PASS | crt-029 sits squarely in Wave 1A / W1-4 territory; no future-wave scope imported |
| Scope Gaps | PASS | All SCOPE.md goals and acceptance criteria are addressed in full |
| Scope Additions | WARN | Architecture introduces `Store::query_existing_supports_pairs()` as a fourth store helper not named in SCOPE.md; also makes three private helpers `pub(crate)` — both are necessary implementation details, not functional additions |
| Architecture Consistency | PASS | Architecture is internally consistent; all risk mitigations from SCOPE-RISK-ASSESSMENT.md are mapped to concrete ADRs and components |
| Risk Completeness | WARN | One risk (R-06 — conflicting ADRs #3593 vs #3595 on `compute_graph_cohesion_metrics` pool choice) is explicitly marked UNRESOLVED and requires human action before the implementation brief is written |

**Overall: PASS with two WARNs. No VARIANCE or FAIL. No items require blocking approval. One item (R-06 ADR conflict) requires a human decision before delivery starts.**

---

## Scope Alignment

| Type | Item | Details |
|------|------|---------|
| Addition | `Store::query_existing_supports_pairs()` | Architecture (ADR-004) introduces this as a dedicated store helper for the pre-filter HashSet. SCOPE.md mentions "pre-filter query" but names only `query_entries_without_edges()` as a new store helper. `query_existing_supports_pairs()` is a reasonable decomposition — the alternative (filtering `query_graph_edges()` in Rust) is also acceptable per ARCHITECTURE.md §Open Questions item 2. No functional addition; pure implementation decomposition. |
| Addition | `pub(crate)` promotions for `write_nli_edge`, `format_nli_metadata`, `current_timestamp_secs` | Required to make the new `nli_detection_tick.rs` module compile. Not mentioned in SCOPE.md. Risk R-11 in RISK-TEST-STRATEGY.md covers this. No functional change to the existing helpers. |
| Simplification | `Prerequisite` edges with `bootstrap_only = true` not in scope | SCOPE.md Non-Goals section explicitly defers `Prerequisite` edge promotion (§Non-Goals, final bullet). Architecture, spec, and risk docs all confirm Prerequisite is out of scope. Consistent. |
| Simplification | Tick-modulo interval gate not added | SCOPE.md Design Decision #1 resolved this to "every tick, `max_graph_inference_per_tick` is the throttle". All source docs honour this decision. |

---

## Variances Requiring Approval

None. Both WARNs are informational; neither blocks delivery.

---

## Detailed Findings

### Vision Alignment

**PASS.**

The product vision states (W1-4 summary): "Post-store NLI runs fire-and-forget off the MCP hot path. Contradiction > threshold writes `Contradicts` edge to GRAPH_EDGES. Entailment writes `Supports` edge." The vision identifies the limitation — NLI fires only at store time, only for HNSW neighbours of the new entry — and implicitly requires systematic graph densification as part of the "typed knowledge graph formalizes relationships" mandate.

The vision's intelligence pipeline section states: "A typed knowledge graph formalizes relationships — not just what agents retrieve together, but why: support, contradiction, supersession, dependency." A graph where pre-NLI entries remain isolated contradicts this — col-029's `isolated_entry_count` metric makes that isolation visible. crt-029 acts directly on that observable gap.

The feature also feeds the W3-1 GNN feature vector. Entry features include `nli_edge_confidence` and `graph_degree`; a sparse graph from the pre-NLI era degrades both. crt-029 improves the quality of W3-1's training data before W3-1 ships.

No vision principles are contradicted. The feature does not introduce new domain coupling, does not touch security primitives, and does not add schema migrations or crate dependencies.

### Milestone Fit

**PASS.**

crt-029 is a Wave 1A / W1-4 extension feature. The wave is "Intelligence Foundation" and specifically W1-4 established NLI-derived graph edges. crt-029 extends that by running NLI systematically over the existing entry population rather than only on newly stored entries.

There is no Wave 2 (deployment), Wave 3 (GNN), or future-wave capability imported into this feature. `Prerequisite` edge inference — the one capability that would bleed into W3-1 territory — is explicitly deferred in SCOPE.md and consistently excluded across all three source documents.

The feature's dependency chain is sound: it builds on W1-1 (typed graph), W1-2 (rayon pool), W1-4 (NLI pipeline), col-029 (graph cohesion metrics), and col-030 (suppress_contradicts interaction). All are complete features.

### Architecture Review

**PASS.**

The architecture is thorough and addresses all SCOPE-RISK-ASSESSMENT.md risks with concrete ADRs:

- **ADR-001**: New `nli_detection_tick.rs` module (mandatory split — `nli_detection.rs` is actually 1,373 lines, not ~650 as SCOPE.md estimated). The architecture correctly escalates the split from "judgment call" (SCOPE.md) to "mandatory" — this is the right call given the measured line count.
- **ADR-002**: `write_inferred_edges_with_cap` as a named variant with scalar threshold parameters (not `InferenceConfig` dependency). Directly addresses SR-08 / R-08 cap-logic testability.
- **ADR-003**: Source-candidate bound derived from `max_graph_inference_per_tick` (no separate config field). Addresses SR-02 / R-02.
- **ADR-004**: Separate `query_existing_supports_pairs()` with targeted SQL on the `UNIQUE` index. Addresses SR-04 / R-04.

The eight-phase algorithmic design (Phase 1 guard through Phase 8 write) is precise and sequentially correct. The component interaction diagram is accurate and complete.

SR-06 pool ambiguity is handled in architecture §SR-06 by asserting `compute_graph_cohesion_metrics` uses `read_pool()` per Unimatrix entry #3619. However, the RISK-TEST-STRATEGY.md correctly flags that two conflicting ADRs (#3593 write-pool, #3595 read-pool) remain unreconciled. The architecture assumes the correct answer but has not resolved the conflict. This is the open item flagged in R-06.

One noteworthy gap: SCOPE.md Proposed Approach §Layer 2, step 10 says "Log total edges written at `debug` level." The architecture documents this but the specification FR-01 last bullet also captures it. Both are consistent.

### Specification Review

**PASS.**

The specification is complete and adds two acceptance criteria beyond SCOPE.md:

- **AC-18† (InferenceConfig struct literal grep)**: Directly implements SR-07 / R-07 recommendation. The grep count of 52 occurrences (confirmed at spec time) is a useful concrete gate datum.
- **AC-19† (Contradicts threshold floor)**: Directly implements SR-01 / R-01 recommendation. Ensures the tick cannot silently suppress search results via col-030 by using a softer contradiction threshold.

These additions are improvements to the scope, not scope additions — they make the specified acceptance criteria more rigorous without adding functionality.

All 17 scope acceptance criteria (AC-01 through AC-17) are present in the specification and are strengthened with verification methods. The specification adds testability detail that SCOPE.md left implicit (e.g., AC-06 adds "integration test verifying that a Deprecated entry is never a candidate").

The Open Questions section (OQ-01 through OQ-03) correctly routes unanswered scope questions to the architect. OQ-01 (compute_graph_cohesion_metrics pool) is confirmed resolved in the architecture doc but the ADR conflict is unreconciled (see R-06 below). OQ-02 and OQ-03 are resolved in ADR-002 and ADR-004 respectively.

One minor inconsistency: SCOPE.md Proposed Approach says `nli_detection.rs` is "currently ~650 lines." The architecture doc (§Component 3) states the actual count is 1,373 lines. The specification's NFR-05 and C-08 carry forward the SCOPE.md estimate (~650 lines) while the architecture uses the actual count. This is a documentation inconsistency with no functional impact — the architecture's mandatory split is the correct outcome regardless of which line count is used. Not a WARN because the outcome (split is mandatory) is correctly derived in the architecture.

### Risk Strategy Review

**WARN — R-06 ADR conflict unresolved.**

The RISK-TEST-STRATEGY.md is comprehensive. It covers 13 risks across Critical/High/Medium/Low priorities with scenario-level test coverage mapping. All 8 SCOPE-RISK-ASSESSMENT.md risks are traced in the scope risk traceability table.

The test scenario coverage is appropriate:
- Critical risks (R-01, R-02, R-09) each have 3-4 scenarios including the specific at-threshold boundary cases.
- High risks (R-03, R-06, R-07, R-08, R-10, R-11) each have 2-3 scenarios plus pre-merge gate conditions.
- Medium risks have coverage ratios consistent with their severity.

The pre-merge gate list is precise and actionable:
- `grep -rn 'InferenceConfig {'` (52 occurrences, AC-18†)
- `wc -l nli_detection_tick.rs` (≤ 800 lines, NFR-05)
- `grep -n 'spawn_blocking' nli_detection_tick.rs` (zero results, C-01/AC-08)
- `pub(crate)` promotions grep (R-11)

**R-06 WARN**: Two conflicting ADRs exist in Unimatrix (#3593 says `write_pool_server()`, #3595 says `read_pool()`) for `compute_graph_cohesion_metrics`. The RISK-TEST-STRATEGY.md correctly identifies this as "UNRESOLVED — human decision required." The architecture asserts the read-pool answer per entry #3619 but does not deprecate the conflicting ADR. The spec C-12 constraint says "the architect must confirm" — the architecture doc does confirm, but the conflicting entry #3593 remains active.

**Required action**: Before the implementation brief is written, a human must reconcile ADRs #3593 and #3595 by deprecating the incorrect entry. If #3595 (read_pool) is correct, #3593 must be deprecated. This is a knowledge integrity task, not a feature scope task.

R-09 (tokio Handle inside rayon closure) is correctly elevated to Critical based on prior crt-022 failure pattern (entries #3339, #3353). The architecture and specification both explicitly prohibit any `.await` inside the rayon closure. The test strategy calls for code review as the primary gate — appropriate given this is a compile-invisible runtime panic.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for vision alignment patterns — found 1 relevant pattern (#2298, config key semantic divergence) and 2 unrelated patterns. Entry #2298 (config key semantic divergence) was checked against crt-029; no semantic divergence found in the four new `InferenceConfig` fields. All field names, types, defaults, and valid ranges in source docs match SCOPE.md exactly.
- Stored: nothing novel to store — the R-06 conflicting ADR pattern is a feature-specific documentation artifact, not a recurring cross-feature alignment failure. No new vision-alignment anti-pattern observed in crt-029 that generalizes beyond this feature.
