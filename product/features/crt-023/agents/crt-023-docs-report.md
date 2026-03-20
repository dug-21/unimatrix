# crt-023 Documentation Agent Report

**Agent ID**: crt-023-docs
**Feature**: crt-023 — NLI + Cross-Encoder Re-ranking
**Issue**: #327
**Commit**: 9b121ca

---

## Artifacts Read

- `product/features/crt-023/SCOPE.md` — Goals, Acceptance Criteria, Resolved Decisions (D-01 through D-04)
- `product/features/crt-023/specification/SPECIFICATION.md` — FR-07, FR-11, FR-14, FR-15, NFR-08, NFR-09, AC-16; config field table in FR-11 used as source of truth for field names, types, defaults, and validated ranges
- `README.md` — current state before edits

---

## Sections Modified

### 1. Core Capabilities — "Semantic Search with Confidence-Aware Ranking" (renamed and updated)

Renamed to "Semantic Search with NLI Re-ranking". Updated to describe NLI entailment-based ranking as the primary path when the model is present, with graceful fallback to confidence-aware cosine ranking when absent or `nli_enabled = false`. Added `nli_top_k` default, rayon pool dispatch, and entailment-sort description. Traces to: Goals 3, FR-14, FR-15, AC-08.

### 2. Core Capabilities — "Contradiction Detection" (renamed and updated)

Renamed to "Contradiction Detection and NLI Edge Classification". Updated to describe post-store fire-and-forget NLI pipeline writing `Contradicts`/`Supports` edges to `GRAPH_EDGES` with `source='nli'`, replacing the lexical heuristic for new edge creation. Added circuit breaker (`max_contradicts_per_tick`) description. Traces to: Goals 4, FR-18, FR-19, AC-10, AC-13.

### 3. Tips for Maximum Value — two new items added (items 9 and 10)

- Item 9: NLI model must be downloaded separately via `unimatrix model-download --nli`; graceful degradation when absent. Traces to: AC-16, FR-07.
- Item 10: `nli_model_sha256` must be pinned in production; security motivation (model-poisoning). Traces to: NFR-09, AC-06.

### 4. Configuration — `[inference]` block extended

Updated `rayon_pool_size` comment to remove "future NLI". Added all ten NLI config fields with comments: `nli_enabled`, `nli_model_name`, `nli_model_sha256`, `nli_top_k`, `nli_post_store_k`, `nli_entailment_threshold`, `nli_contradiction_threshold`, `max_contradicts_per_tick`, `nli_auto_quarantine_threshold`. Defaults and ranges from FR-11 table. Security note on `nli_model_sha256`. Traces to: FR-11, AC-07, AC-17, D-03, D-04.

### 5. CLI Reference — `model-download` row updated

Extended to describe `--nli` flag (downloads NLI model, prints SHA-256 hash) and `--nli-model minilm2|deberta` flag (model variant selection). Traces to: AC-16, FR-07, D-03.

### 6. Security Model — new subsection "NLI Model Integrity"

Describes SHA-256 hash pinning for the NLI model file, `NliServiceHandle` transition to Failed on mismatch, fallback to cosine, and the model-poisoning threat. Directs operators to `unimatrix model-download --nli` to obtain the hash. Traces to: NFR-09, AC-06, FR-09.

---

## Not Modified

- MCP Tool Reference — no new MCP tools; `context_search` response schema unchanged (Non-Goals, FR-17)
- Skills Reference — no new skills
- Knowledge Categories — no new categories
- Architecture Overview — no new crates, no schema migration (NFR-11), no storage change
- Getting Started — no new installation steps required beyond model-download (covered in Tips)

---

## Fallback Chain

SPECIFICATION.md was present and used as the primary source. SCOPE.md used for cross-reference on Goals and Acceptance Criteria. No source code read.
