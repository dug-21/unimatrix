# Component: resolution-funnel (`StoreResolver` trait)

**Source:** `crates/unimatrix-server/src/http/router/seam.rs`
**ADR:** ADR-001 · **FR:** FR-12 · **Risks:** R-06, R-11, R-14

## Purpose

Extend THE single per-request resolution funnel so it resolves **all** per-slug observe state, not
just the store. Today the trait exposes `resolve_store` + `adapter_for`. Add three sibling methods
that resolve registry, pending, and services from the **same** `slug → ProjectEntry` map. No parallel
side-map (the vnc-034 #4974 guard); **no trait default impl** (a default re-admits the bypass, exactly
as `adapter_for` is deliberately default-less at `seam.rs:149`).

## New Trait Methods (add to `trait StoreResolver`, `seam.rs:124`)

```
trait StoreResolver: Send + Sync + 'static {
    fn resolve_store(&self, key: &ProjectKey) -> Result<Arc<Store>, RouteError>;   // existing
    fn adapter_for(&self, key: &ProjectKey) -> Option<&McpAdapter>;                // existing

    // NEW — no default body (ADR-001). Same RouteError domain as resolve_store.
    fn registry_for(&self, key: &ProjectKey) -> Result<Arc<SessionRegistry>, RouteError>;
    fn pending_for(&self,  key: &ProjectKey) -> Result<Arc<Mutex<PendingEntriesAnalysis>>, RouteError>;
    fn services_for(&self, key: &ProjectKey) -> Result<ServiceLayer, RouteError>;
}
```

Import additions in `seam.rs`: `SessionRegistry` (`crate::infra::session`),
`PendingEntriesAnalysis` (`crate::server`), `ServiceLayer` (`crate::services`), `std::sync::Mutex`.

### Method contract (each method)
```
FUNCTION registry_for(self, key):
    // O(1): one HashMap lookup + Arc::clone. NO I/O, NO lock, NO DB (NFR-1 / SR-01).
    MATCH key:
        ProjectKey::Slug(s):
            entry = self.slugs.get(s)                    // same map resolve_store reads
            IF entry is None: RETURN Err(RouteError::UnknownProject)
            RETURN Ok(Arc::clone(entry.session_registry))
// pending_for: identical shape, returns Arc::clone(entry.pending_entries_analysis)
// services_for: identical shape, returns entry.services.clone()  (ServiceLayer: Clone of Arcs)
```
The concrete impl lives on `MultiProjectRouter` — see `project-resolver.md`. The trait only declares
the contract.

## Error Boundary (R-14)

The methods return `RouteError` on the **same domain** as `resolve_store` (`UnknownProject`). But in
`route_observe` the key has **already** resolved a store, so an `Err` here is a *boot-wiring
contradiction* (foreclosed by ADR-003's boot assertion), NOT a client error:
- Handler maps it to **500**, never 404, never panic (see `observe-handler.md`).
- `UnknownProject` for a genuinely unregistered slug still 404s **upstream, at `resolve_store`** —
  unchanged surface (NFR-3). The `*_for` methods are only reached after `resolve_store` succeeded.

## Hot-Path Cost (NFR-1 / R-11)

Each `*_for` = one `HashMap` lookup + `Arc::clone`. `ServiceLayer::clone` is a handful of
`Arc::clone`s (no reconstruction). Same cost class as the existing `resolve_store`. The `ProjectKey`
is parsed once (`handlers.rs` Step 0) and reused across all four resolve calls — no re-parse, no lock
held across resolution.

## Test-Double Obligation (R-06 — critical)

Adding 3 no-default methods forces **every** `StoreResolver` impl to implement them, including the
test doubles at `http/router/tests.rs` (~lines 1982/2004/2472/2651 per the brief). Each double MUST
resolve `*_for` from the **same** `slug → handle` map its own `resolve_store` reads — **never** a
freshly-minted or shared-global registry. A lenient double that returns a fresh/global handle
re-admits the split-brain *inside the harness* and greens a broken build. See `project-resolver.md`
for the production impl and `isolation-suite.md` for the audit gate.

## Key Test Scenarios (hints)

- Trait has no default body for the 3 methods → a `StoreResolver` impl omitting any of them is a
  **compile error** (proves the no-default discipline).
- `registry_for(Slug(unregistered))` → `Err(UnknownProject)` (same as `resolve_store`).
- Production resolver: `Arc::ptr_eq(registry_for(&slug)?, <slug server's session_registry>)` holds
  (wiring-pin — covered by boot-assertion + a unit; not in the behavioral crate).
- Cost review: no lock/IO/DB on any `*_for` path (NFR-1).
