# Gate 3a Report: crt-023

> Gate: 3a (Component Design Review)
> Date: 2026-03-20
> Result: PASS (re-check after rework)

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 9 components map to architecture decomposition; interfaces and file assignments match |
| Specification coverage (25 ACs) | WARN | FR-11 lists 10 fields; AC-07 says "nine" — minor internal spec inconsistency; pseudocode correctly implements 10; no coverage gap |
| Risk coverage (22 risks) | PASS | All 22 risks mapped in test-plan/OVERVIEW.md; all 6 non-negotiable tests present |
| Interface consistency | PASS | Shared types in OVERVIEW.md consistent with per-component usage |
| W1-2 compliance (bootstrap promotion) | PASS | bootstrap-promotion.md batches all pairs and dispatches via single rayon_pool.spawn(); no inline async inference |
| Knowledge stewardship — pseudocode agent | PASS | Queried entries present; "nothing novel" with reason noted |
| Knowledge stewardship — architect agent | WARN | Section added on re-check; ADR-007 has no Unimatrix entry ID yet — coordinator to store via `/uni-store-adr` |

---

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: PASS

**Evidence**: Each of the 9 components (NliProvider, NliServiceHandle, Search Re-ranking, Post-Store Detection, Bootstrap Promotion, Config Extension, Eval Integration, Model Download CLI, Auto-Quarantine Threshold) maps to the architecture's component breakdown. File assignments match:

- `cross_encoder.rs` (Component 1) → architecture Component 1
- `nli_handle.rs` (Component 2) → architecture Component 2
- `search.rs` (Component 3) → architecture Component 3
- `store_ops.rs` (Component 4) → architecture Component 4
- `nli_detection.rs` (Components 4–5) → architecture Components 4–5
- `config.rs` (Component 6) → architecture Component 6
- `eval.rs` (Component 7) → architecture Component 7
- `main.rs` CLI (Component 8) → architecture Component 8
- `background.rs` (implicit Component 9) → architecture auto-quarantine section

ADR decisions are reflected consistently across all pseudocode files:
- ADR-001 (single Mutex<Session> + pool floor 6): enforced in nli-provider.md, nli-service-handle.md, config-extension.md
- ADR-002 (pure entailment replacement): enforced in search-reranking.md
- ADR-003 (model config + hash pinning): enforced in nli-service-handle.md
- ADR-004 (embedding move after HNSW insert): enforced in post-store-detection.md with explicit "INVARIANT" comment requirement
- ADR-005 (COUNTERS idempotency): enforced in bootstrap-promotion.md
- ADR-006 (skip-not-fail for eval): enforced in eval-integration.md
- ADR-007 (cross-field threshold invariant): enforced in config-extension.md

Pool floor logic: `config.inference.rayon_pool_size = config.inference.rayon_pool_size.max(6).min(8)` is in config-extension.md pseudocode, applied at startup after validate(). Matches architecture spec exactly.

Minor note: The architecture Integration Points table says "9 NLI fields" in two places (`config.rs` row and `InferenceConfig new fields` row) but Component 6 lists 10 fields (including `nli_auto_quarantine_threshold`). The pseudocode correctly implements 10 fields. This is a stale count in the architecture table, not a gap in the pseudocode.

---

### Check 2: Specification Coverage (All 25 ACs)

**Status**: WARN

**Evidence**: All 25 acceptance criteria (AC-01 through AC-25) are addressed in the pseudocode. Tracing each:

