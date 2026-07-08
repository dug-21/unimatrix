# Agent Report — vnc-046-agent-1-pseudocode

**Role:** Pseudocode specialist (Session 2 Stage 3a)
**Feature:** vnc-046 — Per-slug state isolation for the cloud (HTTPS) observe path

## Deliverables

Per-component pseudocode under `product/features/vnc-046/pseudocode/`:
- `OVERVIEW.md` — component interaction, write/read/config/boot data flow, shared types, the
  ObserveContext reshape, field-census classification table, sequencing constraints, open questions.
- `resolution-funnel.md` — `StoreResolver` 3-method extension (no default), error boundary, hot-path
  cost, test-double obligation (R-06).
- `project-resolver.md` — `ProjectEntry` 3 new fields, `from_server` clone-before-move,
  `MultiProjectRouter` method impls.
- `observe-context.md` — `ObserveContext` reshape to 3 fields, `main.rs` construction update, deletions.
- `observe-handler.md` — `route_observe` per-request resolution, 500-not-404 (R-14),
  `dispatch_request` param removal + ~100 call-site blast radius.
- `project-provisioner.md` — `build_project_server` P1/P2/P3 construction parity, 3 params-at-end,
  `main.rs:1204` call-site update.
- `boot-assertion.md` — `assert_per_slug_isolation` + exhaustive field census (no `..`), sequencing.
- `isolation-suite.md` — bidirectional N≥2 behavioral suite structure + mandatory coverage table.

## Load-bearing constraints honored

- registry↔transcript_hold constructed as a pair inside `build_project_server`, before the
  `main.rs:1229` tick clone (F1/SR-03).
- 3 resolution methods ON the `StoreResolver` trait, no default impl, resolving from the same
  `slug → ProjectEntry` map, no side-map (FR-12, #4974 guard).
- 500-not-404 for post-`resolve_store` `*_for` errors (R-14).
- Exhaustive `UnimatrixServer` field census, no `..` (ADR-003), full field list enumerated from
  `server.rs:197-289`.
- Test doubles resolve `*_for` from their own `resolve_store` map, never fresh/global (R-06).
- Clone-before-move in `from_server` (convergence by construction, pinned by boot `Arc::ptr_eq`).

## Open questions / gaps (flagged, not papered over)

1. **Boot-assertion vs `from_servers` move.** ADR-003's `assert_per_slug_isolation(input:
   &ProjectServerInput, ...)` collides with `from_servers` consuming the inputs before the resolver
   exists, while the `Arc::ptr_eq` check needs both sides at once. Recommended: capture a per-slug
   `IsolationProbe` (Arc clones) in the pre-move loop, assert after the router is built — refines the
   param from `&ProjectServerInput` to `&IsolationProbe`. **Needs architect sign-off.**
2. **Per-slug signature scanner.** The daemon builds `SessionRegistry` with
   `.with_signature_scanner(...)` (`main.rs:852`); ADR-002's construction-parity param list does not
   include a scanner. Confirm whether FR-9 `signal_class_counts` needs a per-slug scanner or derives
   from `transcript_signal_class_names` alone. Did not invent a param.
3. **`categories` classification.** NFR-5 calls it global; the code threads per-slug `slug_categories`
   (`main.rs:1183`). Census must classify consistent with the code — flag for census author.
4. **#800 fixture ownership (SR-08).** N≥2 suite + INV-C proof reuse the OPEN #800 fixture (owner
   unconfirmed). Confirm before building INV-C fixtures.

## Knowledge Stewardship

- **Queried:** `context_search` (pattern) "per-slug isolation resolver funnel patterns" → #5172 (N=2
  model-free isolation), #5082 (vnc-038 ADR-003 per-slug observe on funnel), #5347 (bidirectional N×M
  gate), #5170 (per-slug ExtractionContext outside handle bundle); `context_search` (decision,
  topic vnc-046) → #5630 (ADR-001), #5634 (ADR-005), #5633 (ADR-004). Read all five ADR files,
  ARCHITECTURE, SPECIFICATION, RISK-TEST-STRATEGY, IMPLEMENTATION-BRIEF, and grounded every signature
  against source (`seam.rs`, `project_resolver.rs`, `router.rs`, `handlers.rs`, `http_provision.rs`,
  `main.rs`, `server.rs`, `session.rs`, `transcript_hold.rs`, `uds/listener.rs`).
- **Deviations from established patterns:** none. Pseudocode follows governing pattern #5629
  (construction parity + funnel completeness + `Arc::ptr_eq` boot guard) and #5348/#5172 (bidirectional
  N≥2 test shape). The one structural addition (`IsolationProbe`) is a mechanical reconciliation of the
  ADR-003 signature with the existing `from_servers` move semantics, flagged as OQ-1 for sign-off, not
  a pattern deviation.
- **Stored:** nothing — read-only tier. No novel cross-feature knowledge emerged.
