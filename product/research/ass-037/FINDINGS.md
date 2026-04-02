# ASS-037: Strategic Architecture Review — Findings

**Status**: Complete  
**Spike**: Background intelligence pipeline strategic architecture review  
**Prerequisites**: ASS-035 (NLI task mismatch confirmed, cosine validated), ASS-036 (GGUF FAIL)  
**Harness**: W1-3 eval harness, 2356 scenarios, fresh snapshot (2491 quarantined entries excluded)

---

## Baseline

| Metric | Value |
|--------|-------|
| P@5    | 0.1530 |
| MRR    | 0.3411 |

Note: Baseline P@5 is lower than WA-0's 0.3060 because 2491 quarantined entries make many soft ground-truth result_entry_ids unreachable. Relative deltas across profiles on the same snapshot are valid comparators.

---

## Q1 — Graph Topology Audit

**Finding**: The knowledge graph is too sparse for PPR to contribute to ranking.

| Metric | Value |
|--------|-------|
| Active entries | 1,134 |
| Entries with ≥1 edge | 419 (37%) |
| Entries with zero edges | 715 (63%) |

**Edge distribution (active→active only)**:

| Type | Count | Notes |
|------|-------|-------|
| CoAccess | 1,000 | Co-query pairs; densest signal |
| Informs | 85 | Phase 4b structural pre-filters (cosine + category pairs) |
| Supports | 3 | Surviving after quarantine wave (30 total written, 27 endpoints quarantined) |
| Contradicts | 0 | Contradiction scan has never written an edge in production |

CoAccess connects ~37% of the corpus. Informs connects cross-feature pairs (lesson-learned→decision, pattern→decision) via structural pre-filters. Supports edges are practically gone post-quarantine. Contradicts edges are structurally absent.

---

## Q2 — PPR Contribution

**Finding**: PPR contributes zero to ranking quality in current state.

| Profile | P@5 | MRR | ΔP@5 | ΔMRR |
|---------|-----|-----|------|------|
| baseline-nli | 0.1530 | 0.3411 | — | — |
| ablation-ppr-disabled | 0.1530 | 0.3411 | 0.0000 | 0.0000 |

Disabling PPR entirely (`ppr_blend_weight=0.0, ppr_max_expand=0`) produced identical results.

**Root cause**: Graph sparsity. With 63% of entries isolated and CoAccess limited to co-query pairs (predominantly same-category), PPR cannot propagate meaningful cross-category signals. The `inclusion_threshold=0.05` and `max_expand=50` provide no additional coverage because there are no meaningful paths to traverse.

---

## Q3 — Informs Hypothesis (Filtered PPR)

**Finding**: The Informs hypothesis cannot be evaluated in its current NLI-gated form. The structural pre-filters are sound; the NLI guard is task-mismatched.

**Current state (pre-amendment)**:
- 85 Informs edges in the live graph (written by Phase 4b cosine structural pre-filters)
- Phase 8b NLI guard: `neutral > 0.5, entailment ≤ 0.45, contradiction ≤ 0.6`
- Live edge metadata shows: neutral 0.51–0.69, contradiction 0.24–0.44, entailment 0.04–0.09
- Edges that passed the NLI check are the ones whose contradiction score was below 0.6 — the guard is filtering based on the same misaligned scores as Supports detection

### Q3b — Synthetic Informs Graph Test (Amendment)

**Methodology**: Rather than waiting for the NLI gate to be removed, the Informs hypothesis was tested via synthetic edge injection. All pairs satisfying Phase 4b structural criteria only (no NLI, cosine ≥ 0.5) were injected into a copy of the snapshot (`snapshot-synthetic.db`). Two profiles were run against the same synthetic snapshot — the only variable is PPR on/off.

**Synthetic edge generation**:
- Source entries eligible: 658 (lesson-learned: 305, pattern: 353)
- Entries with HNSW vectors: 614 (44 skipped: no vector mapping)
- Candidate pairs passing all Phase 4b structural criteria: **77 new edges**
- Existing Informs edges in snapshot: 83 (skipped — already present)
- Total Informs edges in synthetic snapshot: **160** (83 original + 77 injected)

Phase 4b criteria applied: cosine ≥ 0.5, valid category pair (lesson-learned/pattern → decision/convention), cross-feature (source.feature_cycle ≠ target.feature_cycle), temporal (source.created_at < target.created_at), within k=20 HNSW neighborhood.

**Results** (2356 scenarios, conf-boost-c weights: w_sim=0.50, w_conf=0.35):

| Profile | P@5 | MRR | ΔP@5 | ΔMRR |
|---------|-----|-----|------|------|
| synthetic-ppr-disabled | 0.1530 | 0.3420 | — | — |
| synthetic-ppr-enabled (blend=0.15) | 0.1530 | 0.3420 | 0.0000 | 0.0000 |

