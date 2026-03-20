# Agent Report: crt-023-agent-1-pseudocode

**Agent**: crt-023-agent-1-pseudocode
**Session**: 2 Stage 3a — Pseudocode Production
**Feature**: crt-023 — NLI + Cross-Encoder Re-ranking (W1-4)
**Status**: COMPLETE

---

## Summary

Produced 10 pseudocode files for all 8 crt-023 components. All files are in
`product/features/crt-023/pseudocode/`.

---

## Files Produced

| File | Component | Purpose |
|------|-----------|---------|
| `OVERVIEW.md` | Cross-cutting | Component map, data flow, shared types, sequencing constraints |
| `nli-provider.md` | NliProvider (`unimatrix-embed`) | CrossEncoderProvider trait, NliScores struct, NliModel enum, NliProvider impl, ensure_nli_model |
| `nli-service-handle.md` | NliServiceHandle (`infra/nli_handle.rs`) | State machine, SHA-256 verification, poison detection, retry monitor |
| `config-extension.md` | Config (`infra/config.rs`) | 10 InferenceConfig fields, validate(), pool floor, ConfigError variants |
| `search-reranking.md` | SearchService (`services/search.rs`) | HNSW expansion, NLI re-ranking step, stable sort, fallback path |
| `post-store-detection.md` | Post-store NLI (`services/nli_detection.rs`) | Fire-and-forget spawn, run_post_store_nli, write_nli_edge, format_nli_metadata |
| `bootstrap-promotion.md` | Bootstrap promotion (`services/nli_detection.rs`) | One-shot promotion, COUNTERS idempotency, single rayon spawn for batch |
| `auto-quarantine-threshold.md` | Auto-quarantine guard (`background.rs`) | NLI-only penalty classification, higher threshold guard |
| `eval-integration.md` | Eval integration (`eval/profile/layer.rs`) | W1-4 stub fill, NLI handle construction, 60s readiness wait, SKIPPED profile handling |
| `model-download-cli.md` | Model download CLI (`main.rs`) | --nli flag extension, ensure_nli_model, compute_file_sha256, SHA-256 stdout output |

---

## Source Documents Read

- `product/features/crt-023/IMPLEMENTATION-BRIEF.md` — authoritative for resolved decisions, function signatures, data structures
- `product/features/crt-023/architecture/ARCHITECTURE.md` — component boundaries, integration surface, SQL contracts
- `product/features/crt-023/specification/SPECIFICATION.md` — FR-01 through FR-29, AC-01 through AC-25
- `product/features/crt-023/RISK-TEST-STRATEGY.md` — R-01 through R-22, non-negotiable tests
- `product/features/crt-023/architecture/ADR-001` through `ADR-007` — all 7 ADRs read in full
- Existing codebase: `OnnxProvider`, `EmbedServiceHandle`, `InferenceConfig`, `SearchService`, `StoreService`, `EvalServiceLayer`, `background.rs`, `main.rs`, `ensure_model`

---

## Open Questions / Flags for Implementation

### Flag 1: GRAPH_EDGES Contradicts Directionality (auto-quarantine-threshold.md)

The `query_contradicts_edges_for_entry` Store method uses `WHERE target_id = ?1` (penalized entry
is the target). This assumes the NLI pipeline writes edges where the NEW entry (premise/source)
contradicts the NEIGHBOR entry (hypothesis/target). This must be verified against crt-021 schema
before implementing `query_contradicts_edges_for_entry`. If the convention is reversed (penalized
entry is source_id), the WHERE clause must change to `WHERE source_id = ?1`.

### Flag 2: NliProvider Softmax Label Order

`softmax_3class` in `nli-provider.md` assumes output index 0=entailment, 1=neutral, 2=contradiction.
This must be verified against the MiniLM2 ONNX model's `config.json` `id2label` field at
implementation time. Common MiniLM2 fine-tuned NLI models use this order, but it is not universal.

### Flag 3: run_replay_loop Profile/Layer Index Alignment

`eval-integration.md` notes that after SKIPPED profile handling, `profiles` and `layers` may have
different lengths. The existing `run_replay_loop` in `eval/runner/replay.rs` must be checked for
1:1 profile/layer index assumptions before implementing the SKIPPED path.

### Flag 4: ServiceLayer::with_rate_config Signature Change Impact

Adding `nli_handle`, `nli_top_k`, and `nli_enabled` parameters to `with_rate_config` affects all
call sites: `ServiceLayer::new`, `EvalServiceLayer::from_profile`, and any test fixtures using
`with_rate_config`. All call sites must be updated together in the same commit.

### Flag 5: sha2 Crate Dependency Placement

The `sha2 = "0.10"` dependency may need to be added to `unimatrix-embed/Cargo.toml` (for
`NliServiceHandle` hash verification in the embed crate) AND/OR `unimatrix-server/Cargo.toml`
(for `compute_file_sha256` in main.rs). Verify which crates already have it before adding.

---

## Key Design Decisions Reflected in Pseudocode

- **W1-2 rayon constraint**: Every NLI inference call routes through `rayon_pool.spawn()` (post-store,
  bootstrap) or `rayon_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT)` (search). Never inline async,
  never via `spawn_blocking`.
- **Conservative error handling**: Every DB error or missing metadata in the auto-quarantine guard
  returns `true` (allow quarantine), not `false`. The guard only suppresses quarantine when there is
  positive evidence all NLI scores are below the higher threshold.
- **Embedding move semantics (ADR-004)**: The `Vec<f32>` is MOVED into the fire-and-forget tokio task.
  The comment "NLI hand-off point" is required in the implementation to prevent future reordering.
- **INSERT OR IGNORE idempotency**: All NLI edge writes use `INSERT OR IGNORE` on the
  `UNIQUE(source_id, target_id, relation_type)` constraint. No deduplication logic needed at the
  application level.
- **Eval SKIPPED path (ADR-006)**: A missing NLI model produces a SKIPPED profile entry (not an
  error). `skipped.json` is written to the output directory. The baseline profile always runs.
- **SHA-256 output to stdout**: Hash goes to stdout; all other messages go to stderr. This allows
  `unimatrix model-download --nli | head -1` for hash capture without redirecting progress messages.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for NLI cross-encoder ONNX inference patterns and crt-023 ADRs — found 5 relevant entries (#2700 ADR-001, #2701 ADR-002, #2702 ADR-003, #2703 ADR-004, #2716 ADR-007). ADR-005 and ADR-006 not yet stored at query time; read from ADR files directly.
- Deviations from established patterns:
  - `NliProvider` softmax output differs from `OnnxProvider` mean-pooling + L2-normalize. Otherwise mirrors `OnnxProvider` exactly (Mutex<Session> + Tokenizer outside).
  - `NliServiceHandle` adds `wait_for_nli_ready()` method not present in `EmbedServiceHandle`. This is an eval-specific method; eval requires synchronous blocking wait, not present in the server path.
  - `ensure_nli_model` in `download.rs` is a new function alongside `ensure_model` — same internal structure, different model type argument. Not a deviation, but implementation agent should confirm `file_size` helper is shared (not duplicated).
