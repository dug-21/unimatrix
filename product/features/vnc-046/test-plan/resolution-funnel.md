# Test Plan — resolution-funnel (`StoreResolver` trait)

Source: `http/router/seam.rs`. Adds `registry_for` / `pending_for` / `services_for` beside
`resolve_store` / `adapter_for` — **no default impl** (a default re-admits the bypass, ADR-001).
Risks: R-06 (test-double bypass), R-11 (hot-path), R-14 (error mapping).

## Unit Test Expectations

Vehicle: `http/router/tests.rs` (in-crate) + production-resolver pins in
`project_routing_integration.rs`.

1. **`test_registry_for_returns_ok_for_registered_slug`** — with a `MultiProjectRouter` holding
   slug A, assert `registry_for(&ProjectKey::Slug("A"))` returns `Ok(Arc<SessionRegistry>)`.
   Same for `pending_for` (→ `Arc<Mutex<PendingEntriesAnalysis>>`) and `services_for`
   (→ `ServiceLayer`).
2. **`test_registry_for_unknown_slug_returns_unknown_project`** — unregistered key returns
   `Err(RouteError::UnknownProject)` for all three methods (same domain as `resolve_store`).
3. **`test_star_for_resolves_same_map_as_resolve_store`** (R-06 core) — for a given key, the
   handle returned by `registry_for` is the SAME instance the entry's `resolve_store`/adapter
   path holds. Assert with `Arc::ptr_eq` against the `ProjectEntry`'s stored handle — the
   funnel must NOT mint a fresh or global handle.
4. **`test_no_trait_default_impl`** (compile-level, AC/ADR-001) — documented in the test-double
   audit: because there is no default impl, adding the methods forces every impl to define them;
   verify (review + the doubles below compile only when they implement all three).

## Test-Double Audit (R-06 — Critical for harness integrity)

Every `StoreResolver` double at `tests.rs:~1982/2004/2472/2651` MUST implement the 3 methods by
resolving from its **own** `slug → entry` map — the same source its `resolve_store` reads —
returning the entry's stored handle. A double that returns a freshly-minted or shared-global
registry silently makes the harness pass while production would split-brain (the bypass moves
into the test infra).

- Assertion per double: no `*_for` may construct `SessionRegistry::new()` / `ServiceLayer`
  fresh, or return a module-global handle. Each returns `self.entries[key].<handle>.clone()`.
- Back-stop: the behavioral suite runs against the **production** `MultiProjectRouter`
  (isolation-suite.md), so a lenient double cannot green the primary gate.

## Hot-Path Expectations (R-11 / NFR-1 — SR-01)

5. **Cost-class review assertion** — each `*_for` is one `HashMap` lookup + `Arc::clone`
   (`ServiceLayer::clone` = a handful of `Arc::clone`s). No lock held across resolution, no I/O,
   no DB, no `ServiceLayer` reconstruction. Verified by code/diff review against the stated cost
   class of `resolve_store`; called out in the RISK-COVERAGE-REPORT.
6. **`test_project_key_parsed_once`** — `route_observe` parses `ProjectKey` once (Step 0) and
   reuses it across `resolve_store` + all three `*_for` calls (covered end-to-end in
   observe-handler.md; noted here as the funnel's caller contract).

## Error Boundary (R-14)

7. `RouteError` domain unchanged (`UnknownProject`, `InvalidSlug`). The **mapping** of a
   post-`resolve_store` `Err` to `500` (not `404`) lives in `route_observe` — tested in
   observe-handler.md. The funnel itself only returns the `RouteError`; it must never panic.

## Edge Cases
- Unknown slug → `UnknownProject` (uniform across all `*_for`).
- The three methods must agree on registration: if `resolve_store` resolves, all three resolve
  (no partial registration) — asserted by the wiring-pin (boot-assertion.md) and #3 above.

## Coverage Trace
| Risk | Test |
|------|------|
| R-06 | #3, test-double audit, production-resolver `ptr_eq` back-stop |
| R-11 | #5, #6 |
| R-14 | #2 (domain), mapping in observe-handler.md |