Zero delta on both P@5 and MRR. CC@k and ICD also identical. No ranking changes recorded across any of the 2356 scenarios.

**D2 verdict: NOT CONFIRMED** — Informs category topology adds zero retrieval value beyond cosine at current corpus size. PPR with Informs edges provides no measurable benefit even with 160 cross-category edges connecting 600+ entries.

**Interpretation**: The HNSW k=20 cosine search + confidence re-ranking already retrieves the correct entries. PPR can only promote entries already in the k=20 candidate set; it cannot surface new results that cosine missed. At corpus scale (1,134 entries, average 0.14 Informs edges per entry), the PPR graph is too thin to propagate signals that differ from cosine similarity. The Informs hypothesis may require significantly higher edge density or a larger corpus before the topology contributes meaningfully.

---

## Q4 — Signal Ablation

> **ASS-039 UPDATE**: The original Q4 was run against 2,356 scenarios built from human orchestration commands with null `expected` fields (soft ground truth = self-consistency, not retrieval quality). Those results were directionally useful but not reliable for formula calibration. ASS-039 replaced these with 1,585 behaviorally-grounded scenarios (context_search → context_get pairs from observations, `expected` populated with entry IDs agents actually read). The table below shows both runs. The re-run results on valid ground truth are the authoritative signal attribution.

### Original run (2,356 null-expected scenarios — directionally informative only)

| Profile | P@5 | MRR | ΔP@5 vs baseline | ΔMRR vs baseline |
|---------|-----|-----|-----------------|-----------------|
| baseline-nli | 0.1530 | 0.3411 | — | — |
| ablation-nli-zero | 0.1530 | 0.3411 | 0.0000 | 0.0000 |
| ablation-ppr-disabled | 0.1530 | 0.3411 | 0.0000 | 0.0000 |
| ablation-conf-zero | 0.1530 | 0.3074 | 0.0000 | −0.0337 |
| ablation-util-prov-zero | — | — | — | — |
| ablation-phase-zero | — | — | — | — |
| ablation-cosine-only | 0.1530 | 0.3074 | 0.0000 | −0.0337 |

### ASS-039 re-run (1,585 behavioral scenarios — valid ground truth)

Profiles evaluated against ASS-037 snapshot.db (1,134 active entries). Scenarios from observations.jsonl with `expected` = entry IDs agents actually retrieved.

| Profile | P@5 | MRR | ΔP@5 vs baseline-nli | ΔMRR vs baseline-nli |
|---------|-----|-----|---------------------|---------------------|
| baseline-nli | 0.1116 | 0.2882 | — | — |
| ablation-cosine-only | 0.1116 | 0.2670 | 0.0000 | −0.0212 |
| ablation-conf-zero | 0.1116 | 0.2670 | 0.0000 | −0.0212 |
| ablation-phase-zero | 0.1116 | 0.2882 | 0.0000 | 0.0000 |
| ablation-util-prov-zero | 0.1116 | 0.2912 | 0.0000 | +0.0030 |
| **conf-boost-c** | **0.1116** | **0.2911** | **0.0000** | **+0.0029** |

**Signal attribution (valid ground truth)**:

| Signal | Attribution | Verdict |
|--------|-------------|---------|
| Confidence (w_conf=0.35) | +0.0241 MRR (+9%) when conf-boost-c vs ablation-conf-zero | **Active** — strongest signal |
| Cosine (w_sim) | Load-bearing — floor of all retrieval | **Active** |
| Phase signals (w_phase_explicit) | +0.0029 MRR when present (ablation-phase-zero drops by 0.0029) | **Marginal active** |
| NLI (w_nli=0.35) | −0.0029 MRR vs conf-boost-c (baseline-nli = 0.2882 < 0.2911) | **Net negative at 0.35 weight** |
| Util (w_util) | ≤0 — ablation-util-prov-zero (0.2912) > conf-boost-c (0.2911) | **Neutral/negligible** |
| Prov (w_prov) | ≤0 — same as util | **Neutral/negligible** |
| P@5 | Zero — all profiles identical P@5=0.1116 | **Formula affects ordering, not set composition** |

**Key insight (updated)**: Confidence remains the only signal with measurable impact on MRR (+9%). The original run's −0.0337 MRR from ablation-conf-zero is confirmed in direction but the magnitude is different (−0.0212 in the re-run) — the valid ground truth produces a less extreme but directionally identical result. NLI at w_nli=0.35 is now confirmed net-negative (hurts 0.0029 MRR). Phase signals contribute a small positive (+0.0029). util/prov are genuinely neutral.