| AC | Pseudocode File | Coverage |
|----|----------------|---------|
| AC-01 | nli-provider.md: score_pair + sum invariant | COVERED |
| AC-02 | nli-provider.md: Mutex<Session> + concurrent test | COVERED |
| AC-03 | nli-provider.md: truncate_input + oversized test | COVERED |
| AC-04 | nli-provider.md: NliModel methods | COVERED |
| AC-05 | nli-service-handle.md: Loading/Failed state machine | COVERED |
| AC-06 | nli-service-handle.md: hash mismatch → Failed + error log | COVERED |
| AC-07 | config-extension.md: all 10 fields + defaults | COVERED |
| AC-08 | search-reranking.md: NLI pipeline + fallback | COVERED |
| AC-09 | eval-integration.md: gate execution via run_eval_async | COVERED |
| AC-10 | post-store-detection.md: write_nli_edge with correct fields | COVERED |
| AC-11 | post-store-detection.md: format_nli_metadata JSON | COVERED |
| AC-12 | bootstrap-promotion.md: zero-row + promotion + deletion | COVERED |
| AC-13 | post-store-detection.md: circuit breaker (all edge types) | COVERED |
| AC-14 | nli-service-handle.md + OVERVIEW.md: graceful degradation | COVERED |
| AC-15 | post-store-detection.md: RayonError::Cancelled handled | COVERED |
| AC-16 | model-download-cli.md: --nli flag + SHA-256 stdout | COVERED |
| AC-17 | config-extension.md: validate() range checks + cross-field | COVERED |
| AC-18 | eval-integration.md: W1-4 stub fill + two-profile run | COVERED |
| AC-19 | config-extension.md + search-reranking.md: distinct top_k | COVERED |
| AC-20 | search-reranking.md: rerank_score not called on NLI path | COVERED |
| AC-21 | nli-provider.md: from_config_name string resolution | COVERED |
| AC-22 | eval-integration.md: gate waiver documentation | COVERED |
| AC-23 | post-store-detection.md: per-call semantic with comment | COVERED |
| AC-24 | bootstrap-promotion.md: COUNTERS marker idempotency | COVERED |
| AC-25 | auto-quarantine-threshold.md: higher threshold for NLI-only edges | COVERED |

**Issue (WARN)**: Specification AC-07 says "all nine NLI fields" but FR-11 defines a table of 10 fields (including `nli_auto_quarantine_threshold`). The pseudocode correctly implements 10. The WARN is for the spec inconsistency — it could confuse an implementer reading AC-07 in isolation. The pseudocode resolves correctly by following FR-11 (10 fields). No pseudocode fix needed; the spec's internal count error is minor.

All functional requirements FR-01 through FR-29 are addressed:
- FR-01–FR-07: nli-provider.md covers CrossEncoderProvider trait, NliScores, NliProvider, NliModel, config string selection, truncation, download CLI
- FR-08–FR-13: nli-service-handle.md covers state machine, hash verification, poison detection, config validation, AppState wiring
- FR-14–FR-17: search-reranking.md covers NLI active pipeline, fallback, rayon dispatch with timeout, response schema unchanged
- FR-18–FR-22b: post-store-detection.md + auto-quarantine-threshold.md cover fire-and-forget, edge writes, embedding handoff, panic containment, per-call cap, higher auto-quarantine threshold
- FR-23–FR-25: bootstrap-promotion.md covers batch rayon dispatch (W1-2), idempotency, NLI-only deferral
- FR-26–FR-29: eval-integration.md covers W1-4 stub fill, two-profile comparison, gate pass criteria, waiver condition

NFR coverage:
- NFR-01 (latency): MCP_HANDLER_TIMEOUT via spawn_with_timeout in search-reranking.md
- NFR-02 (post-store latency): fire-and-forget in post-store-detection.md
- NFR-03 (availability): graceful degradation in all components
- NFR-04 (pool contention): pool floor in config-extension.md
- NFR-05 (memory): single Mutex<Session> in nli-provider.md
- NFR-06 (panic containment): RayonError::Cancelled handling in post-store and search
- NFR-07 (config-driven): all thresholds are config fields
- NFR-08 (security/truncation): truncate_input in nli-provider.md (enforced inside NliProvider)
- NFR-09 (model integrity): SHA-256 in nli-service-handle.md
- NFR-10 (ort version): noted in nli-provider.md
- NFR-11 (no schema migration): noted in OVERVIEW.md
- NFR-12 (eval portability): skip-not-fail in eval-integration.md

No scope additions detected. No pseudocode implements unrequested features.

---

### Check 3: Risk Coverage (All 22 Risks)

**Status**: PASS

**Evidence**: The test-plan/OVERVIEW.md Risk-to-Test Mapping table covers all 22 risks (R-01 through R-22). All 6 non-negotiable tests from the Risk Strategy are present:

| Non-Negotiable | Test Function | Location |
|----------------|--------------|---------|
| R-01 (pool saturation) | `test_concurrent_nli_search_pool_saturation` | nli-service-handle.md |
| R-03 (stable sort) | `test_nli_sort_stable_identical_scores` | search-reranking.md |
| R-05 (hash mismatch) | `test_hash_mismatch_transitions_to_failed` | nli-service-handle.md |
| R-09 (circuit breaker all types) | `test_circuit_breaker_counts_all_edge_types` | post-store-detection.md |
| R-10 (cascade no auto-quarantine) | `test_miscalibration_cascade_no_auto_quarantine` | auto-quarantine-threshold.md |
| R-13 (mutex poison at get_provider) | `test_mutex_poison_detected_at_get_provider` | nli-service-handle.md |

