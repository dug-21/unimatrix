# crt-022 Researcher Report

**Agent**: crt-022-researcher
**Feature**: crt-022 (W1-2: Embedded NLI Model + Rayon Thread Pool)

## Deliverable

SCOPE.md written to `/workspaces/unimatrix/product/features/crt-022/SCOPE.md`.

## Key Findings

### W1-1 Handshake Is Fully Designed

crt-021 shipped the GRAPH_EDGES table with `bootstrap_only` flag, the
DELETE+INSERT promotion path (§SR-07), the direct `write_pool` requirement for
NLI-confirmed edges (§SR-02), and the `metadata` column for NLI scores. W1-2
has a clear, designed interface to fulfill. Zero bootstrap Contradicts edges
exist (AC-08 was a dead path — shadow_evaluations has no entry ID columns).

### Rayon Is Not Yet in the Codebase

No rayon usage exists anywhere in the crates (confirmed by grep). The crate
must be added to `unimatrix-server/Cargo.toml`. The call sites for embedding
inference are concentrated in `unimatrix-core/src/async_wrappers.rs` (all
`spawn_blocking`), `services/search.rs:228`, `services/store_ops.rs:113`, and
`background.rs:543` (contradiction scan — the longest-running call). The model
loading call site (`embed_handle.rs:76`: `OnnxProvider::new`) should stay on
`spawn_blocking` — it's I/O+initialization, not steady-state inference.

### The Embedding Provider Architecture Has a Crate Boundary Problem

`AsyncEmbedService` is in `unimatrix-core` and uses `spawn_blocking`. To give
it a rayon pool, either rayon goes into `unimatrix-core` (heavyweight dep for a
lean crate) or the async wrappers move to `unimatrix-server`. This is a design
decision requiring human input (OQ-2).

### NLI Model ONNX Availability Is Blocking

`cross-encoder/nli-deberta-v3-small` may not have a pre-exported ONNX file.
The design can proceed with any NLI-class ONNX model, but the exact model
determines tokenizer format, hash pinning, and example config values. The
architect needs a confirmed model before the specification can be written
without placeholders. `cross-encoder/nli-MiniLM2-L6-H768` (~85MB) is a
confirmed ONNX-available alternative.

### Current Download Path Has No Hash Verification

`unimatrix-embed/src/download.rs` checks only file existence and non-empty
size. The Critical security requirement (SHA-256 pin NLI model) requires
adding this capability to the NLI download path (not necessarily to the
embedding path, though the product vision recommends both).

### Contradiction Detection Today

`scan_contradictions` is a full O(N) pass running in `spawn_blocking` every
`CONTRADICTION_SCAN_INTERVAL_TICKS` ticks (~60 min). Results cache in
`ContradictionScanCacheHandle`. The `conflict_heuristic` uses regex-based
negation/directive/sentiment signals. This continues to run in W1-2 —
the NLI post-store pass is additive, not a replacement for the full scan.
Over time, as NLI edges accumulate in GRAPH_EDGES, the cosine heuristic
cache becomes supplementary.

### Config Infrastructure (W0-3/dsn-001) Is Proven

`UnimatrixConfig` with sub-structs, `load_config`, `validate_config`, preset
resolution — all complete. W1-2 adds a `[nli]` section to `UnimatrixConfig`
following the exact same pattern. `toml = "0.8"` is already in
`unimatrix-server/Cargo.toml`.

### Schema Version Is Stable

W1-2 requires no schema migration. The `metadata TEXT DEFAULT NULL` column on
`graph_edges` was added by W1-1 specifically to hold NLI scores for W3-1 GNN.
The schema is ready.

## Open Questions for the Human

Eight open questions are documented in SCOPE.md. The most important ones
requiring human decision before design proceeds:

1. **OQ-1 (BLOCKING)**: Which NLI model? DeBERTa-v3-small ONNX availability
   is uncertain. `nli-MiniLM2-L6-H768` is a confirmed alternative. Need a
   definitive choice + download source before the architect can spec the
   NliProvider without placeholders.

2. **OQ-2 (BLOCKING)**: Rayon pool in `unimatrix-core` (adds heavyweight dep)
   or move async wrappers to `unimatrix-server` (cleaner boundary)? This
   determines the crate structure the architect designs around.

3. **OQ-5 (DESIGN)**: NLI provider in `unimatrix-server/src/infra/` (simple)
   or new `unimatrix-nli` crate (clean boundary, reusable for W3-1)? The human
   noted "there is much to discuss" — this is a meaningful structural question.

4. **OQ-8 (SECURITY)**: Hash pinning required always (Critical, current
   proposal) or optional with warning (easier ops)? The product vision says
   Critical. Confirm this is intentional given operational friction.

## Risks Identified

- **Model availability gap** (High): If DeBERTa ONNX export requires an
  export step, that's not captured in the effort estimate and could add a day.
- **Crate boundary churn** (Medium): Moving `AsyncEmbedService` from
  `unimatrix-core` to `unimatrix-server` touches ~20 call sites. Scope creep
  risk if this is larger than expected.
- **NLI false positive rate** (Medium): NLI models trained on SNLI/MultiNLI
  may have high false positive rates on terse technical knowledge entries
  (e.g., "Use cargo test" vs "Use cargo nextest"). The circuit breaker (AC-12)
  mitigates damage; the domain qualifier in Non-Goals documents the caveat.
- **Effort underestimate** (Low): Product vision says 3-4 days. With the model
  availability question, crate boundary decision, and full test suite, this may
  run 5-6 days. The architect should validate.

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "NLI contradiction detection rayon thread pool" --
  found spawn_blocking saturation incidents (#735, #1628, #1688 referenced in entries);
  found OnnxProvider mutex pattern (entry #67, #19); found EmbedServiceHandle degradation
  lesson (entry #685).
- Queried: "ONNX model loading graceful degradation" -- found ADR-006 Lazy Embedding
  Model Initialization (entry #82), embedding init failure lesson (entry #685).
- Queried: "rayon tokio bridge ML inference spawn_blocking" -- found spawn_blocking pool
  saturation pattern (entry #735), spawn_blocking_with_timeout pattern (entry #1367),
  block_in_place bridge pattern (entry #2126).
- Stored: entry #2491 "Rayon-Tokio bridge pattern for CPU-bound ML inference" via
  `/uni-store-pattern`.
- Stored: entry #2492 "NLI model integration: W1-2 architecture constraints from
  codebase analysis" via `/uni-store-pattern`.
