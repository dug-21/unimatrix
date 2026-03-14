# Agent Report: crt-019-agent-2-spec

## Output

- SPECIFICATION.md written to `product/features/crt-019/specification/SPECIFICATION.md`
- 13 acceptance criteria: AC-01 through AC-12 (AC-08 split into AC-08a and AC-08b)

## Key Decisions

1. **AC-02 corrected**: SCOPE.md stated `helpfulness_score(2, 2, α₀, β₀) > 0.5`. The actual
   Bayesian posterior with cold-start prior is `(2+3)/(4+6) = 0.5` exactly — equal votes return
   neutral. The AC is corrected to `== 0.5`, with responsiveness demonstrated by the unhelpful-vote
   assertion `(0, 2, 3, 3) = 0.375 < 0.5`.

2. **SR-04 resolved in spec**: base_score differentiation applies to `Status::Active` only.
   `Status::Proposed` with `trust_source = "auto"` retains base_score = 0.50. Documented as
   Constraint C-03 with explicit rationale (T-REG-01 ordering preservation).

3. **SR-05 resolved in spec**: UsageDedup access filtering precedes the ×2 multiplier for
   `context_lookup`. Repeated lookup by the same agent produces 0 increments, not 2. Documented
   as Constraint C-05.

4. **SR-07 resolved as constraint**: Implicit helpful vote for `context_get` must be implemented
   by setting `UsageContext.helpful` before the existing spawn_blocking fires. A second spawn is
   explicitly prohibited by C-04.

5. **SR-03 deferred to architect**: Adaptive blend state management (parameter vs. Arc/atomic) is
   delegated to ARCHITECTURE.md. The specification mandates the formula and behavior only.

6. **SR-02 deferred to architect**: Atomicity boundary for α₀/β₀ and observed_spread within a
   refresh tick is an architectural decision.

7. **Voted-entry threshold set at ≥ 5**: SR-01 recommended ≥ 10 entries; spawn prompt specified ≥ 5.
   Spec uses ≥ 5 as directed, with the empirical prior clamped to [0.5, 20.0] to guard against
   degenerate estimates from sparse populations.

8. **AC-06 formula**: The SCOPE.md formulation `clamp(spread * 1.25, 0.15, 0.25)` is preserved
   verbatim. At spread = 0.10: `0.10 * 1.25 = 0.125`, clamped to floor 0.15. At spread = 0.20:
   `0.25` exactly. At spread = 0.30: capped at 0.25.

## Open Questions for Architect

- SR-02: Are α₀/β₀ and observed_spread updated atomically under a single lock, or is stale-read
  acceptable between refresh ticks?
- SR-03: Does `rerank_score` receive `confidence_weight` as a parameter (pure function, 6+
  call-site updates) or read from an Arc/atomic (shared state, zero call-site changes)?
- SR-08: Confirm post-vnc-010 that zero direct `lock_conn()` calls exist in async context before
  architect approves 500-entry batch recommendation.

## Knowledge Stewardship

- Queried: /uni-query-patterns for confidence formula scoring retrieval signal — no prior
  AC patterns or confidence formula procedures in active knowledge. Relevant ADRs found: #705
  (W_COAC deletion, 0.92 invariant history), #706 (two-mechanism co-access), ADR-002 server
  dedup/vote-correction, ADR-004 fire-and-forget usage recording. All consistent with this spec.