Per-risk coverage confirmation:
- R-01 (Critical): 3 scenarios in nli-service-handle.md + concurrent search tests
- R-02 (High): 2 scenarios in nli-service-handle.md (pool floor raised/not raised)
- R-03 (Critical): 2 scenarios in search-reranking.md (stable sort, narrative > terse)
- R-04 (High): 3 scenarios in search-reranking.md (timeout, handle not failed, second call)
- R-05 (Critical): 4 scenarios in nli-service-handle.md (missing hash warn, mismatch failed, correct ready, wrong length)
- R-06 (High): 3 scenarios in nli-service-handle.md (truncated file, corrupt header, retry then failed)
- R-07 (High): 3 scenarios in post-store-detection.md (non-empty embedding reaches task, empty guard, W1-2 compliance)
- R-08 (Med): 2 scenarios in post-store-detection.md (HNSW fail store returns ok, zero neighbors clean exit)
- R-09 (Critical): 3 scenarios in post-store-detection.md (combined cap, mixed types, debug log)
- R-10 (Critical): 4 scenarios in auto-quarantine-threshold.md (cascade no quarantine, cap=1 sandbox, metadata read, mixed edges)
- R-11 (High): 4 scenarios in bootstrap-promotion.md (write error idempotent, marker check, synthetic rows, zero-row)
- R-12 (Med): 2 scenarios in bootstrap-promotion.md (no HNSW dependency via signature, cold index)
- R-13 (High): 3 scenarios in nli-service-handle.md (poison detected, retry after poison, exhaustion stays failed)
- R-14 (High): 3 scenarios in eval-integration.md (SKIPPED annotation, reason string, AC-01 independent)
- R-15 (High): 3 scenarios in config-extension.md (invalid name fails validate, valid names pass, from_config_name returns None)
- R-16 (High): 2 scenarios in post-store-detection.md (burst 5 concurrent stores, write pool error not MCP error)
- R-17 (High): 2 scenarios in search-reranking.md (deprecated entry present, raw scores in metadata)
- R-18 (High): 2 scenarios in nli-provider.md (cache subdirs distinct, deberta entailment > 0.5)
- R-19 (Med): 3 scenarios in nli-provider.md (511+10 tokens valid, 512+512 no panic, 256+256 valid)
- R-20 (Med): 1 integration scenario in bootstrap-promotion.md (promotion-then-post-store conflict)
- R-21 (Med): 1 scenario in eval-integration.md (eval layer no store during replay)
- R-22 (Med): build gate via cargo check in config-extension.md + model-download-cli.md

Risk priorities correctly reflected: Critical (R-01, R-03, R-05, R-09, R-10) each have 3+ scenarios; High risks have 2–3 scenarios; Med risks have 1–2 scenarios.

Integration risk scenarios (from RISK-TEST-STRATEGY.md Integration Risks section) are covered by:
- Pool saturation under sustained load → R-01 concurrent tests
- write_pool_server contention between NLI tasks and background tick → R-16 burst test
- EvalServiceLayer with_rate_config affecting existing callers → eval-integration.md Step 6b note
- GRAPH_EDGES write path divergence (post-store vs bootstrap race) → R-20 test

Edge cases from Risk Strategy are covered in test plans:
- Empty candidate pool after quarantine filter → search-reranking.md
- Single-word query vs full document → nli-provider.md
- nli_top_k smaller than requested k → search-reranking.md notes
- UTF-8 multibyte truncation → noted in nli-provider.md edge cases section
- Softmax overflow for extreme logits → nli-provider.md softmax test
- max_contradicts_per_tick=1 with pair above both thresholds → auto-quarantine-threshold.md

---

### Check 4: Interface Consistency

**Status**: PASS

**Evidence**: All shared types defined in OVERVIEW.md are used consistently across component files:

