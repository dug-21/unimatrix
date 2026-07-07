# Test Plan — observe-context (`ObserveContext` reshape)

Source: `http/router.rs`. Reshape to `{ resolver, embed_service, server_version }`; DROP the 3
flat global handles (`session_registry`, `pending_entries_analysis`, `services`) + the 2
vestigial fields (`vector_store`, `adapt_service`). Risks: R-09 (no new flat global handle),
AC-09 (vestigial deletion).

## Unit / Structural Test Expectations

1. **`test_observe_context_has_no_global_state_handles`** (AC-09, grep + compile) — assert
   `ObserveContext` contains exactly `resolver`, `embed_service`, `server_version`. No
   `session_registry`, `pending_entries_analysis`, `services`, `vector_store`, or `adapt_service`
   field. Enforced by:
   - a grep-gate over `http/router.rs`: zero `vector_store` / `adapt_service` references;
   - `cargo build -p unimatrix-server` compiles with the fields and their `dispatch_request`
     `_`-params removed (a dangling reference blocks compile — verify no live reader remains).
2. **Regression guard (R-09):** review assertion — no new flat global handle may be added to
   `ObserveContext`. The only per-slug state reaches the handler via `resolver.*_for(&key)`, never
   a struct field. Any future field added as a flat global re-flattens the P2 knowledge-read leak.
   State this explicitly in the coverage-enumeration (isolation-suite.md).

## Integration Expectations

3. `route_observe` builds/consumes the reshaped `ObserveContext` and resolves per-slug state from
   `key` (observe-handler.md #1). The behavioral suite is the enforcement: if a global handle
   sneaks back in, cross-slug isolation (INV-K2/INV-T2) fails behaviorally.

## Blast Radius
- Deleting the 3 handles + 2 vestigial fields ripples to `route_observe` construction sites and
  `dispatch_request` call sites in `uds/listener.rs` (the two `_`-unused params removed). Verify
  no live reader of the removed fields remains in either transport path; UDS/stdio construction
  must stay behavior-identical (NG-4/NFR-4).

## Coverage Trace
| Risk / AC | Test |
|-----------|------|
| AC-09 | #1 (grep + compile) |
| R-09 regression | #2, behavioral back-stop in isolation-suite |