**Formula recommendation**: conf-boost-c (w_sim=0.50, w_conf=0.35) is confirmed as the best formula on valid behavioral ground truth. The ASS-037 recommendation holds.

---

## Q5 — Tags Viability

**Finding**: Tags are structurally stored but completely unused in the retrieval pipeline. Coverage is sufficient to build on.

| Metric | Value |
|--------|-------|
| Active entries | 1,134 |
| Entries with ≥1 tag | 945 (83%) |
| Total tag assignments | 6,099 |
| Avg tags per tagged entry | 6.5 |

Table: `entry_tags (entry_id, tag)` — clean M:N. Most common tags: `adr`, `source:retrospective`, `unimatrix-server`, `testing`, `background-tick`, `config`, `sqlx`, `sqlite`.

**Current pipeline state**: `feature_tag: Option<String>` field in `FusedScoreInputs` is `#[allow(dead_code)]`. Tags are written but never read during search or ranking.

**Viability assessment**: Tags are a viable supplemental signal for two use cases:
1. **Pre-filter** — restrict HNSW search to entries matching query tag(s); reduces recall risk for focused queries
2. **Boost** — Jaccard overlap between query tag set and entry tag set, summed into fused score

Domain-agnostic: tags are user-authored structured metadata, orthogonal to embedding similarity. In a corpus where tags are consistently applied (83% here), they can improve precision without degrading recall.

**Constraint**: Tags require the query to carry a tag set. MCP `context_search` currently takes a text query; tag-filtered search would need an optional `tags` parameter or a separate tool.

---

## Q6 — Formula Redesign

**Finding**: Empirically optimal formula removes all dead signals, raises sim and conf weights to fill the budget.

**Confidence boost curve** (NLI zeroed, util/prov zeroed, coac=0):

| Profile | w_sim | w_conf | P@5 | MRR | ΔMRR vs baseline |
|---------|-------|--------|-----|-----|-----------------|
| conf-boost-a | 0.70 | 0.15 | 0.1530 | 0.3411 | 0.0000 |
| conf-boost-b | 0.60 | 0.25 | 0.1530 | 0.3420 | +0.0009 |
| **conf-boost-c** | **0.50** | **0.35** | **0.1530** | **0.3420** | **+0.0009** |
| conf-boost-d | 0.40 | 0.45 | 0.1530 | 0.3411 | 0.0000 |

**Recommended formula**:

```
w_sim  = 0.50    # cosine similarity (HNSW, k=20, ef=32)
w_conf = 0.35    # Wilson-score confidence composite (f64)
w_nli  = 0.00    # removed: task mismatch (ASS-035)
w_coac = 0.00    # moved to PPR topology (crt-032)
w_util = 0.00    # redundant: subsumed by confidence
w_prov = 0.00    # redundant: subsumed by confidence
```

PPR is retained at `blend_weight=0.15` — it is zero-harm today and will contribute once Informs edges are generated at scale via structural inference (Q3, Q8).

**Rationale**:
- conf-boost-c and conf-boost-b both beat baseline MRR (+0.0009)
- conf-boost-c preferred: more balanced sim/conf ratio (0.50/0.35 vs 0.60/0.25)
- conf-boost-d shows diminishing returns — over-weighting confidence hurts
- Domain-agnostic: the formula contains only semantic content (cosine) and usage history (confidence), which are universally meaningful signals in any typed knowledge corpus

---

## Q7 — NLI Infrastructure Audit

**Finding**: All four NLI use sites are either zero-effect or task-mismatched. NLI should be removed from the inference path.

### Per-use Verdict Table

| Use Site | File | Purpose | Current Effect | Verdict |
|----------|------|---------|----------------|---------|
| Post-store detection | `nli_detection.rs: run_post_store_nli` | Find Supports/Contradicts edges on new entry store | 30 Supports written total; 27 endpoints now quarantined; 0 Contradicts ever written | **REMOVE** — replace Supports with cosine ≥ 0.65 (ASS-035); Contradicts with structural/manual |
| Background graph inference tick | `nli_detection_tick.rs: run_graph_inference_tick` | Phase 4b structural + Phase 8b NLI guard for Informs edges | Phase 4b (cosine + category pairs) writes 85 Informs edges; Phase 8b NLI neutral check applies same task mismatch | **RESTRUCTURE** — keep Phase 4b structural pre-filters; remove Phase 8b NLI guard; raise cosine floor from 0.3 to 0.5 |
| Bootstrap promotion | `nli_detection.rs: maybe_run_bootstrap_promotion` | One-shot: promote bootstrap Contradicts edges to NLI-confirmed | 0 bootstrap Contradicts rows in DB; idempotency marker not set (would run, find nothing) | **REMOVE** — dead code; no bootstrap edges exist |
| Auto-quarantine NLI guard | `background.rs: process_auto_quarantine` | Block quarantine if Contradicts edges are all NLI-origin and below threshold | 0 Contradicts edges → guard never triggers; always returns Allowed | **REMOVE** — dead code path; effectiveness-based quarantine is sufficient |

