# crt-024: Ranking Signal Fusion (WA-0)

## Problem Statement

The current search ranking pipeline applies signals as sequential transformations rather than a
unified scoring function. Each step can override the judgment of a preceding step without any
principled relationship between them.

**Current pipeline (post crt-023):**

```
HNSW(k=nli_top_k)           ← pure similarity anchor
  → NLI re-rank             ← entailment sort; this judgment is discarded downstream
  → + co_access_boost       ← additive [0, 0.03], uncapped relative to NLI scores
  → + affinity_boost        ← (WA-2 planned) additive again
```

The structural defect: `apply_nli_sort` sorts candidates by `nli_scores.entailment *
status_penalty` and truncates. Then Step 8 **re-sorts the already-NLI-ranked results** by
`(rerank_score + utility_delta + boost + prov) * penalty`. An entry with low NLI entailment (0.3)
but high co-access count can accumulate enough additive boost to rank above a high-entailment (0.9)
entry. The NLI signal is computed and then discarded.

Measured magnitude: `MAX_CO_ACCESS_BOOST = 0.03`. `rerank_score` at crt-023 is
`(1-cw)*sim + cw*conf` where `cw ∈ [0.15, 0.25]` — so a typical score is in [0.0, 1.0]. A boost
of 0.03 is small but non-zero; with multiple boosted results and tight similarity clusters, it
produces visible ranking inversions. When WA-2 adds `affinity_boost` (up to 0.015 per entry), the
problem compounds.

**GH #329** patched co-access specifically; WA-0 fixes the structural cause so no individual signal
can be patched out of the problem again.

**Who is affected**: Every `context_search` call when both NLI is enabled and co-access data exists
for results in a candidate set with similar entailment scores.

**Why now**: WA-0 is explicitly listed as the prerequisite for all Wave 1A items. Adding WA-2
session-context signals to the broken formula makes the problem worse. The product vision states:
"WA-0 comes first. Before adding session-conditioned signals to the ranking pipeline, the pipeline's
existing signals must be fused correctly."

---

## Goals

1. Replace the sequential transformation pipeline with a single linear combination where **all six
   ranking signals** are normalized to [0, 1] and weighted proportionally via config-driven weights.
2. Add `w_sim`, `w_nli`, `w_conf`, `w_coac`, `w_util`, `w_prov` to `InferenceConfig` under
   `[inference]`, validated to sum ≤ 1.0 at startup. Every signal that currently influences ranking
   is inside the formula — nothing lives outside it as an additive afterthought.
3. Normalize all signals to [0, 1] before fusion: co-access (÷ MAX_CO_ACCESS_BOOST), utility delta
   (÷ UTILITY_BOOST), provenance (÷ PROVENANCE_BOOST).
4. Ensure no signal can negate another by additive accumulation — contribution is strictly proportional
   to its weight.
5. Preserve NLI graceful degradation: when NLI is absent or disabled, `w_nli` term is 0.0 and
   remaining weights are re-normalized proportionally.
6. Provide a principled extension point for WA-2 session-context signals (`w_phase * phase_boost`)
   as additional weighted terms in the same formula.
7. Set default weights based on principled signal reasoning (not placeholder splits) — these are
   W3-1's initialization point and training signal origin.
8. Update all existing ranking tests to reflect the new score semantics; no test logic regressions.
9. `EvalServiceLayer` wires `InferenceConfig` fusion weights through to `SearchService`, so that
   `[inference]` overrides in profile TOMLs take effect during eval harness runs.
10. D1–D4 eval harness run required before merging crt-024: compare old-behavior profile vs.
    new-weights profile on the pre-crt024 snapshot; human reviews report before merge.

---

## Non-Goals

- **WA-1 (Phase Signal + FEATURE_ENTRIES tagging)**: separate feature; this feature does not add
  `current_phase` to `SessionState` or FEATURE_ENTRIES schema changes.
- **WA-2 (Session Context Enrichment)**: the affinity boost formula and `category_counts` histogram
  are not implemented here. WA-0 creates the extension point (the formula) that WA-2 adds a term to.
