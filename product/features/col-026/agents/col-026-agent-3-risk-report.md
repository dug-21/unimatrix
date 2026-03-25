# Agent Report: col-026-agent-3-risk

**Mode**: architecture-risk
**Agent ID**: col-026-agent-3-risk
**Artifact**: `product/features/col-026/RISK-TEST-STRATEGY.md`

## Summary

Produced the architecture-risk test strategy for col-026. Identified 13 risks across Critical/High/Med/Low priority tiers. All 8 scope risks (SR-01 through SR-08) are traced to architecture risks and/or specific ADRs/spec requirements.

## Risk Register Summary

| Priority | Count | Key Risks |
|----------|-------|-----------|
| Critical | 5 | R-01 (ts conversion bypass), R-02 (phase window edge cases), R-03 (GateResult inference fragility), R-04 (batch query partial results), R-05 (is_in_progress three-state) |
| High | 5 | R-06 (metric direction table), R-07 (formatter section reorder regression), R-08 (threshold regex strip-and-replace), R-09 (attribution path fallback), R-10 (multi-phase hotspot annotation) |
| Med | 2 | R-11 (tenth threshold site future regression), R-12 (Some(vec![]) vs None) |
| Low | 1 | R-13 (FeatureKnowledgeReuse construction site migration) |

## Top Risks for Human Attention

**R-03 (Critical/High)** — GateResult inference from free-form outcome text. The `contains("pass")` substring match on free-form agent-authored text is the highest-likelihood breakage point. Scenario 8 (outcome `"compass"` containing "pass" as substring) documents a known fragility. The spec should clarify whether word-boundary matching is required or naive substring matching is accepted.

**R-07 (Med/High)** — Formatter section reorder regression. With Recommendations moving from position 9 to position 2 and two new sections inserted, every existing section-position assumption in the test suite is invalidated. A golden-order test (asserting all 12 section headers in sequence) is the only adequate regression guard. Stored as pattern #3426.

**R-05 (Critical/Med)** — `is_in_progress` three-state semantics. The `None` branch (pre-col-024 historical retros) is the most likely to be missed by implementation agents — the naive default is `Some(false)` which silently corrupts historical retro output. ADR-001 and NFR-04 are the guard; test coverage of the `events = vec![]` → `None` path is the verifiable check.

## Open Questions for Spec/Implementation

1. **R-03 scenario 8**: Does `"compass"` (containing "pass") match `GateResult::Pass`? Should the inference use word-boundary matching rather than `str::contains`? The spec's keyword list at line 500 uses "pass", "complete", "approved" — but does not specify word-boundary semantics.

2. **R-09 path 4**: When all three attribution paths return empty, what is `attribution_path`? The spec does not define this case. Should it be `None` or a sentinel string like `"none"`?

3. **R-04 missing entries**: When a served entry ID returns no metadata from the batch query (quarantined/deleted), should it be counted as intra-cycle, excluded from the split, or counted as an "unknown" bucket? The spec describes the split but does not address missing metadata.

## SR Traceability Summary

All 8 scope risks traced:
- SR-01 → R-01 (ADR-002, NFR-02 enforce `cycle_ts_to_obs_millis`)
- SR-02 → R-04 (ADR-003 enforces single batch call)
- SR-03 → R-05 (ADR-001, NFR-04 enforce `Option<bool>`)
- SR-04 → R-07 (section-order golden test, NFR-05)
- SR-05 → R-08, R-11 (ADR-004 nine-site audit, general regex)
- SR-06 → R-12 (per-cycle simple check, None canonicalization)
- SR-07 → accepted risk (API surface pinned in ARCHITECTURE.md)
- SR-08 → R-13 (compile-time enforcement, three known sites)

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection retrospective formatter" — no results found in Unimatrix
- Queried: `/uni-knowledge-search` for "risk pattern observation slicing phase window timestamp" — found #3383 (cycle_events-first algorithm), #883 (Chunked Batch Scan)
- Queried: `/uni-knowledge-search` for "SQLite IN clause batch query entry lookup feature_cycle" — found #883 (Chunked Batch Scan), #3423 (ADR-003 col-026 batch query)
- Stored: entry #3426 "Formatter overhaul features consistently underestimate section-order regression risk — golden-output test required" via `/uni-store-pattern` — pattern visible across any multi-section formatter feature with section reordering; directly informs future col-* formatter work
