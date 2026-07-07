## ADR-001: Per-Slug Observe State Resolves on `Arc<dyn StoreResolver>` — Registry, Pending, and Services Beside `resolve_store`, No Side-Map

### Context

vnc-038 (ADR-003, #5082) put observe on the per-request store funnel but resolved **only the
store**. `ObserveContext` (`http/router.rs:81-102`) still carries `session_registry`,
`pending_entries_analysis`, and `services` as flat **daemon-global** `Arc` clones wired in
`main.rs:1274-1276`. So observe-path transcript deltas, pending-entries state, and
briefing/search/compact **reads** all hit the *global* instance, while each slug's
`UnimatrixServer` reads its *own* constructor-default instance — the #930 split-brain, plus the
P2 cross-project knowledge-**read** leak (SR-07): a slug's observe-path briefing reads another
project's persisted knowledge store.

The governing pattern (#5629, funnel completeness) requires the per-request resolver to resolve
**all** per-slug observe state, not just the store. Two shapes were considered:

- **(A) A `slug → registry/pending/services` side-map built in `main.rs`** (the #930 addendum's
  alternative). Rejected: it is a *second parallel funnel*, re-opening the vnc-034 #4974
  ceremonial-funnel guard — a resolver that can resolve a store while a side-map disagrees is
  exactly the N=1-green / N=2-broken bypass vnc-038 deleted. SR-01 also flags it as the wrong
  place for hot-path resolution.
- **(B) New methods on the existing `Arc<dyn StoreResolver>`**, beside `resolve_store` /
  `adapter_for`, resolving from the **same** `slug → ProjectEntry` map. Chosen.

The key is already parsed once in `route_observe` Step 0 (`handlers.rs:51`) and reused, so
per-request resolution adds no parse and no new error surface (`UnknownProject` already 404s
upstream of the write).

### Decision

Extend the `StoreResolver` trait (`seam.rs:124`) with three methods, resolving from the same
map `resolve_store` reads, each an O(1) `HashMap` lookup + `Arc::clone` (SR-01 — no I/O, no
lock, no DB, same cost class as `resolve_store`):

```rust
fn registry_for(&self, key: &ProjectKey) -> Result<Arc<SessionRegistry>, RouteError>;
fn pending_for(&self, key: &ProjectKey)  -> Result<Arc<Mutex<PendingEntriesAnalysis>>, RouteError>;
fn services_for(&self, key: &ProjectKey) -> Result<ServiceLayer, RouteError>;
```

`ProjectEntry` (`project_resolver.rs:50`) gains three fields — `session_registry`,
`pending_entries_analysis`, `services` — `Arc::clone`d from the assembled `server` inside
`from_server` **before** `server` moves into `McpAdapter::new`. This makes the resolver hand
back the **same instances** the slug's `UnimatrixServer` (the MCP read path) holds: write and
read converge on one instance per slug **by construction**. No trait default impl (mirrors
`adapter_for`'s deliberate no-default — a default would re-admit the bypass).

`ObserveContext` is reshaped to `{ resolver, embed_service, server_version }` — the three global
handles are **deleted**, and `route_observe` resolves them per-request from the already-parsed
`key`, right after `resolve_store(&key)`:

```rust
let store    = observe_ctx.resolver.resolve_store(&key)?;
let registry = observe_ctx.resolver.registry_for(&key)?;   // A's registry
let pending  = observe_ctx.resolver.pending_for(&key)?;
let services = observe_ctx.resolver.services_for(&key)?;
dispatch_request(hook, &store, …, &registry, &pending, &services, …);
```

The vestigial `ObserveContext.vector_store` / `adapt_service` fields (unused `_`-params in
`dispatch_request`, `listener.rs:777/779`) are deleted in the same pass (AC-09; see ADR-002).
An `Err` from a `*_for` method after the store already resolved is a boot-wiring contradiction
(foreclosed by ADR-003), mapped to `500`, never `404`.

### Consequences

- **Easier:** #930 fixed and the F2 cross-slug transcript fold + P2 knowledge-read leak dissolve
  **by construction** — each slug's `take_transcripts_for_feature` and observe-path reads touch
  only that slug's instances. One funnel, one isolation proof (inherits MCP's isolation for
  free, like vnc-038). Tick contexts (`main.rs:1237`) already clone
  `input.server.session_registry`, so they become correct with zero change.
- **Easier:** any future per-slug observe handle is a new `*_for` method on the one funnel, not
  a new side-map — the #4974 guard stays closed.
- **Harder:** the `StoreResolver` trait grows 3 methods with no default, so every impl
  (including the ~4 test doubles at `tests.rs:1982/2004/2472/2651`) must implement them —
  intentional, forcing resolve/dispatch agreement. `ProjectEntry` grows 3 handle fields
  (bounded `Arc` clones; per-slug hot caches still live in the server).
- **Neutral:** hot-path cost is 3 extra `HashMap`+`Arc::clone` per observe call — negligible,
  same class as the existing store resolve (SR-01 satisfied; architect states the cost class
  explicitly).

Related: vnc-038 ADR-003 (#5082, the store funnel this completes), vnc-034 #4974 (ceremonial-
funnel guard), pattern #5629 (funnel completeness), ADR-002 (construction parity that populates
the entries), ADR-003 (boot assertion that pins the convergence).