**`NliScores`**:
- Defined in OVERVIEW.md with fields `{entailment: f32, neutral: f32, contradiction: f32}`
- Used correctly in nli-provider.md (softmax output), search-reranking.md (sort key is `.entailment`), post-store-detection.md (threshold comparisons), bootstrap-promotion.md (threshold comparison), auto-quarantine-threshold.md (metadata parsing)

**`CrossEncoderProvider` trait**:
- Defined in OVERVIEW.md with `score_pair`, `score_batch`, `name` (synchronous)
- Used as `Arc<dyn CrossEncoderProvider>` in bootstrap-promotion.md (function signature matches architecture)
- Implemented by `NliProvider` in nli-provider.md
- Mock providers in test plans reference this trait correctly

**`NliModel` enum**:
- Defined consistently: `NliMiniLM2L6H768` / `NliDebertaV3Small`, `from_config_name`, `model_id`, `onnx_repo_path`, `onnx_filename`, `cache_subdir`
- Used in nli-provider.md (model construction), nli-service-handle.md (resolve_nli_model helper), config-extension.md (validate), model-download-cli.md (CLI resolution)

**`ServerError` variants**:
- `NliNotReady` and `NliFailed(String)` defined in OVERVIEW.md
- Used consistently in nli-service-handle.md (return types), search-reranking.md (fallback trigger), post-store-detection.md (early exit), eval-integration.md (wait_for_nli_ready)

**NLI edge write contract**:
- Defined in OVERVIEW.md: `source='nli'`, `bootstrap_only=0`, `INSERT OR IGNORE`, JSON metadata
- Applied consistently in post-store-detection.md (`write_nli_edge`) and bootstrap-promotion.md (`promote_bootstrap_edge`)

**`run_bootstrap_promotion` signature** from ARCHITECTURE.md:
```
async fn(Arc<Store>, Arc<NliServiceHandle>, Arc<RayonPool>, &InferenceConfig)
```
Pseudocode bootstrap-promotion.md has:
```
async fn run_bootstrap_promotion(store, provider, rayon_pool, config)
```
Minor: `maybe_run_bootstrap_promotion` takes `nli_handle` (not `provider`) and extracts provider internally. The public `run_bootstrap_promotion` signature differs slightly from the architecture integration surface — it takes `Arc<dyn CrossEncoderProvider>` rather than `Arc<NliServiceHandle>`. This is an acceptable internal design choice (the provider is already resolved by `maybe_run_bootstrap_promotion`), not a contract violation. The architecture integration surface defines `maybe_run_bootstrap_promotion`, not `run_bootstrap_promotion`.

No contradictions between component pseudocode files found. Data flow between components is coherent: embedding moves from store_ops.rs to nli_detection.rs (move semantics), neighbors are fetched inside run_post_store_nli, GRAPH_EDGES writes use write_pool_server() consistently.

---

### Check 5: W1-2 Compliance (Bootstrap Promotion)

**Status**: PASS

**Evidence**: The spawn prompt specifically requires validation that bootstrap promotion pseudocode batches all NLI pairs and dispatches via `rayon_pool.spawn()` — NOT calling `score_pair`/`score_batch` inline in async context.

From `bootstrap-promotion.md`, Step 3:
```
// Step 3: W1-2 constraint — ALL inference dispatched as a SINGLE rayon spawn.
// Build pairs from indexed_pairs before moving into closure.
let pairs_owned: Vec<(String, String)> = indexed_pairs.iter()...

let provider_clone = Arc::clone(&provider)
let nli_scores: Vec<NliScores> = match rayon_pool.spawn(move || {
    let pairs: Vec<(&str, &str)> = pairs_owned.iter()...collect()
    provider_clone.score_batch(&pairs)
}).await:
```

This is correct: pairs are collected first (lines before the spawn), then dispatched in a single `rayon_pool.spawn(move || ...)`, then the async tick awaits the result. There is no per-pair spawn, no per-pair call, and no inline `score_batch` call outside a rayon closure.

The OVERVIEW.md also explicitly documents this at the top level:
```
// 3. ALL NLI inference batched into a single rayon spawn — W1-2 contract
scores: Vec<NliScores> = rayon_pool.spawn(move || {
    provider.score_batch(&all_pairs)   // CPU-bound ONNX inference on rayon thread
}).await?
```

The test plan further enforces this with `test_bootstrap_promotion_single_rayon_spawn_for_all_pairs` in bootstrap-promotion.md, which uses a `RayonSpawnCounter` and asserts `spawn_count() == 1`.

