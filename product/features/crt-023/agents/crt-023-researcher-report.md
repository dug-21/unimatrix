# crt-023 Researcher Report — W1-4: NLI + Cross-Encoder Re-ranking

**Agent ID**: crt-023-researcher
**Date**: 2026-03-20

---

## Summary

SCOPE.md written to `product/features/crt-023/SCOPE.md`. The problem space is well-defined by the
product vision W1-4 section and the three completed prerequisites (crt-021, crt-022, nan-007). All
key technical constraints are confirmed from codebase inspection.

---

## Key Findings

### Prerequisites Are Fully Satisfied

- **crt-021 (W1-1)**: `GRAPH_EDGES` table live at schema version 13. `bootstrap_only=1` Contradicts
  edge column exists. `metadata TEXT DEFAULT NULL` column exists for W3-1 GNN features. The
  `AnalyticsWrite::GraphEdge` variant already carries the shed-policy note mandating direct
  `write_pool_server()` for NLI-confirmed edge writes.
- **crt-022 (W1-2)**: `RayonPool` fully implemented with `spawn()` and `spawn_with_timeout()`. Pool
  is `Arc<RayonPool>` on `AppState` and all service structs. Panic containment confirmed via oneshot
  channel drop. Pool naming is `ml_inference_pool`. Single pool shared by all ONNX inference.
- **nan-007 (W1-3)**: All four CLI commands (`snapshot`, `eval scenarios`, `eval run`, `eval report`)
  are shipped and tested. `EvalServiceLayer::from_profile()` has an explicit NLI stub comment:
  "InferenceConfig is stub-only for nan-007 (no nli_model field yet). When W1-4 adds nli_model,
  validation goes here." This is the exact hook crt-023 fills in.

### NLI Provider Is Architecturally Different from EmbeddingProvider

`EmbeddingProvider` takes single texts and returns `Vec<f32>` embeddings. Cross-encoders take
`(query, passage)` pairs and return `[entailment, neutral, contradiction]` logits/softmax — a
completely different output shape. A new `CrossEncoderProvider` trait is required; `NliProvider`
cannot reuse `EmbeddingProvider`. The `Mutex<Session>` + lock-free `Tokenizer` pattern (ADR-001)
applies directly.

### NLI-Confirmed Edges Must Bypass Analytics Queue (SR-02 Already Documented)

The `AnalyticsWrite::GraphEdge` doc comment in `analytics.rs` (line 166–168) explicitly states:
"W1-2 NLI confirmed edge writes MUST NOT use this variant — use direct write_pool path instead."
This constraint is baked in — the implementer just follows it.

### `EmbedServiceHandle` Is the Exact Pattern for `NliServiceHandle`

The lazy-loading state machine (Loading → Ready | Failed → Retrying with exponential backoff) in
`embed_handle.rs` is clean and well-tested. `NliServiceHandle` mirrors it verbatim, adding SHA-256
hash verification between model file load and session construction.

### SearchService Already Has the Rayon Pool and the Right Integration Point

`SearchService` holds `rayon_pool: Arc<RayonPool>` and performs the re-rank sort at Step 7 of the
pipeline. NLI re-ranking slots in between the HNSW expansion step and the existing `rerank_score`
sort. Graceful degradation (NLI not ready → fall through to existing path) is a single `if let Ok()`
branch on `nli_handle.get_provider()`.

### Bootstrap Edge Rows Are Likely Zero

crt-021 open question OQ-1 (shadow_evaluations → entry_id pair mapping) was flagged as unresolved
before AC-08. The implementation shipped with an empty bootstrap path for Contradicts edges from
shadow_evaluations. The promotion task in crt-023 must be designed to handle zero bootstrap rows
gracefully (no-op, no error).

### Eval Gate Requires Human Knowledge Base with Query History

The gate condition (AC-09) requires `unimatrix eval scenarios` to mine real query history from a
snapshot. A fresh installation has no query history. This is an open question (#4 in SCOPE.md) that
the architect or human must resolve before the spec writer locks the gate condition wording.

---

## Proposed Scope Boundaries

**In scope**: `NliProvider` + `CrossEncoderProvider` trait (unimatrix-embed), `NliServiceHandle`
(unimatrix-server/infra), search re-ranking in `SearchService`, post-store NLI detection in
`StoreService`, bootstrap edge promotion (one-shot background task), circuit breaker, SHA-256 hash
pinning, `InferenceConfig` extension, `model-download --nli` CLI extension, `EvalServiceLayer` NLI
stub fill-in, eval gate execution (AC-09).

**Out of scope**: GGUF pool, GNN training, automated CI gate, deberta model as primary (eval
validates), full `scan_contradictions` background scan NLI upgrade, new schema migration.

---

## Open Questions for Human

1. **Eval scenario set for gate (OQ-4)**: How is the gate satisfied for deployments with no query
   history? Require hand-authored scenarios, waive gate for new deployments, or define minimum count?

2. **NLI score blend vs replace in rerank formula (OQ-6)**: Replace `rerank_score` entirely with
   NLI entailment score, or blend? The product vision implies replacement ("re-rank by NLI score")
   but a blend may be more robust. Architect will decide and document as ADR.

3. **Post-store neighbor count (OQ-2)**: Use same `nli_top_k=20` for post-store detection, or a
   separate config `nli_post_store_k`?

4. **Deberta ONNX availability (OQ-3)**: Should the eval harness compare three profiles (baseline,
   MiniLM2, deberta) in this feature, or defer deberta to a follow-on?

5. **Pool floor with NLI enabled (OQ-1)**: Should `InferenceConfig` increase the pool floor (e.g.,
   to 6) when `nli_enabled = true`, or leave the existing 4–8 default?

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "NLI cross-encoder rayon inference" -- ADR-001 (entry #67)
  confirms `Mutex<Session>` pattern. ADR-002 (entry #68) confirms raw ort + tokenizers approach.
- Queried: "bootstrap contradiction edges NLI confirmation" -- crt-003 outcome entry confirms cosine
  heuristic shipped. No existing NLI patterns.
- Queried: "search reranking eval harness gate P@K MRR" -- ADR-003 crt-013 (entry #703) confirms
  behavior-based ranking assertions; relevant for test design.
- Queried: "circuit breaker auto-quarantine NLI" -- entry #1542 pattern (hold counter on error) and
  entry #1544 ADR-002 crt-018b directly apply to NLI circuit breaker design.
- Stored: nothing novel to store -- all findings confirm already-stored patterns or are crt-023-specific
  scope details. Post-architect ADR decisions (NLI blend formula, pool sizing with NLI) should be
  stored via `/uni-store-adr` during Session 2.