- **WA-3 (MissedRetrieval Signal)**: post-store signal collection; no scope here.
- **WA-4 (Proactive Delivery)**: no changes to `context_briefing` injection pipeline.
- **W3-1 (GNN training)**: WA-0 creates the config-driven weight baseline W3-1 will learn to replace;
  training the GNN is not in scope.
- **Removing `rerank_score`**: the `rerank_score` function is retained as a utility function and
  used internally by the fused formula. It is not deleted.
- **Changing co-access data collection**: `compute_search_boost` and `MAX_CO_ACCESS_BOOST` remain
  unchanged in the engine crate. Only the fusion step changes — raw affinity is normalized before use.
- **Changing the `GRAPH_EDGES` schema or NLI post-store detection**: this is a scoring formula
  change only; the NLI detection pipeline (crt-023) is untouched.
- **Changing the eval harness**: no new eval profiles or eval gate for this feature. The change is
  formula-deterministic and fully testable with unit assertions.
- **Changing the MCP response schema**: `ScoredEntry.final_score` will reflect the new formula, but
  the field name and response shape are unchanged.
- **Config migration tooling**: operators update `config.toml` manually; no migration assistant.

---

## Background Research

### Current Pipeline (verified in crates/unimatrix-server/src/services/search.rs)

The 12-step pipeline as implemented post-crt-023:

```
Step 0: Rate check
Step 1: Query validation
Step 2-4: Embed query (rayon pool, L2-normalized)
Step 5: HNSW search — k=nli_top_k when nli_enabled, else params.k
Step 6: Fetch entries, quarantine filter
Step 6a: Status filter/penalty marking (produces penalty_map)
Step 6b: Supersession candidate injection
Step 7: Re-rank sort — NLI path (apply_nli_sort) OR fallback rerank_score sort
Step 8: Co-access boost (re-sorts results_with_scores again via boost_map)
Step 9: Truncate to k
Step 10: Apply floors
Step 11: Build ScoredEntry with final_score
Step 12: Audit
```

Step 7 and Step 8 are the problem. Step 7 (NLI path) sorts by `entailment * status_penalty`.
Step 8 re-sorts by `(rerank_score + utility_delta + boost + prov) * penalty`. The co-access
boost in Step 8 uses the same `rerank_score` formula it always did — NLI entailment is not a
term in Step 8's sort key. An entry sorted below another in Step 7 can overtake it in Step 8.

**Exact formula after crt-023 (NLI path):**
- Step 7 sort key: `nli_scores.entailment * status_penalty`
- Step 8 sort key: `(rerank_score(sim, conf, cw) + utility_delta + co_access_boost + prov_boost) * status_penalty`
- Step 8 uses `rerank_score = (1-cw)*sim + cw*conf` where `cw ∈ [0.15, 0.25]`
- NLI entailment score is not present in Step 8's formula

**Exact formula after crt-023 (fallback path, no NLI):**
- Step 7 sort key: `(rerank_score + utility_delta + prov_boost) * status_penalty`
- Step 8 sort key: `(rerank_score + utility_delta + co_access_boost + prov_boost) * status_penalty`
- These formulas are coherent with each other; the problem is NLI path + Step 8 interaction.

### Signal Ranges (verified)

| Signal | Source | Range | Notes |
|--------|--------|-------|-------|
| `similarity_score` | HNSW cosine (L2-normalized) | [0, 1] | Already normalized |
| `nli_entailment_score` | `NliScores.entailment` (softmax) | [0, 1] | Already normalized |
| `confidence_score` | `EntryRecord.confidence` (f64) | [0, 1] | Wilson score composite |
| `co_access_affinity` | `compute_search_boost` | [0, 0.03] | NOT normalized to [0,1] |

`MAX_CO_ACCESS_BOOST = 0.03` is a constant in `unimatrix-engine/src/coaccess.rs`. The co-access
boost formula is `ln(1 + count) / ln(1 + MAX_MEANINGFUL_CO_ACCESS) * max_boost` with
`MAX_MEANINGFUL_CO_ACCESS = 20`. Raw output is in [0, 0.03].