All three inference paths comply with W1-2:
- **Search re-ranking**: `rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)` — PASS
- **Post-store detection**: `rayon_pool.spawn(...)` (no timeout, background) — PASS
- **Bootstrap promotion**: single `rayon_pool.spawn(...)` for all pairs — PASS

No `spawn_blocking` is used for NLI inference anywhere. `spawn_blocking` appears only for model loading in `nli-service-handle.md` (I/O + one-time CPU, explicitly noted as NOT rayon pool — correct per architecture).

---

### Check 6: Knowledge Stewardship — Pseudocode Agent

**Status**: PASS

**Evidence**: `crt-023-agent-1-pseudocode-report.md` contains:
```
## Knowledge Stewardship
- Queried: `/uni-query-patterns` for NLI cross-encoder ONNX inference patterns and crt-023 ADRs
  — found 5 relevant entries (#2700, #2701, #2702, #2703, #2716).
- Stored: [implicit via "Deviations from established patterns" section noting NliProvider
  softmax differs from OnnxProvider mean-pooling]
```

The report has both `Queried:` entries (evidence of /uni-query-patterns before implementing) and notes on novel findings (softmax vs mean-pool difference, `wait_for_nli_ready` addition). The "nothing novel to store" reasoning is implicit rather than explicit but the report does describe what was examined and the pattern deviations found. This meets the stewardship standard.

---

### Check 7: Knowledge Stewardship — Architect Agent

**Status**: WARN (re-check after rework)

**Evidence**: `crt-023-agent-1-architect-report.md` previously had NO `## Knowledge Stewardship` section. The section has been added and now contains:
- `Queried:` entry documenting entries #67, #1544, and crt-022 ADR-003 queried before designing
- `Stored:` entries for ADR-001 through ADR-006 with confirmed Unimatrix entry IDs (#2700–#2705)
- ADR-007 noted as stored in architecture file; Unimatrix entry ID not yet assigned

**Residual WARN**: ADR-007 (`nli-auto-quarantine-threshold.md`) exists as an architecture file but has no Unimatrix entry ID recorded in the stewardship block. Coordinator should store ADR-007 via `/uni-store-adr` and record the assigned entry ID. This does not block the gate — the file exists and the stewardship block is honest about the gap.

---

## Rework Resolved

The single FAIL item from the initial run has been addressed:

| Issue | Resolution |
|-------|------------|
| Missing `## Knowledge Stewardship` section in architect report | Section added to `crt-023-agent-1-architect-report.md` with `Queried:` and `Stored:` entries for ADR-001–006; ADR-007 flagged for coordinator to store via `/uni-store-adr` |

## Follow-Up Action (Non-Blocking)

| Action | Owner | Priority |
|--------|-------|----------|
| Store ADR-007 in Unimatrix via `/uni-store-adr` and record entry ID in architect report | Coordinator | Before gate 3b |

---

## Additional Observations (Not Blocking)

### Open Flag in Pseudocode (Flag 1 — GRAPH_EDGES Directionality)

`auto-quarantine-threshold.md` explicitly flags that `query_contradicts_edges_for_entry` uses `WHERE target_id = ?1` (penalized entry is target), which must be verified against the crt-021 schema before implementation. This is a correctly surfaced implementation-time verification requirement, not a pseudocode defect.

### Open Flag in Pseudocode (Flag 2 — Softmax Label Order)

`nli-provider.md` explicitly notes that the label order `[entailment=0, neutral=1, contradiction=2]` must be verified against MiniLM2's `config.json` `id2label` before coding. This is a correctly surfaced implementation-time verification requirement.

### Spec AC-07 "Nine" vs FR-11 Ten-Field Table

AC-07 in SPECIFICATION.md says "all nine NLI fields" but FR-11 defines a table of 10 fields (the 10th being `nli_auto_quarantine_threshold`). The pseudocode correctly follows FR-11 and implements all 10 fields. The spec text count error in AC-07 should be corrected at next available opportunity but does not block this gate or the implementation.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for validation failure patterns across features — not applicable for this gate run (gate 3a validator does not use /uni-query-patterns per role boundaries).
- Stored: nothing novel to store — the missing stewardship block in an architect report is a crt-023-specific observation, not a recurring pattern across features. If this pattern appears in multiple feature gates, a lesson-learned entry "architect agents must include Knowledge Stewardship block" would be warranted.
