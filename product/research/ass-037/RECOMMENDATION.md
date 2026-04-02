# ASS-037: Strategic Architecture Recommendation

**Status**: Complete  
**Based on**: ASS-035, ASS-036, ASS-037/FINDINGS.md

---

## Recommended Formula

```toml
[inference]
w_sim            = 0.50    # cosine similarity
w_conf           = 0.35    # Wilson-score confidence composite
w_nli            = 0.00    # removed: task mismatch confirmed (ASS-035, ASS-037/Q7)
w_coac           = 0.00    # already zeroed in crt-032; PPR subsumes co-access via graph topology
w_util           = 0.00    # redundant: subsumed by confidence composite
w_prov           = 0.00    # redundant: subsumed by confidence composite
ppr_blend_weight = 0.00    # inactive: zero contribution at current corpus scale (Q3b)
ppr_max_expand   = 0       # inactive
```

**Empirical basis**: conf-boost-c + Q3b synthetic test + ASS-039 re-run on valid ground truth.

Original (null-expected scenarios):
- conf-boost-c: MRR 0.3420 vs baseline 0.3411 (+0.0009); P@5 unchanged

ASS-039 re-run (1,585 behavioral scenarios, expected.entry_ids populated from observations):
- conf-boost-c: MRR 0.2911, baseline-nli: MRR 0.2882 (+0.0029); P@5 unchanged (0.1116)
- Confidence is confirmed as the only signal with measurable MRR impact (+9% when present)
- Formula recommendation is **unchanged** — conf-boost-c remains the recommended config

Note: ASS-037's original 2,356 scenarios were built from human orchestration commands with null `expected` fields, producing self-consistency metrics rather than retrieval quality metrics. Those results were directionally consistent with the re-run but are superseded by the behavioral ground truth. All future eval runs should use the scenarios from `product/research/ass-039/harness/scenarios.jsonl`.

Note on PPR: setting `ppr_blend_weight=0.0` is a runtime calibration, not an architectural removal. The PPR infrastructure and Informs edge generation remain worth maintaining for scale readiness. The test showed no measurable benefit at current scale — it did not disprove PPR's value at larger corpus sizes.

---

## Recommended Architecture Changes

### 1. Decouple structural graph inference from NLI (Q8 / High Priority)

**Current**: Both `run_graph_inference_tick` and `maybe_run_bootstrap_promotion` are gated on `inference_config.nli_enabled`. Structural Informs detection (Phase 4b) doesn't use NLI — it uses cosine + category pairs + temporal ordering + cross-feature constraints. This coupling prevents Informs edge generation when NLI is disabled.

**Change**:
- Remove `nli_enabled` gate from `run_graph_inference_tick`
- Remove Phase 8b NLI neutral check from Informs path entirely
- Raise `nli_informs_cosine_floor` from 0.3 → 0.5 (compensates for removed NLI guard; tightens pre-filter)
- Remove `maybe_run_bootstrap_promotion` (dead code: zero bootstrap Contradicts edges in production)
- Separate contradiction scanning into its own periodic tick, gated on NLI availability

**Why**: Structural pre-filters are the load-bearing Informs detection mechanism (85 edges written by Phase 4b alone). The NLI guard applies task-mismatched scores to a structurally sound filter — it can only reduce quality, never improve it given the current model.

### 2. Remove post-store NLI Supports detection; replace with cosine (Q7 / Medium Priority)

**Current**: `run_post_store_nli` fires after each `context_store`, finds neighbors via HNSW, scores with cross-encoder, writes Supports/Contradicts edges. Result: 30 Supports edges written total (27 endpoints quarantined), 0 Contradicts edges ever written.

**Change**:
- Replace NLI path with cosine threshold detection: neighbors with cosine ≥ 0.65 → write Supports edge (threshold validated in ASS-035: 6/8 true pairs, 0/10 false positives)
- This removes the ONNX cross-encoder inference dependency from the hot post-store path
- Contradicts edges: defer to the dedicated contradiction tick (once a domain-adapted model exists)

### 3. Remove auto-quarantine NLI guard (Q7 / Low Priority)

**Current**: `nli_auto_quarantine_allowed` checks whether all Contradicts edges are NLI-origin and above threshold before allowing quarantine. With 0 Contradicts edges in production, this guard always returns `Allowed` — it is dead code.

**Change**: Remove the NLI check from `process_auto_quarantine`. Effectiveness-based quarantine (behavioral signals) is sufficient.

### 4. Activate tags as a supplemental signal (Q5 / Future)

**Current**: `entry_tags` table has 83% coverage (945/1134 active entries), 6,099 total assignments. Tags are written on store but never read during search. `feature_tag: Option<String>` in `FusedScoreInputs` is `#[allow(dead_code)]`.

**Change** (future feature, not this spike):
- Add optional `tags: Vec<String>` parameter to `context_search` MCP tool
- Implement Jaccard overlap scoring between query tags and entry tags in `FusedScoreInputs`
- Pre-filter: if query tags provided, restrict HNSW candidates to entries with ≥1 matching tag before cosine ranking

### 5. Revisit PPR at higher corpus scale (Q3 / Deferred)

Q3b synthetic test showed zero delta at 1,134 entries / 160 Informs edges (0.14 edges/entry). PPR propagation is density-dependent; the test conditions were a lower bound.

A reasonable retest trigger is either:
- Corpus grows to ≥5,000 active entries, OR
- Organic Informs edge density reaches ≥1.0 edges/entry (via structural inference running normally after Q8 tick decomposition)

At that point, re-run the Q3b method (probe tool + snapshot copy, same two-profile comparison). The probe tool at `product/research/ass-037/tools/informs_probe/` is reusable as-is.

---

## What to Deprioritize

| Item | Reason |
|------|--------|
| NLI replacement model | No domain-adapted model available; GGUF failed (ASS-036 44% accuracy, 70% FP, 24s/pair); block on future research |
| Contradicts edge generation | Requires reliable contradiction detection; SNLI cross-encoder is not it; defer |
| PPR at current scale | Zero contribution at 1,134 entries / 160 Informs edges; set blend_weight=0.0 now; revisit at corpus ≥5K or density ≥1.0 edges/entry |
| w_util, w_prov reintroduction | Both signals are subsumed by the confidence composite; no additive value; keep at 0.00 |

---

## Evidence Summary

| Finding | Evidence |
|---------|---------|
| NLI is dead | w_nli=0 → identical P@5/MRR (2356-scenario eval) |
| PPR contributes zero at current scale | PPR disabled → identical (Q2); 160 synthetic Informs edges → still zero delta (Q3b); insufficient corpus density |
| Confidence is the sole non-cosine signal | w_conf=0 → −0.0337 MRR; P@5 unchanged |
| conf-boost-c is optimal formula | MRR 0.3420 vs baseline 0.3411; no P@5 regression |
| Structural Informs pre-filters work | 85 edges written by Phase 4b alone; no NLI needed |
| Contradicts scan has never fired | 0 Contradicts edges in production across all observed history |
| Tags unused but available | 83% coverage, 6,099 assignments, dead code in search pipeline |
| Informs topology hypothesis not confirmed at current scale | 160 Informs edges × 1,134 entries = 0.14 edges/entry; insufficient for PPR propagation |
