# crt-022 Researcher Report (W1-2 Rewrite)

**Agent**: crt-022-researcher
**Feature**: crt-022 (W1-2: Rayon Thread Pool + Embedding Migration)
**Replaces**: Prior researcher report (scoped to combined NLI+Rayon)

## Deliverable

SCOPE.md rewritten at `/workspaces/unimatrix/product/features/crt-022/SCOPE.md`.
NLI content removed entirely. Rayon infrastructure only.

---

## Key Findings

### Scope Reduction Is Clean

The old SCOPE.md conflated two distinct features: rayon infrastructure (W1-2) and
NLI integration (now W1-4). The split is clean — no acceptance criteria from the
rayon scope depend on NLI, and no NLI content needs to survive in SCOPE.md. The
bootstrap edge promotion path (OQ-7 in old SCOPE.md) is purely a W1-4 concern.

### AsyncEmbedService Has No Consumers

The most important structural finding: `AsyncEmbedService` in
`unimatrix-core/src/async_wrappers.rs` is defined but never imported anywhere in
`unimatrix-server`. The server bypasses it entirely, using `EmbedServiceHandle.
get_adapter()` → `Arc<EmbedAdapter>` → direct `spawn_blocking` closure. This
means:
- There is no "migration" of `AsyncEmbedService` to rayon — it is simply removed.
- The rayon migration targets 8 direct call sites in server services.
- `AsyncVectorStore` (same file) has ~20 import sites in the server and stays.

### Actual ONNX Inference Call Sites (8 total)

All confirmed by file read and grep:

1. `services/search.rs:228` — query embedding, `spawn_blocking_with_timeout`
2. `services/store_ops.rs:113` — store-path embedding, `spawn_blocking_with_timeout`
3. `services/store_correct.rs:50` — correction-path embedding, `spawn_blocking_with_timeout`
4. `background.rs:543` — contradiction scan loop, plain `spawn_blocking` (longest-running)
5. `background.rs:1162` — quality-gate embedding loop, plain `spawn_blocking`
6. `uds/listener.rs:1383` — warmup embedding, plain `spawn_blocking`
7. `services/status.rs:542` — embedding consistency check, `spawn_blocking_with_timeout`
8. `async_wrappers.rs:100,110` — `AsyncEmbedService` methods (dead code, remove)

Non-inference `spawn_blocking` sites that stay (confirmed by reading):
- `embed_handle.rs:76` — `OnnxProvider::new` (model I/O, not inference)
- `background.rs:1088` — extraction rule evaluation (no ONNX)
- `background.rs:1144` — shadow evaluation persistence (DB write)
- `server.rs`, `gateway.rs`, `usage.rs`, `uds/listener.rs` (various) — DB/registry I/O

### Rayon Is Not Present in Any Crate

Confirmed by grepping all `Cargo.toml` files. The only rayon reference is a
comment in `unimatrix-engine/Cargo.toml` explicitly excluding it from the
petgraph feature set. `rayon = "1"` goes only into `unimatrix-server/Cargo.toml`.

### Architect Consultation Resolved OQ-2 and OQ-5

The crt-022a architect report (already in the feature dir) established:
- OQ-2 RESOLVED: Async wrappers move to server. `unimatrix-core` stays lean.
  Rayon is a deployment scheduling concern, not a domain abstraction (ADR-001).
- OQ-5 RESOLVED: NLI provider lives in `unimatrix-server/src/infra/` for W1-4.
  No new crates at W1-2. `unimatrix-onnx` extraction deferred to before W3-1.

### Config Section Named `[inference]` Not `[nli]`

Old SCOPE.md proposed `[nli]` for all config. Since this feature is rayon-only,
the config section is named `[inference]` — a neutral name that accommodates W1-4
NLI parameters and W2-4 GGUF parameters without renaming. Only `rayon_pool_size`
is added in W1-2.

### Timeout Question Is Genuinely Open

`spawn_blocking_with_timeout` enforces `MCP_HANDLER_TIMEOUT` at the blocking-pool
level. Rayon closures have no equivalent built-in timeout. The spec writer needs
to decide whether to wrap `rayon_pool.spawn(...).await` with `tokio::time::timeout`
at each call site. This is OQ-2 in the new SCOPE.md (renamed from the resolved
OQ-2 about crate boundaries).

---

## Open Questions for Human Review

Only two genuine open questions remain after architect consultation:

**OQ-1 (NAMING)**: Should the shared pool struct be named `RayonPool`,
`OnnxPool`, or `MlInferencePool`? Low stakes; naming convention only.

**OQ-2 (DESIGN)**: After rayon migration, should per-call-site timeout semantics
be preserved by wrapping `rayon_pool.spawn(...).await` with `tokio::time::timeout`?
Or is the tokio-blocking-pool timeout superseded by rayon's work-stealing
behavior? The spec writer should resolve this before writing the implementation
plan.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for "rayon thread pool tokio bridge spawn_blocking
  inference" — found entry #2491 (Rayon-Tokio bridge pattern, tagged crt-022),
  entries #735 #771 #1367 (spawn_blocking saturation incidents and patterns).
  All relevant to this scope.
- Stored: entry #2524 "AsyncEmbedService is unused in unimatrix-server — server
  uses EmbedAdapter directly" via `mcp__unimatrix__context_store`. Topic:
  `unimatrix-core`. Tags: async-wrappers, crt-022, embed, onnx, rayon,
  server-layer, spawn-blocking.