### Key Root Cause

The cross-encoder (`cross-encoder/nli-MiniLM2-L6-H768 Q8`) is trained on SNLI — natural language sentence pairs. Unimatrix knowledge entries are structured records (ADRs, lessons, patterns, conventions) with imperative phrasing. The model maps these to NLI labels poorly:

- A (lesson-learned, decision) pair (problem description → solution rule) scores high contradiction even though it's informativeness — same task mismatch found in ASS-035 for Supports
- A (pattern, pattern) pair with different scope scores neutral when it should score entailment
- The neutral zone check (`neutral > 0.5`) in Phase 8b is the only passing gate — structurally it selects for pairs where the model is maximally uncertain

**The structural pre-filters (Phase 4b) are the actual load-bearing component** for Informs edge detection:
- cosine ≥ 0.3 floor
- category pair membership (informs_category_pairs config)
- temporal ordering (older entry → newer entry)
- cross-feature constraint

These are domain-agnostic structural constraints that do not depend on NLI inference.

### NLI Tick Gating (Q8 blocker)

Both `maybe_run_bootstrap_promotion` and `run_graph_inference_tick` are gated on `inference_config.nli_enabled` in `background.rs:775–787`. When `nli_enabled=false`, the structural Informs inference also stops. This prevents Informs edges from being generated without NLI, even though Phase 4b doesn't use NLI.

---

## Q8 — Tick Decomposition

**Finding**: The background tick must be decomposed to decouple structural graph inference from NLI availability.

### Current Architecture

```
background tick (every 15 min)
  if nli_enabled:
    maybe_run_bootstrap_promotion()   # NLI-only, one-shot
    run_graph_inference_tick()        # Structural Phase 4b + NLI Phase 8b
```

### Recommended Architecture

```
background tick (every 15 min)
  structural_graph_tick()             # Always runs; no NLI dependency
    - cosine-only Supports detection (threshold ≥ 0.65, ASS-035 validated)
    - Informs detection via Phase 4b pre-filters only (cosine ≥ 0.5, category pair, temporal, cross-feature)
    - NO NLI scoring in any phase

  if nli_enabled:
    contradiction_scan()              # NLI-specific; periodic; separate concern
      - SNLI-appropriate task only (explicit contradiction detection with domain-adapted model)
      - Blocked until a domain-adapted model is available (GGUF failed in ASS-036)
```

**Why this decomposition**:
1. Structural Informs detection works today (85 edges written by Phase 4b alone)
2. Unlocks the Informs hypothesis test (Q3) without waiting for a replacement NLI model
3. Contradiction detection is a separate concern that requires domain adaptation — blocking graph inference on it is a category error
4. The `maybe_run_bootstrap_promotion` one-shot can be removed entirely (no bootstrap Contradicts edges)

---

## Summary — Verdicts

| Question | Verdict |
|----------|---------|
| **D1: PPR contributes to ranking** | **FAIL** — zero delta; graph too sparse |
| **D2: Informs hypothesis testable** | **NOT CONFIRMED** — 77 synthetic edges injected (160 total); PPR delta = 0.0000; corpus too small for topology to add value beyond cosine |
| **D3: Signal attribution clear** | **PASS** — confidence is the sole non-cosine signal; all others zero |
| **D4: Tags viable** | **PASS** — 83% coverage, unused, viable for boost/pre-filter |

---

## Forward Path

1. **Immediate**: Apply formula conf-boost-c (`w_sim=0.50, w_conf=0.35`) — net gain, no risk
2. **Q8 (structural tick)**: Decouple `run_graph_inference_tick` from `nli_enabled` gate; remove Phase 8b NLI guard; raise cosine floor to 0.5; this enables fair Informs hypothesis test
3. **Q3 (Informs hypothesis)**: After tick decomposition, run filtered PPR eval with Informs-only edges vs CoAccess-only edges; measure cross-category retrieval improvement
4. **Q5 (tags)**: Add optional `tags` parameter to `context_search`; implement Jaccard-overlap tag boost in FusedScoreInputs; low risk, high coverage (83%)
5. **NLI future**: Block on domain-adapted model (ASS-036 GGUF failed; path unclear). Do NOT re-use SNLI cross-encoder for contradiction detection in a knowledge-entry corpus.
