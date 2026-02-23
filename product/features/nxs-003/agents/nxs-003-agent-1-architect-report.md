# Agent Report: nxs-003-agent-1-architect

**Agent ID:** nxs-003-agent-1-architect
**Role:** uni-architect
**Feature:** nxs-003 (Embedding Pipeline)
**Status:** COMPLETE

## Artifacts Produced

### ARCHITECTURE.md
- **Path:** `product/features/nxs-003/architecture/ARCHITECTURE.md`
- 11 components defined: provider, model, onnx, pooling, normalize, text, download, config, error, lib, test_helpers
- System context diagram, component interactions, data flow diagrams
- Full integration surface table (14 entries for this crate, 8 upstream, 5 downstream consumers)
- Cargo workspace integration with dependency rationale

### ADR Files
1. **ADR-001-mutex-session-concurrency.md** -- Mutex\<Session\> for ONNX inference (ort::Session::run requires &mut self)
2. **ADR-002-raw-ort-no-fastembed.md** -- Raw ort + tokenizers, no fastembed wrapper (edition 2024 risk, exact-pinned ort, limited control)
3. **ADR-003-hf-hub-model-download.md** -- hf-hub crate for model downloading (vs raw HTTP vs embedded binary)
4. **ADR-004-custom-cache-directory.md** -- Custom ~/.cache/unimatrix/models/ via dirs crate (vs HuggingFace default vs project-local)

## Key Design Decisions

1. **No dependency on unimatrix-store or unimatrix-vector.** The embed crate is standalone -- it produces `Vec<f32>` and the caller coordinates with store/vector. This keeps the dependency graph clean.

2. **Mutex\<Session\> + unlocked Tokenizer.** Only the ONNX session needs synchronization. Tokenization (the first step) is lock-free, minimizing time under lock.

3. **EmbeddingProvider trait is object-safe.** `&dyn EmbeddingProvider` and `Box<dyn EmbeddingProvider>` both work. The `Send + Sync` bound enables `Arc<dyn EmbeddingProvider>`.

4. **MockProvider for downstream testing.** Hash-based deterministic embeddings allow testing the full write/search path without ONNX model loading.

5. **7-model catalog, all 384-d.** Fixed dimension matches nxs-002's VectorConfig. Model is selected at init and fixed for provider lifetime.

6. **Empty string produces a valid embedding (not an error).** Avoids edge-case panics. Matches AC-12.

## Open Questions

1. **tokenizers `onig` feature + edition 2024.** May need fallback to `esaxx` or default features if onig's native dependencies don't build cleanly.

2. **ONNX output tensor name.** Expected to be `last_hidden_state` or index 0, but must be verified against actual model files during implementation.

3. **hf-hub cache atomicity under concurrent construction.** Unlikely in production (single OnnxProvider per process), but may surface in parallel tests.

## Self-Check

- [x] ARCHITECTURE.md contains System Overview, Component Breakdown, Interactions, Integration Surface
- [x] All ADRs follow format: `## ADR-NNN: Title` / `### Context` / `### Decision` / `### Consequences`
- [x] Each ADR is a separate file in `architecture/` with correct naming
- [x] ADR file paths included in return
- [x] Integration Surface table included for features with cross-boundary concerns
- [x] No placeholder or TBD sections -- unknowns flagged as open questions
- [x] All output files within `product/features/nxs-003/`
