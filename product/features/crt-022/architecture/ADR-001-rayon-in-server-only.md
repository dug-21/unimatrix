## ADR-001: Rayon Dependency Confined to `unimatrix-server`

### Context

The rayon thread pool is required for CPU-bound ML inference. The question is which
crate owns the rayon dependency. Two candidates exist:

- `unimatrix-core` — domain aggregation layer, hosts `EmbedService` and `VectorStore`
  traits, re-exports types for server convenience
- `unimatrix-server` — deployment entry point, owns the tokio runtime, responsible
  for startup wiring, resource allocation, and serving MCP requests

`unimatrix-core` currently holds `AsyncEmbedService`, which wraps `EmbedService` in
`spawn_blocking`. This represents the same category of mistake: execution scheduling
embedded in a domain crate. The architect consultation (crt-022a) identified this as
a crate boundary violation.

If rayon were added to `unimatrix-core`, it would pull a thread-pool dependency into
every crate that transitively depends on `unimatrix-core`. The embed crate, vector
crate, store crate, and any future domain crate would all inherit rayon. Domain
crates would export `RayonPool` or schedule-aware types, conflating domain
abstractions with deployment infrastructure.

### Decision

`rayon = "1"` is added to `unimatrix-server/Cargo.toml` only. No other crate in the
workspace gains a rayon dependency.

`AsyncEmbedService` is removed from `unimatrix-core/src/async_wrappers.rs`. It has
zero consumers in `unimatrix-server` (all server call sites use `EmbedAdapter`
directly). Its removal eliminates the scheduling-in-domain anti-pattern.

`AsyncVectorStore` remains in `unimatrix-core` because HNSW search is short-duration
memory-mapped index traversal — not the problem workload — and the `async` feature
on `unimatrix-core` is retained for it.

`RayonPool` (`unimatrix-server/src/infra/rayon_pool.rs`) is a server-side
infrastructure module. It is not exported from `unimatrix-core` or any domain crate.

### Consequences

Easier:
- `unimatrix-core` stays dependency-lean; future domain crate authors do not inherit
  scheduling infrastructure by default
- W1-4 (NLI) and W3-1 (GNN) providers are implemented in `unimatrix-server` or
  dedicated provider crates, not in the domain layer
- The domain boundary is clean: domain crates define what computation does, server
  crates define how it is scheduled

Harder:
- A future crate that wants ML inference must either depend on `unimatrix-server`
  (undesirable for library crates) or implement its own scheduling. This is the
  correct constraint: inference scheduling is a deployment decision.
- When `unimatrix-onnx` crate is extracted before W3-1 (deferred per SCOPE.md), its
  design must not include a rayon dependency. The extraction point is the ONNX
  provider types and trait impls, not the scheduling wrapper.
