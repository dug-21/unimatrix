# DefaultResolver — Wave-1 `StoreResolver` impl

> `crates/unimatrix-server/src/http/router.rs`. The Wave-1 resolver behind the `StoreResolver` trait (ADR-003). Realizes FR-X4/X5, R-04 (local/cloud seam parity), part of R-01. Returns the one store for `ProjectKey::Default`; `UnknownProject` for any `Slug`.

## Purpose

The single Wave-1 resolver. It makes the Wave-1 single store reachable ONLY *through* the seam (not around it — A4/FR-X5), and it is the IDENTICAL resolver the local UDS install uses (with its path-hash store) and the cloud single-project install uses (with the one project store). Same struct, same code path — local parity is exercised by the very seam the cloud depends on (R-04).

## Locked type + signature (from slug-router.md)

```rust
pub struct DefaultResolver { store: Arc<Store> }

impl DefaultResolver {
    pub fn new(store: Arc<Store>) -> Self { DefaultResolver { store } }
}

impl StoreResolver for DefaultResolver {
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>;
}
```

## Function: resolve_store

```
fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>:
    match key:
      ProjectKey::Default   => Ok(self.store.clone())     // Arc clone — the one store, served THROUGH the funnel
      ProjectKey::Slug(_)   => Err(RouteError::UnknownProject)
            // Wave 1: slug routes parse but are INERT. NOT a panic, NOT a silent fall-through to
            // the default store (R-01 sc.3). Wave 2 swaps in ProjectRouter which resolves Slug(_).
```

That is the whole resolver. No I/O, no locking, no per-slug map — the entire point is that Wave 1 is the degenerate-but-genuine case of the trait.

## Local / cloud parity (R-04 / FR-X4 — the load-bearing equivalence)

The SAME `DefaultResolver` is constructed in both deployment modes; only the injected `store` differs in provenance:

| Mode | Construction | Identity source |
|------|--------------|-----------------|
| Local UDS single-project | `DefaultResolver::new(path_hash_store)` where `path_hash_store` opened at `data_dir = ~/.unimatrix/{compute_project_hash(project_root)}` (ADR-004 #80, unchanged) | daemon path-hash (transport-derived) |
| Cloud single-project (Wave 1) | `DefaultResolver::new(the_one_project_store)` opened at `/data/.unimatrix/{hash}` | `/v1/tools/...` default alias (transport-derived) |

Both resolve `ProjectKey::Default` through the identical `resolve_store`. The path-hash assumption ("moving a project changes its hash") lives ONLY in how the local store is *opened* upstream — it never enters `resolve_store` and never leaks into cloud mode (A2/R-04 sc.2). The cloud slug (Wave 2) is a DIFFERENT resolver behind the SAME trait, never a path-hash leak.

## Initialization sequence

Constructed once during listener wiring (see slug-router.md wiring), after `open_store_with_retry` yields the `Arc<Store>`:
```
store    = open_store_with_retry(db_path).await?       // existing
resolver = Arc::new(DefaultResolver::new(store))       // this component
slug_router = SlugRouter::new(resolver, project_router)
```

## Data flow

- **Input:** `&ProjectKey`.
- **Output:** `Ok(Arc<Store>)` for `Default`; `Err(UnknownProject)` for `Slug`.
- **Holds:** one `Arc<Store>` (the sole write capability threaded to the edge — FR-X3).

## Error handling

Only `RouteError::UnknownProject` (for `Slug`). Total otherwise. No `.unwrap()`, no panic.

## Key test scenarios (hints for tester)

- `resolve_store(Default)` returns the injected store (Arc identity) (FR-X5, AC-W1-X1).
- `resolve_store(Slug(s))` returns `UnknownProject` for any slug — never the default store, never panic (R-01 sc.3).
- **Local-install regression (Wave-1 set, NOT deferred):** local UDS resolves its path-hash store through this SAME `resolve_store` seam (AC-W1-X2, NFR-10, R-04 sc.1) — the headline parity test.
- Swap drop-in: a stub `ProjectRouter` implementing `StoreResolver` substitutes at the `SlugRouter::new` call site with no other change (R-01 sc.2).
- Path-hash logic unchanged and lives behind the trait; slug never leaks into the local path (R-04 sc.2, source assertion).
```