To normalize co-access affinity to [0, 1]: `co_access_affinity_norm = raw_boost / MAX_CO_ACCESS_BOOST`.

### Additional Signals in Current Pipeline (NOT in WA-0 fusion formula)

The current pipeline also applies `utility_delta` (±UTILITY_BOOST/UTILITY_PENALTY from effectiveness
classification) and `PROVENANCE_BOOST` (for boosted_categories). These are not in the WA-0 target
formula. The product vision formula has four terms; effectiveness and provenance signals are not named.

**Open question**: whether `utility_delta` and `PROVENANCE_BOOST` should be folded into the fused
formula as additional weighted terms, or preserved as pre-fusion adjustments to the confidence
signal. This scope proposes they remain as their own terms, preserving backward compatibility, with
the status_penalty multiplier applied to the entire fused score (same semantics as today).

### Config Structure (verified in crates/unimatrix-server/src/infra/config.rs)

`InferenceConfig` (struct with `#[serde(default)]`) is the `[inference]` section. It currently
holds NLI model parameters added by crt-023: `rayon_pool_size`, `nli_enabled`, `nli_model_name`,
`nli_model_path`, `nli_model_sha256`, `nli_top_k`, `nli_post_store_k`, `nli_entailment_threshold`,
`nli_contradiction_threshold`, `max_contradicts_per_tick`, `nli_auto_quarantine_threshold`.

WA-0 adds fusion weights: `w_sim`, `w_nli`, `w_conf`, `w_coac` — four `f64` fields with
`#[serde(default)]`. Validation at startup: `w_sim + w_nli + w_conf + w_coac <= 1.0`.

**Pattern precedent from dsn-001**: `InferenceConfig::validate()` already validates field ranges
for NLI thresholds; the same pattern is used for weight sum validation.

