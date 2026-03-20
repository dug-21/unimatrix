## ADR-002: NLI Entailment Score Replaces rerank_score for Search Re-ranking Sort

### Context

The current search pipeline sorts candidates using `rerank_score(similarity, confidence, cw)`, a composite formula:

```
rerank_score = 0.85 * cosine_similarity + 0.15 * confidence + co_access_boost(max 0.03)
```

This formula measures topical closeness (bi-encoder cosine) blended with entry quality (confidence). It does not measure whether an entry *answers* the query.

NLI entailment score directly measures whether the candidate passage is the "hypothesis" that follows from the query as the "premise" — i.e., whether the candidate is a valid answer to the query. This is the semantic signal that the bi-encoder/rerank_score combination approximates but cannot directly compute.

Three design options were considered:

**Option A — Pure replacement**: NLI entailment score completely replaces `rerank_score` as the sort key when NLI is active. The `rerank_score` computation is retained in code for the fallback path.

**Option B — Additive blend**: `final_score = alpha * nli_entailment + (1 - alpha) * rerank_score`. Both signals contribute; blend weight `alpha` is config-driven.

**Option C — Two-pass**: NLI re-ranks; then co_access_boost and status_penalty are re-applied after NLI sort.

**Analysis:**

Option B's weakness is that the blended formula inherits all of `rerank_score`'s calibration assumptions (the 0.85/0.15 split, confidence weight adaptation) and adds NLI as an additional factor. If NLI is well-calibrated, the blend dilutes its signal. If NLI is poorly calibrated (e.g., short terse ADR entries), the blend moderates the damage. The blend formula is a hedge for an unproven model.

The eval gate (AC-09) is designed exactly to detect whether NLI improves or regresses P@K/MRR relative to baseline. If NLI score alone is worse than the blend, the eval harness will surface that. We commit to pure replacement here and use the eval gate as the quality proof.

Option C (two-pass) re-applies `co_access_boost` after NLI sort. The architecture adopts this: NLI sort determines semantic relevance order; `co_access_boost` and `status_penalty` apply after truncation (as they do today). The co-access boost is deliberately post-sort because it measures session context, not query relevance.

D-02 in SCOPE.md resolves this question: pure replacement in this first iteration, blended formula as a follow-on if eval warrants.

### Decision

**NLI entailment score is the sole sort key for the NLI-active search path.** The pipeline becomes:

```
embed
→ HNSW top-nli_top_k (expanded candidate pool)
→ quarantine filter
→ status filter / penalty application (score = base_score * status_penalty)
→ supersession injection
→ NLI batch score (query, each_candidate)
→ SORT by nli_scores.entailment DESCENDING
→ truncate to top-K
→ co-access boost
→ floors
```

The sort step uses `nli_scores.entailment` as the sole ordering key. `rerank_score` is not called on the NLI-active path. The `rerank_score` function remains in the codebase, called only on the fallback path (NLI not ready or `nli_enabled = false`).

**Status penalty interaction**: entries penalized by status (deprecated, superseded) still receive their penalty multiplier before NLI scoring. The NLI score is computed on the raw text of the entry regardless of status. Status penalty is applied as a post-NLI multiplicative modifier only if the search pipeline requires it; see FR-14 for the exact pipeline order.

Specifically, the status/penalty step applies *before* NLI scoring (so entries with severe penalties may have their NLI scores depressed by a multiplicative factor before the sort). This matches the original pipeline's intent: status-penalized entries should rank lower even if their content strongly entails the query.

**Rollback path**: `nli_enabled = false` in `config.toml` immediately reverts to `rerank_score`. No code change, no deployment, no data migration.

### Consequences

**Easier:**
- Clean pipeline: one sort key, no calibration tradeoffs between cosine and NLI weights.
- Eval gate provides empirical validation of the replacement decision.
- Rollback is trivially a config toggle.
- `rerank_score` code is preserved; no deletion risk.

**Harder:**
- If NLI is poorly calibrated for short, terse, tag-heavy entries (e.g., ADRs with 3-word content), entailment scores may be near 0.33 (uniform distribution) and ranking degrades. The eval harness using the real knowledge base is the primary guard against this.
- SR-05 (risk assessment): no intermediate blending means if MiniLM2 regresses on the specific knowledge base under test, there is no per-entry fallback — only the global `nli_enabled = false` toggle. Operators must review the eval report before shipping.
- The `rerank_score` composite (especially co_access_boost) captures session-context signal that pure NLI entailment does not. Co-access boost is retained as a post-sort step, partially addressing this.

**Follow-on**: If eval results show NLI alone is weaker than the composite on terse entries, ADR-002 is superseded by a blended formula ADR in a follow-on feature.
