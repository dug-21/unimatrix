# Agent Report: crt-039-agent-3-risk

**Role**: Risk Strategist (architecture-risk mode)
**Feature**: crt-039 — Tick Decomposition: Decouple Structural Graph Inference from NLI Gate

## Output

- `/workspaces/unimatrix/product/features/crt-039/RISK-TEST-STRATEGY.md` — written

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 4 |
| High | 4 |
| Medium | 4 |
| Low | 1 (absorbed into medium coverage) |

Total risks: 12 (R-01 through R-12)

## Top Risks by Severity

**R-01 (Critical)** — Path B / Path A boundary: Supports edges written without NLI scores if the `get_provider()` guard fires too late or not at all. Silent data corruption. Mitigated by ADR-001 Option Z structural placement; validated by TC-02 integration test.

**R-02 (Critical)** — Test coverage gap: TR-01 (old no-op test) removed without confirmed replacement. History (lesson #3579) shows gate-3b delivery where entire test modules were absent. TC-01 and TC-02 must be present and integration-level before gate-3c.

**R-03 (Critical)** — Mutual-exclusion gap at cosine 0.50 boundary: Architecture claims disjoint-by-construction; spec (FR-06, AC-13) requires explicit Phase 4 set subtraction. If subtraction is omitted, a pair at cosine exactly 0.50 satisfying the `informs_category_pairs` filter could be written as both edge types when NLI is enabled.

**R-04 (Critical)** — `NliCandidatePair::Informs` / `PairOrigin::Informs` dead-code variants: partial removal (declaration removed but match arms retained, or vice versa) causes silent gaps in Phase 6 text-fetch for the Supports path. Compiler catches full absence; match arm orphans may not surface without exhaustive match audit.

## Open Questions Carried Forward

The four OQs from the spec writer are resolved in architecture but warrant implementor attention:
- **OQ-01** (drop `config` from `apply_informs_composite_guard`): Resolved by ADR-002 — `config` parameter is removed. All call sites must be updated.
- **OQ-02** (Phase 8b control flow in Option Z): Resolved by ADR-001 — Phase 8b write loop is outside the Path B block, after the `get_provider()` early-return. R-07 covers the nesting error risk.
- **OQ-03** (`format_nli_metadata_informs`): Resolved by ADR-002 — replaced by `format_informs_metadata`. R-08 covers dead-code / metadata content risk.
- **OQ-04** (file size submodule split): Remains an implementor decision per NFR-06. No risk score assigned — compiler and file-size check at gate-3b.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found entries #3579, #2758, #2577 (gate failures, test omission, boundary test ordering). Directly elevated R-02 to Critical.
- Queried: `/uni-knowledge-search` for "risk pattern nli_detection_tick graph inference tick" — found #3937 (NLI neutral-zone pattern being removed), #3723 (threshold tuning blind without log), #3949 (per-guard negative tests). Confirmed SR-06/R-10 and FR-14 observability requirement severity.
- Queried: `/uni-knowledge-search` for "dead code enum variant removal Rust" — found #3437/#3441. Informed R-04 framing around `#[allow(dead_code)]` suppression vs true deletion.
- Stored: nothing novel to store — risks are feature-specific. No cross-feature pattern confirmed yet.
