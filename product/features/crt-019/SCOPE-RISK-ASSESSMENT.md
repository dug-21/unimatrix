# Scope Risk Assessment: crt-019

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | Bayesian empirical prior (α₀/β₀) computed from voted-entry population — with only 192 live entries and few receiving votes, the population may be too sparse for method-of-moments to produce stable estimates; noisy α₀/β₀ would destabilize helpfulness scores across all entries on every refresh tick | High | High | Spec must define minimum voted-entry count threshold below which cold-start default α₀=β₀=3 is used instead of empirical estimation; also specify whether α₀/β₀ are recomputed per-tick or cached between ticks |
| SR-02 | `observed_spread` (p95–p5) and Bayesian prior (α₀/β₀) are both computed during the same confidence refresh tick and cached as shared runtime state — these are two distinct computed values that must be consistent with each other within a tick; concurrent MCP calls reading stale cached values mid-refresh could encounter a partially-updated state | Med | Med | Architect must define the atomicity boundary: are `observed_spread` and `α₀/β₀` updated together under a single lock, or is stale-read on these values acceptable between ticks? |
| SR-03 | `rerank_score()` is a pure function referencing `SEARCH_SIMILARITY_WEIGHT` as a compiled constant — adaptive blend requires either converting this to a runtime parameter or using a thread-safe shared value; existing tests assert `SEARCH_SIMILARITY_WEIGHT == 0.85` exactly (see `search_similarity_weight_is_f64` and `rerank_score_f64_precision`); these tests must change | High | High | Architect must decide: (a) pass `confidence_weight` as a parameter to `rerank_score`, or (b) introduce a runtime-readable atomic/RwLock value; option (a) requires updating all 6+ `rerank_score` call sites in search.rs; option (b) adds shared state to a previously stateless engine |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | `auto_extracted_new()` test profile uses `Status::Proposed` with `trust_source: "auto"` — if `base_score` differentiation is applied only to `Status::Active` entries but Proposed entries share the same base path, the T-REG-01 ordering `auto > stale > quarantined` may invert when the base score for auto+Proposed drops below stale; SCOPE.md §Constraints acknowledges this risk but does not resolve it | High | Med | Spec must explicitly state whether `base_score` differentiation applies to `Proposed` status or only `Active`; resolve before architecture so T-REG-01 update path is clear |
| SR-05 | Doubled `access_count` for `context_lookup` (×2) bypasses the existing UsageDedup access filter — UsageDedup deduplicates per-agent-per-entry but the ×2 multiplier is a DB-level increment, not a second dedup-filtered call; if UsageDedup suppresses the access call entirely (repeat call by same agent), no increment occurs; if it allows through, ×2 is applied; the behavior differs from what SCOPE.md implies | Med | Med | Spec must confirm: does UsageDedup filter operate before or after the multiplier is applied? Is a repeated lookup by the same agent worth 0 or 2 increments? |
| SR-06 | T-REG-02 deliberately fails on weight change — SCOPE.md §Constraints calls this out, but the update path requires simultaneously changing weight constants AND all golden value assertions in T-REG-02; with 7 coordinated changes in one cycle, the risk is that a partial implementation passes compilation but fails calibration tests in non-obvious ways before the T-REG-02 update | Med | Med | Implementation agent must update T-REG-02 constants as the first step of Change 2, not last; spec should explicitly order this |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | `context_get` implicit helpful vote injection adds a second fire-and-forget `spawn_blocking` path — entry #735 documents pool saturation from multiple concurrent spawn_blocking tasks per MCP call; `context_get` currently spawns one task; adding a second vote-injection task (or modifying the existing `record_access` call to carry the implicit helpful signal) requires care not to reintroduce the batching regression fixed in vnc-010 | High | Med | Architect must confirm the implicit helpful vote is folded into the existing `record_access` call (by modifying `UsageContext.helpful`) rather than spawning an additional task; a separate spawn is the regression path |
| SR-08 | `confidence_refresh` batch at 500 entries runs inside `spawn_blocking` holding `store.lock_conn()` for up to 200ms — during this window any other MCP tool call path that calls `lock_conn()` directly (without spawn_blocking) would block a tokio worker thread; entry #771 documents exactly this pattern causing runtime starvation; confirm all MCP paths are fully mediated by spawn_blocking with no direct lock_conn calls remaining | Med | Med | Architect should verify post-vnc-010 that zero direct `lock_conn()` calls exist in async context before recommending 500-entry batch |

## Assumptions

The following SCOPE.md assumptions could invalidate the approach if wrong:

- **§Proposed Approach, Change 1**: Assumes the voted-entry population is large enough by the time crt-019 ships to produce stable α₀/β₀ via method of moments. With 192 entries and historically few votes, this may not hold. The cold-start default path (α₀=β₀=3) may be the only path exercised in practice for the foreseeable future — not a precondition blocker, but the empirical prior path is unvalidated.
- **§Background Research, Change 4**: Assumes `observed_spread` can be computed during the refresh tick and cached without a race condition affecting concurrent search calls. The current architecture has no shared mutable state in the search path — this would be the first such dependency.
- **§Constraints, base_score signature**: The SCOPE notes the compute_confidence path as lower-risk than a signature change, but neither path is committed. The architect needs to choose before spec; the two options have different call-site blast radii.

## Design Recommendations

- **SR-01, SR-02**: Specify a minimum voted-entry threshold (e.g., ≥ 10 entries with votes) for empirical prior activation; document α₀/β₀ as updated atomically with `observed_spread` in a single refresh tick; define acceptable staleness window.
- **SR-03**: Prefer parameter-passing over shared runtime state for `confidence_weight` — keeps `rerank_score` pure and avoids introducing async-visible mutable state into the engine crate; all call sites in search.rs are close together.
- **SR-04**: Commit to base_score differentiation applying to `Active` only (not `Proposed`) to preserve T-REG-01 ordering; document this as a constraint in the specification.
- **SR-07**: Specify that the implicit helpful vote for `context_get` is implemented by setting `UsageContext.helpful = Some(true)` in the existing `record_access` call, not as a separate spawn.
