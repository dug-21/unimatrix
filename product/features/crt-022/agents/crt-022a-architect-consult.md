# crt-022a Architectural Consultation Report

**Agent**: crt-022a-architect-consult
**Date**: 2026-03-19
**Scope**: OQ-2 (async wrapper migration) and OQ-5 (NLI provider home)

---

## 1. Current Structure Summary

### Crate graph (relevant subset)

```
unimatrix-embed          — ONNX + tokenizers, no tokio, no rayon
  └── OnnxProvider       — Mutex<Session>, EmbeddingProvider trait, sentence-transformer specific
      model.rs, onnx.rs, pooling.rs, text.rs, download.rs (hf-hub, no SHA-256)

unimatrix-core           — aggregation crate; re-exports store/vector/embed types
  └── async_wrappers.rs  — AsyncEmbedService + AsyncVectorStore, spawn_blocking wrappers
  features: async (gates async_wrappers), test-support
  deps: store, vector, embed, serde, serde_json, tokio

unimatrix-server         — binary + lib; everything runtime
  └── infra/embed_handle.rs  — EmbedServiceHandle state machine (Loading/Ready/Failed/Retrying)
  └── infra/ (14 modules)    — all server-layer infrastructure lives here
  deps: core (feature=async), store, embed, vector, engine, adapt, observe, learn
        rmcp, tokio, sqlx, schemars, tracing, clap, sha2, ...
```

### spawn_blocking call sites

The grep returned no matches — this means the sqlx migration (nxs-011/W0-1) already eliminated `spawn_blocking` for DB calls. The remaining call sites identified in SCOPE.md are:

1. `unimatrix-core/src/async_wrappers.rs` — `AsyncEmbedService` and `AsyncVectorStore` methods (ONNX inference)
2. `unimatrix-server/src/infra/embed_handle.rs:76` — `OnnxProvider::new()` at model load time (I/O, not inference; stays on spawn_blocking per SCOPE.md)
3. `unimatrix-server/src/services/search.rs:228` — query embedding
4. `unimatrix-server/src/services/store_ops.rs:113` — store-path embedding
5. `unimatrix-server/src/background.rs:543` — contradiction scan inference

The server-layer call sites (3, 4, 5) already bypass `async_wrappers` and call directly into the embed service. This means `async_wrappers.rs` is not the sole embedding path — the server has its own direct calls. This is a critical structural observation for OQ-2.

### unimatrix-embed anatomy

`unimatrix-embed` is sentence-transformer specific throughout:
- `model.rs` — `EmbeddingModel` enum with sentence-transformer model IDs and dimensions
- `onnx.rs` — `OnnxProvider`: mean-pooling + L2 normalization (sentence-transformer post-processing)
- `pooling.rs` — mean-pool implementation (sentence-transformer specific)
- `text.rs` — `prepare_text`, `embed_entry`, `embed_entries` helpers for (title, content) pairs
- `download.rs` — hf-hub download, no SHA-256

NLI inference requires: (premise, hypothesis) pair tokenization, separator concatenation, softmax over three logits. None of the sentence-transformer post-processing (mean-pool, L2 normalize) applies. The models are architecturally different classes.

### ML inference trajectory

| Wave | Model | Size | Runtime | Pool |
|------|-------|------|---------|------|
| Now | sentence-transformer (all-MiniLM-L6-v2) | ~90MB | ONNX/ort | → rayon (W1-2) |
| W1-2 | NLI (DeBERTa-v3-small or MiniLM2) | 85–180MB | ONNX/ort | shared rayon |
| W3-1 | GNN (Graph Attention Network) | ~400KB | ONNX/ort | shared rayon |
| W3-3 | GGUF (Phi-3.5-mini) | ~2.2GB | llama.cpp FFI | separate rayon pool |

Three ONNX models by W3-1, all using `ort = "2.0.0-rc.9"`. One GGUF model, separate stack. The ONNX models accumulate. The GNN is the deciding factor for crate structure.

---

## 2. Recommendation A: Async Wrapper Migration (OQ-2)

**Verdict: Move async wrappers to unimatrix-server (option b). unimatrix-core stays lean; rayon is a server-layer concern.**

### Rationale

**The consumer question is settled.** `unimatrix-core` has exactly one downstream consumer: `unimatrix-server`. There is no other crate in the workspace that depends on `unimatrix-core`. Confirmed via the workspace Cargo.toml — `unimatrix-server` is the only binary. There is no planned library consumer of `unimatrix-core` in any wave. `unimatrix-core` is an aggregation crate that re-exports types from store/vector/embed for the server's convenience.

**ADR-001 (Core Crate as Trait Host) establishes the right role.** `unimatrix-core` hosts `EmbedService` and `VectorStore` traits and their adapters. It is a domain abstraction layer. The async execution strategy (tokio vs rayon) is a deployment concern, not a domain concern. Putting rayon in `unimatrix-core` violates this role boundary — it says "the domain layer knows how embeddings are scheduled," which it should not.

