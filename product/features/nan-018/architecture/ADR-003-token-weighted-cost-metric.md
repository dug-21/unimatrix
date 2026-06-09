# ADR-003 (nan-018): Token-Weighted Cost Metric — `cost = Σ(per-result token-proxy)`

### Context

The product's retrieval objective is **relevance/cost balance**. The harness reports latency, not the noise an agent pays to read — so the cost half is unmeasurable today, and the downstream cost-of-noise findings (rewritten ass-073) have nothing to measure with. Cost must be **token-weighted, not result-count**: the same `k` carries wildly different token loads (a 50-token snippet vs a 500-token one), and the cost an agent actually pays is tokens-read. OQ-1 (resolved, architecture): build the **explicit token-weighted metric**; a precision+k proxy survives only as an explicitly-justified in-design call and never displaces the token-weighted definition, and is **never** a deferral (the downstream spike cannot measure cost-of-noise until this metric exists). SR-02: "token-proxy" is not a real tokenizer; a crude proxy (char/4) can mis-rank sets and mislead downstream — the proxy's definition and error bars must be stated, not deferred.

### Decision

**Build the explicit token-weighted cost metric. No proxy-narrowing — the architecture call is to ship the real thing.**

```
cost_tokens (per profile, per scenario) = Σ over each returned result r of token_proxy(r)
```

`k` (set size) is reported as a **secondary** axis (derivable from `entries.len()`); `cost_tokens` is primary. Surfaced on `ProfileResult.cost_tokens: f64` and in `report` next to P@K/MRR/latency.

**`token_proxy` definition (stated, per SR-02).** The proxy counts tokens over the *payload an agent reads for that result* — the entry's `title` + `content` (the snippet text the search surfaces), not the score metadata. Two-tier:

- **Default (faithful tier):** a real, deterministic subword token count using the same tokenizer family the embedding model uses (the `tokenizers` crate already in the embed dependency tree for all-MiniLM). This is the faithful signal; it is deterministic given the corpus text, so it is reproducible across runs (a property the drift guard relies on).
- **Documented fallback (stated error bars):** if the tokenizer is unavailable in the eval context for a given result, fall back to a **whitespace-and-punctuation word count × 1.3** as a coarse estimate. This fallback is **documented in the Band-2 config-knob/cost reference** with its known error: word×1.3 systematically under-counts subword-split rare tokens and over-counts on punctuation-heavy text; empirically within roughly ±20% of the subword count on knowledge-base prose. The fallback is logged (which tier produced the number) so downstream reads cost figures with the right confidence.

**Why not char/4:** char/4 (the crude proxy SR-02 warns against) ignores vocabulary and is the most mis-ranking option; it is explicitly rejected. The faithful tier is the default and lean; the word×1.3 fallback exists only for environments where the tokenizer cannot load, never as the primary.

**This is not a precision+k proxy.** The OQ-1 escape hatch (narrow to precision+k) is **not** taken. Rationale for declining it: precision+k cannot express "same k, different token load → different cost," which is the entire point of the metric; taking the proxy would re-create the gap nan-018 exists to close.

**Regression surfacing.** `find_regressions` adds cost as an advisory signal: a candidate whose `cost_tokens` exceeds baseline by more than **ε = 0.0** — any growth is reported, none blocks — is listed in the human-reviewed regression block. Consistent with the existing gate, which is advisory ("no automated gate logic is applied"). **ε = 0.0, advisory (report-only), Wave-1 is LOCKED (human-ratified — ARCHITECTURE §7.1).** Rationale: a blocking ε would breach nan-018's boundary (eval is deliberately NOT a workflow gate) and would be premature tuning — the cost threshold is ASS-037 authority, and the downstream ass-073 spike has not yet produced a cost distribution to set a defensible non-zero number against. ε = 0.0 reports everything and pre-commits to nothing.

### Consequences

**Easier:** "Relevance went up but cost went up more" becomes a single visible result in the same sweep (AC-09, AC-14). The faithful subword count makes cost comparable to the agent's real token spend, so ass-073's cost-of-noise conclusions rest on a real signal (SR-02 mitigated). Deterministic proxy keeps eval runs reproducible.

**Harder:** Reusing the embedding tokenizer couples the cost metric to the embed dependency; if the tokenizer load path differs in the eval context, the fallback tier engages and numbers carry the documented ±20% band — acceptable but must be read with the tier label. The cost metric adds a per-result text pass; negligible at fixture-corpus scale, worth noting at snapshot scale. Computing token counts over `content` requires the corpus snapshot to actually carry entry content (it does — entries are materialized from the DB), so no new data dependency.
