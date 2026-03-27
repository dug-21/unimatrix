# Agent Report: col-031-agent-3-risk

## Output

- Produced: `product/features/col-031/RISK-TEST-STRATEGY.md`
- Mode: architecture-risk

## Risk Summary

| Priority | Count |
|----------|-------|
| Critical | 2 |
| High | 6 |
| Medium | 5 |
| Low | 1 |
| **Total** | **14** |

## Top Risks by Severity

1. **R-01 (Critical)** — Silent wiring bypass: `run_single_tick` constructs `SearchService` directly, bypassing `ServiceLayer`. Lesson #3216 documents this exact failure mode. `PhaseFreqTableHandle` must be threaded through all 7 construction sites per pattern #3213. ADR-005 mitigates via required non-optional parameter (compile-error on miss).

2. **R-02 (Critical)** — Vacuous AC-12 gate: `replay.rs` does not forward `current_phase` to `ServiceSearchParams`. All eval replay runs have `phase_explicit_norm = 0.0`; AC-12 passes trivially regardless of weight. AC-16 (`replay.rs` one-line fix) is a hard prerequisite. Gate 3b must reject any AC-12 PASS without evidence of non-null `current_phase` in eval scenario output.

3. **R-03 (High)** — `use_fallback` guard absent or fires after `phase_affinity_score` call: during cold-start with a provided phase, fused scoring applies uniform `0.05` boost to all candidates, breaking pre-col-031 score identity (NFR-04 violation).

4. **R-04 (High)** — Wrong cold-start return for PPR: if `phase_affinity_score` returns `0.0` instead of `1.0` on cold-start, PPR personalization vector collapses to all-zero seeds.

5. **R-05 (High)** — `CAST(json_each.value AS INTEGER)` omitted: query returns zero rows silently; `use_fallback = true`; feature ships inert. Pattern #3678 and Unimatrix #3681 document the verified cast form.

## Scope Risk Traceability

All 7 SR-XX risks traced. SR-01 → R-01/R-14; SR-02 → R-11; SR-03 → R-02/R-11; SR-04 → R-10 (accepted); SR-05 → R-08; SR-06 → R-03/R-04; SR-07 → R-12.

## Eval Harness Correction Applied

Per spawn prompt correction: AC-16 target is `replay.rs`, NOT `extract.rs`. `extract.rs` and `output.rs` already handle phase. The gap is entirely in `replay.rs` not forwarding `current_phase` to `ServiceSearchParams`. This is reflected in R-02, the Coverage Summary, and the Scope Risk Traceability table.

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for lesson-learned failures gate rejection — found #2758, #3579. Elevated R-02 and R-14.
- Queried: `/uni-knowledge-search` for RwLock Arc handle threading pattern — found #1560, #3213, #2961. Confirmed R-01 Critical/High.
- Queried: `/uni-knowledge-search` for json_each query_log — found #3681, #3678. Informed R-05.
- Queried: `/uni-knowledge-search` for eval harness AC-12 AC-16 — found #3688. Confirmed R-02 Critical rating.
- Stored: nothing novel — all relevant patterns already captured in #1560, #3213, #3216, #3678. No duplicate storage warranted.