**Semantic divergence risk (Unimatrix entry #2298)**: This entry documents a known pattern where
the same TOML key carries different semantics than the product vision example. Weight defaults must
match the product vision's stated defaults — the researcher must confirm there are no pending ADRs
that redefine these defaults.

### crt-023 (W1-4) Interaction

crt-023 (PR #328) established the NLI path (Step 7, `apply_nli_sort`) and `NliServiceHandle`. It
introduced the structural problem: NLI ranks in Step 7, co-access re-ranks in Step 8. crt-024
resolves this by removing both Step 7 and Step 8 as separate sort passes and replacing them with a
single fused score computation.

The NLI score must flow through from Step 7 into the unified formula. Currently `apply_nli_sort`
returns `Vec<(EntryRecord, f64)>` where `f64` is the raw HNSW similarity — the NLI score is used
only for sorting and then discarded. crt-024 requires carrying the NLI score forward for fusion.

**Key crt-023 ADR to respect**: ADR-002 (entry #2701) states "NLI Entailment Score Replaces
rerank_score for Search Re-ranking Sort." crt-024 does not contradict this; it makes the
replacement complete and principled by eliminating the co-access re-sort step.

### col-023 (W1-5) Interaction

col-023 generalized the observation pipeline (HookType → ObservationEvent, domain pack registry).
It does not touch the search ranking pipeline. There are no structural interactions with crt-024;
the two features are independent.

### GH #329 Context

GH #329 was described in the product vision as the "co-access override fix." Based on the product
vision description, it was a targeted patch to prevent co-access from overriding NLI scores — a
symptom fix. crt-024 subsumed this by addressing the formula structure. The co-access re-sort step
(Step 8) is the mechanism being replaced; GH #329 likely constrained Step 8's override range
without fully eliminating it. The codebase shows no specific comment referencing #329 in the
search pipeline, suggesting it may have been planned but not yet merged, or its effect was limited.

### Unimatrix Knowledge Base Findings

- Entry #2701 (ADR-002 crt-023): NLI entailment replaces `rerank_score` for sorting — establishes
  the precedent that NLI is the primary signal; WA-0 enforces this architecturally.
- Entry #703 (ADR-003 crt-013): Behavior-based status penalty tests — confirms the `status_penalty`
  multiplier pattern is established and tested; WA-0 retains it as multiplier on fused score.
- Entry #701 (ADR-001 crt-013): W_COAC deleted as dead weight — historical context; a co-access
  weight constant was removed once before. WA-0 reintroduces it as a config-driven weight, not a
  hardcoded constant, which avoids the same fate.
- Entry #2298: Config key semantic divergence pattern — must confirm weight defaults match vision.

---

## Proposed Approach

Replace Step 7 + Step 8 with a single Step 7: fused score computation.

**Target formula:**

```
fused_score = w_sim  * similarity_score                  // [0,1] — HNSW cosine (bi-encoder recall)
            + w_nli  * nli_entailment_score              // [0,1] — cross-encoder entailment (precision)
            + w_conf * confidence_score                  // [0,1] — Wilson score composite
            + w_coac * (raw_boost / MAX_CO_ACCESS_BOOST) // [0,1] — usage pattern, normalized
            + w_util * (utility_delta_norm)              // [0,1] — effectiveness classification, normalized
            + w_prov * (provenance_boost_norm)           // [0,1] — category provenance, normalized

final_score = fused_score * status_penalty               // topology multiplier (not a signal)
```

**Why all six signals are in the formula**: Every signal that currently influences ranking should
be a learnable dimension for W3-1. Signals left outside the formula as additive afterthoughts
cannot have their contribution learned or tuned. The formula is W3-1's feature vector interface —
adding a signal to WA-0 is adding a learnable dimension to W3-1.

**Why status_penalty stays as a multiplier**: It is a topology modifier (deprecated/superseded
entries), not a relevance signal. It uniformly scales all relevance signals down. Retaining it as
a multiplier is consistent with semantics established across crt-010, crt-013, crt-014.

**NLI absence handling**: When `nli_enabled = false` or model not ready, `w_nli` term is 0.0.
Remaining five weights are re-normalized by dividing each by `(1 - w_nli)`. This preserves
relative signal importance when NLI degrades.

**Default weights** — architect must reason about these principled signal roles, not copy placeholder
splits. These are W3-1's initialization point: if WA-0 ranks with NLI underweighted, W3-1 trains
on a world where NLI barely mattered. Suggested starting point for architect review:
- `w_sim` — bi-encoder recall anchor (already filtered candidates, so lower than naive intuition)
- `w_nli` — cross-encoder precision; the semantically richest signal; should be dominant
- `w_conf` — historical reliability; tiebreaker between semantically equivalent candidates
- `w_coac` — usage pattern; useful but a lagging signal
- `w_util` — effectiveness; meaningful signal but sparser data
- `w_prov` — category hint; weakest signal, smallest weight

Sum ≤ 1.0. Leave headroom (e.g., 0.05) for WA-2's phase boost term so operators need not
re-tune weights when WA-2 ships.

**Implementation location**: `unimatrix-server/src/services/search.rs` — replace Steps 7+8.
Config: `InferenceConfig` in `infra/config.rs`. No changes to engine crates, embed crate, or store.

**Data flow change**: `apply_nli_sort` currently returns `Vec<(EntryRecord, f64)>` after
discarding NLI scores. crt-024 needs the NLI score carried forward for fusion. Two options:
(A) return `Vec<(EntryRecord, f64, Option<NliScores>)>` from the NLI step, or (B) compute the
fused score inline within a single pass. Option B is simpler and preferred.

---

## Acceptance Criteria

- AC-01: `InferenceConfig` in `infra/config.rs` adds six f64 fields: `w_sim`, `w_nli`, `w_conf`,
  `w_coac`, `w_util`, `w_prov`, all with `#[serde(default)]`. Default values sum to ≤ 1.0 (with
  headroom reserved for WA-2 phase boost term).
- AC-02: `InferenceConfig::validate()` rejects configurations where the sum of all six weights > 1.0
  with a structured error naming the offending sum and all six fields. Validation runs at server startup.
- AC-03: Each weight must be in [0.0, 1.0] individually. `validate()` rejects any negative weight
  or any weight > 1.0 with a structured error naming the offending field.
- AC-04: The search pipeline applies a single unified scoring pass (no separate re-sort after Step 7).
  The pipeline steps after crt-024 are: HNSW → filters → [NLI scoring] → fused score computation →
  sort by fused_score desc → truncate → floors → audit.
- AC-05: Fused score formula contains all six signal terms: `fused = w_sim*sim + w_nli*nli + w_conf*conf + w_coac*coac_norm + w_util*util_norm + w_prov*prov_norm`,
  multiplied by `status_penalty`. All terms computed in a single pass. Result is in [0.0, 1.0]
  by construction when weights sum ≤ 1.0 and all inputs are in [0, 1].
- AC-06: When NLI is absent (nli_enabled=false or handle not ready), the `w_nli` term is 0.0 and
  the remaining five weights (`w_sim`, `w_conf`, `w_coac`, `w_util`, `w_prov`) are re-normalized
  by dividing each by `(w_sim + w_conf + w_coac + w_util + w_prov)` before fusion. The
  re-normalized score is still in [0.0, 1.0]. All six signal dimensions are present in the
  denominator to correctly preserve relative signal importance.
- AC-07: Co-access raw boost is normalized to [0, 1] before fusion: `coac_norm = raw_boost / MAX_CO_ACCESS_BOOST`.
  `MAX_CO_ACCESS_BOOST = 0.03` is referenced from `unimatrix_engine::coaccess` — no duplication of the constant.
- AC-08: The `ScoredEntry.final_score` field reflects the fused score formula (not the pre-crt-024
  `rerank_score` formula). All existing tests that assert specific `final_score` values are updated
  to use the new formula. No test is deleted; only expected values change.
- AC-09: Existing `status_penalty` behavior is preserved: `status_penalty` is applied as a
  multiplier on the fused score. The penalty constants (ORPHAN_PENALTY, CLEAN_REPLACEMENT_PENALTY,
  etc.) are unchanged.
- AC-10: `utility_delta` and `PROVENANCE_BOOST` are included in the fused formula as `w_util` and
  `w_prov` weighted terms, normalized to [0, 1]. They are not additive afterthoughts outside the
  formula. This makes them learnable dimensions for W3-1.
- AC-11: A regression test asserts that an entry with high NLI entailment (0.9) and zero co-access
  is ranked above an entry with low NLI entailment (0.3) and maximum co-access (raw_boost=0.03),
  given equal similarity and confidence, using the default weights. This test must pass with crt-024
  but fails with the pre-crt-024 pipeline (demonstrating the fix).
- AC-12: All weight config parameters are validated at startup via `InferenceConfig::validate()`
  with structured errors consistent with existing NLI parameter validation style.
- AC-13: When `w_sim + w_nli + w_conf + w_coac < 1.0` (sum < 1.0, valid), the formula is applied
  as-is. No re-normalization when sum < 1.0 — leaving headroom for WA-2 phase boost terms is
  explicitly valid.
- AC-14: The `briefing` service co-access boost path (uses `MAX_BRIEFING_CO_ACCESS_BOOST = 0.01`)
  is NOT changed by this feature. Only `SearchService` receives the fused formula. `BriefingService`
  remains on its existing pipeline.
- AC-15: `EvalServiceLayer` passes `InferenceConfig` (including fusion weights) from the profile
  TOML to `SearchService`. A profile TOML with `w_sim=1.0` and all other weights=0.0 must produce
  `final_score` values equal to `similarity_score * status_penalty` for all candidates.
- AC-16: D1–D4 eval harness run completed on the pre-crt024 snapshot
  (`/tmp/eval/pre-crt024-snap.db`, scenarios at `/tmp/eval/pre-crt024-scenarios.jsonl`) before
  merge. Human reviews the generated report. Zero-regression check reviewed; any soft-truth
  regressions caused by NLI-override corrections are identified as intentional and distinguished
  from true regressions.

---

## Constraints

1. **`InferenceConfig` is the only config change surface.** Weights live under `[inference]` in
   `config.toml`. No new config sections.
11. **Eval run must use the pre-crt024 snapshot.** The pre-implementation snapshot is at
    `/tmp/eval/pre-crt024-snap.db` with scenarios at `/tmp/eval/pre-crt024-scenarios.jsonl`.
    Post-implementation, run D1–D4 eval comparing `old-behavior.toml` (w_sim=0.85, w_nli=0.0,
    w_conf=0.15, all others 0.0) vs. `crt024-weights.toml` (new defaults). Do not use a live
    database for this eval run.
2. **No engine crate changes.** `unimatrix-engine/src/coaccess.rs` (compute_search_boost,
   MAX_CO_ACCESS_BOOST) is unchanged. The normalization step lives in `SearchService`, not the engine.
3. **No schema migration.** This is a runtime scoring formula change only — no DB schema involved.
4. **NLI absence must not break search.** The fused formula degrades cleanly to a three-signal
   version when `w_nli` is zero. Constraint from crt-023 (AC-14): NLI absence never errors callers.
5. **Status penalty semantics preserved.** All existing topology penalty constants (ORPHAN_PENALTY =
   0.75, CLEAN_REPLACEMENT_PENALTY = 0.40) are unchanged. They apply as multipliers on the fused score.
6. **Weight sum validation must use the same structured error pattern as crt-023's NLI config
   validation** (`InferenceConfig::validate()` with named field errors, not panics).
7. **`apply_nli_sort` may be removed or repurposed** — it is currently `pub(crate)` in search.rs
   and called only by `try_nli_rerank`. If the single-pass approach eliminates the need for it as a
   separate function, its test coverage must be migrated to the new single-pass test coverage.
8. **`rerank_score` function in `unimatrix-engine/src/confidence.rs` is NOT removed.** It is still
   used by the fallback path and by existing tests. It may be used internally within the fused formula
   implementation.
9. **Default weights must leave the formula sound under all config combinations.** If all four
   defaults are used and NLI is disabled, the re-normalized weights must produce a ranking consistent
   with pre-crt-024 behavior (sim dominant, confidence secondary). The architect must verify this
   numerically in the ADR.
10. **The `w_sim` default must be large enough that a high-similarity, low-confidence entry still
    outranks a low-similarity, high-confidence entry at default weights.** This preserves the
    user-visible semantic that topical match is the primary filter.

---

## Open Questions

1. **`apply_nli_sort` retention.** It is currently `pub(crate)` with direct unit tests (from crt-023).
   If the single-pass approach eliminates it as a standalone function, those tests need migration.
   Alternatively, it can remain as an internal helper called within the single-pass scorer.
   The architect should decide.

2. **Eval harness gate.** The product vision estimates WA-0 at "1-2 days" and does not mention a
   mandatory eval gate (unlike W1-4 which had AC-09). Given the formula-deterministic nature of
   WA-0 (no model involved), unit tests may be sufficient. Architect to confirm.

3. **Co-access data availability for fusion.** `compute_search_boost` is called via `spawn_blocking`
   in Step 8. In the fused formula, co-access values must be available before scoring. The single-pass
   scorer must receive boost_map as input before iterating candidates. Data-flow order must be preserved.

4. **Principled default weights.** The architect must determine default values for all six weights
   (`w_sim`, `w_nli`, `w_conf`, `w_coac`, `w_util`, `w_prov`) with reasoning for each. Key
   constraint: these are W3-1's initialization point. NLI should be weighted to reflect its role
   as the semantically richest precision signal — not underweighted relative to the simpler
   bi-encoder recall signal that already pre-filtered the candidate set.

---

## Tracking

https://github.com/dug-21/unimatrix/issues/335

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "ranking scoring pipeline inference weights similarity confidence"
  -- Key findings: Entry #2701 (ADR-002 crt-023) establishes NLI entailment as primary sort signal;
  Entry #701 confirms a prior w_coac constant was deleted (hardcoded constants fail, config-driven
  weights are correct approach); Entry #2298 warns of config key semantic divergence pattern;
  Entry #485 (ADR-005) confirms deprecated 0.7x and superseded 0.5x penalty multipliers pattern.
- Stored: entry #2964 "Signal fusion pattern: sequential sort passes cause NLI override by additive
  boosts" via /uni-store-pattern — generalizes the structural defect class (sequential re-sort
  overrides semantic signal) beyond this feature.