**The server already bypasses async_wrappers for the critical paths.** Call sites 3, 4, and 5 in the server invoke the embed service directly without going through `AsyncEmbedService`. This means `async_wrappers.rs` is already not the authoritative embedding path — it is used by some callers but not all. Moving it to the server makes this explicit: all rayon-dispatched calls live in one layer.

**Rayon in unimatrix-core is a layering violation.** `unimatrix-core` currently depends on: store, vector, embed, serde, serde_json, tokio (rt only). Adding rayon would introduce a 2MB compile-time dependency and a thread pool lifecycle concern into what is meant to be a thin glue layer. A library that aggregates domain types does not decide how many threads to use for inference.

**Option (c) — feature flag — is the worst of both.** A rayon feature flag in `unimatrix-core` preserves the layering violation while adding feature matrix complexity. The only code that enables that feature is `unimatrix-server`. It is indirection without benefit.

### Implementation consequence

The rayon-tokio bridge and `RayonPool` handle live entirely in `unimatrix-server`. The `async_wrappers.rs` module in `unimatrix-core` is either:
- Deleted entirely if all call sites move to direct rayon-pool dispatch in the server, or
- Retained but kept tokio-only for the `AsyncVectorStore` methods (HNSW search is not migrating to rayon per SCOPE.md §Non-Goals).

The `AsyncEmbedService` specifically is replaced by server-local rayon dispatch. `AsyncVectorStore` can stay in `unimatrix-core` since HNSW search remains on `spawn_blocking` (short-duration, memory-mapped, not the problem workload).

**Concrete boundary after migration:**
- `unimatrix-core/src/async_wrappers.rs` — retains `AsyncVectorStore` (spawn_blocking, HNSW); `AsyncEmbedService` is removed or deprecated
- `unimatrix-server/src/infra/rayon_pool.rs` — new: `RayonPool`, `rayon_spawn<F,T>` bridge, `RayonError`
- `unimatrix-server/src/infra/embed_handle.rs` — updated: `get_adapter()` returns something the rayon bridge can call directly
- All embed inference call sites in server services use `rayon_spawn` directly

---

## 3. Recommendation B: NLI Provider Home (OQ-5)

**Verdict: unimatrix-server/src/infra/nli_provider.rs for W1-2 (option a), with an explicit commitment to extract to unimatrix-onnx before W3-1 ships.**

This is a two-part answer because the right answer at W1-2 and the right answer at W3-1 are different — and conflating them is the source of the "much to discuss" tension.

### Why not unimatrix-embed (option c) — unambiguous

`unimatrix-embed` is sentence-transformer specific at every layer:
- `EmbeddingModel` enum — sentence-transformer model IDs and output dimensions
- `pooling.rs` — mean-pool (sentence-transformer post-processing; NLI does not use this)
- `text.rs` — `(title, content)` pair helpers (NLI takes `(premise, hypothesis)`)
- `download.rs` — no SHA-256 verification (NLI requires it per AC-07; the gap must be filled)

Adding NLI to `unimatrix-embed` would require branching all of these on model class. The crate would become "ONNX models" rather than "sentence-transformer embedding." That rename of purpose is a loss of clarity that accumulates with every new model class added.

Option (c) is definitively rejected.

### Why not a new unimatrix-nli crate at W1-2 (option b as stated)

The argument for a new crate is correct in its final form but premature at W1-2. The issue is that `unimatrix-nli` as described in the SCOPE is too narrow — it would be the NLI-specific crate. But the GNN (W3-1) also needs ONNX inference. Creating `unimatrix-nli` and then `unimatrix-gnn` (or adding GNN to it) is the wrong split.

The right abstraction, when ONNX accumulates three models, is a **shared ONNX infrastructure crate** (`unimatrix-onnx`) that provides:
- Session lifecycle (builder, graph optimization, `Mutex<Session>` pattern)
- SHA-256 hash verification at load time
- Generic tokenizer loading
- A provider trait that is model-class agnostic

Then model-specific crates (`unimatrix-embed`, a future `unimatrix-nli`) implement that infrastructure. Or alternatively, all three ONNX models live as modules in a single `unimatrix-onnx` crate, organized by model class.

Creating `unimatrix-nli` now without `unimatrix-onnx` means duplicating ONNX session boilerplate between the two crates. But creating `unimatrix-onnx` now — before we understand exactly what the GNN needs — risks designing the abstraction wrong. GNN inference with `ort` uses different tensor shapes and a different execution pattern than sequence classification.

### Why unimatrix-server/src/infra/nli_provider.rs for W1-2

- **Zero new crate overhead.** NLI in W1-2 is server-only: it reads neighbor embeddings from HNSW, writes edges to GRAPH_EDGES, is configured via `UnimatrixConfig`, and uses the same `write_pool`. None of this bleeds outside the server. A server-local module has full access to all server infrastructure without any dependency threading.

