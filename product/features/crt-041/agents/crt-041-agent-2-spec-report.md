# Agent Report: crt-041-agent-2-spec

## Output

- `/workspaces/unimatrix/product/features/crt-041/specification/SPECIFICATION.md`

## Summary

Specification written for crt-041 Graph Enrichment (S1, S2, S8 Edge Sources).

- 32 functional requirements covering S1, S2, S8 behavior, named constants, InferenceConfig
  fields, module structure, tick ordering, write_graph_edge prerequisite, and
  GraphCohesionMetrics extensions.
- 8 non-functional requirements: no-ML constraint, infallible tick pattern, latency budget
  (500 ms S1+S2 / 1000 ms S8), no schema migration, idempotency, backward compat,
  file size limit, eval gate.
- 32 acceptance criteria (AC-01 through AC-32), each independently verifiable. All 24
  SCOPE.md ACs are covered and expanded. AC-03/09/19 (quarantine guard), AC-10/11 (S2
  injection guard), AC-20 (malformed JSON handling), and AC-28 (write_graph_edge pre-flight)
  are new ACs derived from scope risks.

## Key Decisions Reflected

- S2 default vocabulary is empty (operator opt-in); 9-term software-engineering list is
  documented in config comment only. Respects domain-agnostic product vision (W0-3).
- S2 term matching: space-padded instr() word boundary pattern, sqlx bound parameters only.
- S8 watermark write-after-edges ordering; malformed-JSON rows advance the watermark to
  prevent indefinite stall.
- cross_category_edge_count and isolated_entry_count added to GraphCohesionMetrics; isolation
  computed in Rust (active_entry_count - connected_entry_count) per col-029 ADR-002.
- Eval gate must run after at least one complete tick post-delivery (SR-09).

## Open Questions for Architect

- OQ-01 (SR-02): S1 GROUP BY materialization before LIMIT — needs EXPLAIN QUERY PLAN
  verification or two-phase query restructure.
- OQ-02 (SR-05): Confirm `category` column is the correct predicate for
  cross_category_edge_count.
- OQ-03 (SR-06): Whether existing crt-039 compaction covers S1/S2 source values or
  orphaned-edge cleanup is deferred.
- OQ-04 (SR-09): Recommended procedure for CI eval gate to know when one full tick has
  completed post-delivery.
- OQ-05: Confirm MRR baseline is still 0.2875 post-crt-040 or provide updated value.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — 17 entries returned. Most relevant: #4025
  (write_nli_edge — do not reuse); #3592 (GraphCohesionMetrics two-query pattern); #4026
  (S8 watermark pattern); #3822 (near-threshold oscillation AC requirement); #3591 (EDGE_SOURCE
  naming pattern). All findings incorporated.
