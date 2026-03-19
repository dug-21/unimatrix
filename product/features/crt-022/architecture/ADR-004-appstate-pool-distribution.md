## ADR-004: `Arc<RayonPool>` Distributed via `AppState`, Not as Discrete Parameters

### Context

`Arc<RayonPool>` must reach every inference consumer at startup. The current server
architecture passes dependencies to subsystems either as fields on service structs
or as discrete parameters threaded through startup wiring. As the system grows (W1-4
NLI, W2-4 GGUF, W3-1 GNN), the number of ML infrastructure handles grows.

SR-06 in the scope risk assessment identifies the risk: if pool distribution is done
ad-hoc per consumer (passing `Arc<RayonPool>` as a discrete parameter to each
subsystem independently), a future caller may instantiate a second pool
inadvertently. Two pools running concurrently would violate Constraint 5 (single
shared pool for W1-2) and double the thread count without operator awareness.

Two distribution patterns were considered:

**Option A — Discrete parameter threading**
`Arc<RayonPool>` is passed as an argument to each consumer's constructor or
initialisation function. `SearchService::new(store, embed, rayon_pool, ...)`,
`BackgroundTick::new(store, embed, rayon_pool, ...)`, etc.

Problems: as more ML handles are added (NLI service handle, GGUF pool), the
constructor argument lists grow. Each new consumer requires a startup wiring change
at its specific call site. A developer adding W1-4 can call
`RayonPool::new(...)` in their initialisation path, creating a second pool, with no
compile-time or runtime enforcement preventing it.

**Option B — `AppState` owns all shared infrastructure**
A top-level `AppState` struct (or equivalent startup aggregate) owns all shared
infrastructure handles: `Arc<RayonPool>`, `Arc<EmbedServiceHandle>`, store handles,
etc. Each subsystem receives `Arc<AppState>` (or appropriate sub-structs) rather
than discrete handles.

W1-4 adds `Arc<NliServiceHandle>` to `AppState`. W2-4 adds `Arc<RayonPool>` (the
separate GGUF pool) to `AppState`. W3-1 accesses the `ml_inference_pool` already on
`AppState`. The single-pool invariant is enforced structurally: there is one field,
not N independently created instances.

### Decision

**Option B is adopted.** `Arc<RayonPool>` is a field on the server's `AppState`
(or startup wiring struct) alongside `Arc<EmbedServiceHandle>` and other shared
handles. All inference consumers receive access to the pool via the shared state
struct, not as discrete constructor arguments.

The pool is initialised once in `main.rs` startup wiring using
`RayonPool::new(config.inference.rayon_pool_size, "ml_inference_pool")`. The
resulting `Arc<RayonPool>` is stored on `AppState` and not constructed elsewhere.

A `// TODO(W2-4): add gguf_rayon_pool: Arc<RayonPool> here` comment marks the
extension point for W2-4's separate GGUF pool.

### Consequences

Easier:
- W1-4 and W3-1 access `app_state.ml_inference_pool` — no new wiring needed
- Single instantiation site makes pool size easy to audit and configure
- W2-4 adds a second named pool to `AppState` without affecting the existing pool
- No risk of accidental second instantiation: the field is the only construction site

Harder:
- `AppState` grows as more handles are added; this is the expected tradeoff for
  centralised dependency management
- Subsystems that currently receive fine-grained dependencies may need refactoring
  to accept `AppState` or a sub-struct. This is a one-time cost at W1-2 wiring time.