- **Follows the established embed_handle.rs pattern exactly.** `NliServiceHandle` is a direct structural clone of `EmbedServiceHandle`. Both live in `infra/`. Both implement `Loading → Ready | Failed → Retrying`. The pattern is proven and already in the right layer.

- **SHA-256 verification belongs in the load path.** The hash check at load time in `embed_handle.rs` (via `spawn_blocking`) is server-layer infrastructure. Adding it for the NLI provider in the same module is natural — not a layering violation.

- **Defers the abstraction decision to when the GNN requirements are known.** W3-1 GNN design will reveal what a shared ONNX infrastructure layer actually needs to provide. Designing `unimatrix-onnx` before GNN is speculative.

### The extraction commitment (before W3-1 ships)

Before W3-1 ships, there will be two ONNX providers (embed, NLI) and the GNN arrives as a third. At that point the right action is:

1. Create `unimatrix-onnx` crate with shared session infrastructure, SHA-256 loader, provider trait
2. Refactor `unimatrix-embed` and `nli_provider.rs` to use it
3. Implement GNN as a module in `unimatrix-onnx` or as `unimatrix-gnn` depending on size

This extraction is a W3-1 prerequisite design task, not a W1-2 task. The SCOPE.md for W3-1 should include this as an architectural step before GNN implementation.

**W3-3 GGUF is unambiguously its own crate** (`unimatrix-infer`) behind a Cargo feature flag. llama.cpp FFI, platform-specific compilation, and a separate rayon pool make it a distinct subsystem. Nothing in this recommendation affects that.

### Decision summary for W1-2

```
unimatrix-server/src/infra/
  nli_provider.rs     — NliProvider (Mutex<Session>, (premise, hypothesis) input, softmax output)
  nli_handle.rs       — NliServiceHandle (clone of EmbedServiceHandle state machine)
  rayon_pool.rs       — RayonPool, rayon_spawn bridge, RayonError
```

`unimatrix-embed` is unchanged. No new crates at W1-2. ONNX infrastructure duplication between `unimatrix-embed` and `nli_provider.rs` is accepted as a deliberate short-term cost, documented with a `// TODO(W3-1): extract to unimatrix-onnx` comment.

---

## 4. Implications for crt-022a Scope

### What this resolves

- **OQ-2 resolved**: Move async wrappers → server. `unimatrix-core` gets no new deps. `AsyncEmbedService` is removed from `unimatrix-core`; `AsyncVectorStore` may stay (HNSW stays on spawn_blocking).
- **OQ-5 resolved**: NLI provider is server-local at W1-2. No new crate. A `// TODO(W3-1)` comment documents the future extraction.

### What this means for the implementation plan

1. **rayon is added only to unimatrix-server/Cargo.toml** — not to unimatrix-core. This is the cleaner change. The SCOPE.md already suggests this as the primary path.

2. **The async_wrappers.rs `async` feature in unimatrix-core becomes narrower** — it covers only `AsyncVectorStore` after `AsyncEmbedService` is removed. Consider whether the feature is still worth gating, or whether `AsyncVectorStore` can move to `unimatrix-server` as well, allowing the `async` feature and tokio dep to be removed from `unimatrix-core` entirely. This is a scope question for crt-022a: full cleanup is cleaner but adds breadth.

3. **NliServiceHandle follows EmbedServiceHandle exactly** — the state machine, the retry monitor, the spawn_blocking model-load path (loading the ONNX session is I/O + initialization, not steady-state CPU inference; it stays on spawn_blocking). SHA-256 hash check runs inside the spawn_blocking load task before session construction.

4. **RayonPool is a server startup concern** — initialized in `main.rs` or the server's `AppState`/`ServiceLayer` construction, threaded to all subsystems that need it. The pool size comes from `[nli] rayon_pool_size` in config.

5. **No unimatrix-onnx crate is created in crt-022** — this must be explicitly marked as out of scope to prevent scope creep. The extraction ADR should be stored in Unimatrix as a deferred decision with topic `crt-022` so W3-1's architect picks it up.

### Open question that remains design-critical

**OQ-1 (BLOCKING) is still unresolved** — which NLI ONNX model is used determines the tokenizer format and SHA-256 hash that gets pinned in the example config. The NLI provider implementation cannot be finalized until this is answered. The architectural choices above are model-agnostic, but implementation cannot start on `NliProvider::new()` without a concrete model target.

---

## References

- Unimatrix entry #2491 — Rayon-Tokio bridge pattern (tagged crt-022)
- Unimatrix entry #71 — ADR-001: Core Crate as Trait Host
- Unimatrix entry #316 — ServiceLayer extraction pattern for unimatrix-server
- `unimatrix-embed/src/onnx.rs` — sentence-transformer-specific inference pipeline
- `unimatrix-server/src/infra/embed_handle.rs` — the pattern NliServiceHandle should clone
- `product/features/crt-022/SCOPE.md` §Proposed Approach Phase 2 and Phase 3
