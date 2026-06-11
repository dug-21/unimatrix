# Agent Report — vnc-034 Wave 2 / Stage 3b Wave 2 — ProjectRouter (funnel)

- **Agent ID:** vnc-034-wave-2-agent-4-project-router
- **Issue:** #727 (umbrella #733)
- **Component:** ProjectRouter — the Wave-2 `StoreResolver` impl (`MultiProjectRouter`)
- **Task type:** Report-only (implementation DONE; leader-verified, build green). No source modified, no cargo run.

## Summary

The Wave-2 funnel-elimination is implemented and shipped across two prior agents (no
report left by either; this report documents the committed/uncommitted result). The
single-funnel invariant is now genuinely load-bearing: per-key MCP dispatch lives INSIDE
the resolver via `StoreResolver::adapter_for`, the Wave-1 `let _store` discard is gone,
and the fixed single-project HTTP `ProjectRouter<ReqBody>` dispatcher behind the seam is
gone. Confirmed by reading; line citations below.

## Files created / modified

**Created**
- `crates/unimatrix-server/src/http/router/project_resolver.rs` — `MultiProjectRouter`
  (the Wave-2 `StoreResolver`), `ProjectEntry { store, adapter }` (per-slug isolated
  resources, FR-C3), and `ProjectServerInput { slug, store, server }` (the public
  listener-wiring input the binary crate uses without naming `pub(crate)`
  `ProjectEntry`/`McpAdapter`). `from_servers(...)` builds the map; `resolve_store` is the
  store funnel; `adapter_for` is the sole dispatch route. Built from the per-slug
  `UnimatrixServer`s the listener assembles from validated `[[projects]]` slugs.
- `crates/unimatrix-server/src/http/router/project_resolver/tests.rs` — unit tests
  (project-router test plan §A/§B/§C).

**Modified**
- `crates/unimatrix-server/src/http/router/seam.rs` — extended the `StoreResolver` trait
  with `adapter_for` (no default impl); rewrote `SlugRouter::route_mcp` to use the
  resolved store and dispatch via `adapter_for` (discard + fixed-adapter dispatch removed);
  `SlugRouter::new` now takes only the resolver.
- `crates/unimatrix-server/src/http/router/default_resolver.rs` — `DefaultResolver` gains
  an optional `adapter`; `with_adapter(...)` constructor; `adapter_for(&Default)` returns
  it (byte-identical Default path, AC-W2-R2 / AC-CT-C4). `new(...)` kept as a store-only
  resolver for tests (`adapter_for` → `None`).
- `crates/unimatrix-server/src/http/router.rs` — removed the old fixed
  `ProjectRouter<ReqBody>` MCP dispatcher; `PathRouter::new` now takes only
  `Arc<dyn StoreResolver>` + `ObserveContext`; `McpAdapter` gained `store` +
  `wraps_store()` for resolve/dispatch agreement; module wiring re-exports
  `MultiProjectRouter` / `ProjectServerInput`.
- `crates/unimatrix-server/src/http/mod.rs` — seam-surface re-exports.
- `crates/unimatrix-server/src/http/http_provision.rs` — `build_project_server(...)`
  per-slug subsystem assembly helper returning a `ProjectServerInput`.
- `crates/unimatrix-server/src/main.rs` — the single seam swap site (~L911): `[[projects]]`
  absent → `DefaultResolver::with_adapter`; present → `MultiProjectRouter::from_servers`;
  `/observe` resolves its store through the funnel at boot.

## FUNNEL CONFIRMATION (verified by reading — line citations)

1. **Wave-1 `let _store` discard is removed.** `seam.rs:290` binds `let store = match
   self.resolver.resolve_store(&key)` and the resolved handle is USED — passed to
   `wraps_store(&store)` at `seam.rs:324` (`debug_assert!`) and `let _ = &store`
   (`seam.rs:327`). No `let _store` anywhere in the file.

2. **The generic HTTP `ProjectRouter<ReqBody>` fixed-adapter fallback is removed.**
   `seam.rs:310` dispatches via `self.resolver.adapter_for(&key)` only; there is no
   `self.project_router.route_mcp` call. `router.rs` module header (`router.rs:330-344`)
   states the old single-project `ProjectRouter<ReqBody>` is GONE; `PathRouter::new`
   (`router.rs:140`) takes only `resolver` + `observe_ctx` (no `project_router` param);
   the catch-all arm (`router.rs:283-284`) dispatches solely through `slug_router`.

3. **`adapter_for(&ProjectKey)` is the SOLE MCP dispatch on the trait — no bypass-able
   default impl.** The trait method is declared with NO default body
   (`seam.rs:137-138`), with the deliberate-no-default rationale documented at
   `seam.rs:127-131`. Both impls provide it: `MultiProjectRouter`
   (`project_resolver.rs:217-222`) and `DefaultResolver` (`default_resolver.rs:115-120`).

4. **DefaultResolver returns its adapter so the Default path is byte-identical
   (AC-W2-R2 / AC-CT-C4).** `adapter_for(&Default)` → `self.adapter.as_ref()`
   (`default_resolver.rs:116-117`); `with_adapter` builds it (`default_resolver.rs:76-87`);
   main.rs uses `DefaultResolver::with_adapter` when `[[projects]]` is absent
   (`main.rs:913`). Same single store/adapter as Wave 1, now SELECTED via the funnel.

5. **`MultiProjectRouter` renamed to avoid shadowing the removed generic type (OQ-PR-2).**
   The resolver type is `MultiProjectRouter` (`project_resolver.rs:94`); the module header
   notes the generic `ProjectRouter<ReqBody>` is removed and the docs call this resolver
   "ProjectRouter" for fidelity.

## Per-slug isolation realized (AC-W2-R3)

- `ProjectEntry { store: Arc<Store>, adapter: McpAdapter }` per slug
  (`project_resolver.rs:46-53`); each entry's store/adapter are the slug's OWN resources.
- `resolve_store(Slug(s))` → that slug's store, `UnknownProject` for an unregistered slug,
  never a fall-through to default or another slug (`project_resolver.rs:199-210`).
- `adapter_for(Slug(s))` selects the SAME map's adapter (`project_resolver.rs:217-222`),
  so resolution and dispatch read one map and cannot diverge (asserted in the seam via
  `McpAdapter::wraps_store`, `router.rs:420-422`).
- Unit tests use `Arc::ptr_eq` for store identity because `Store` has no `PartialEq`
  (pattern #4958) — see `tests.rs:11` and the `make_slug_input` / `assert_agreement`
  helpers. `test_no_residual_fixed_adapter_path` (`tests.rs:196-269`) is the structural
  no-bypass proof: each slug's adapter wraps only its own store; an unknown key yields
  `adapter_for` `None`.

## Test status (leader-verified — NOT re-run here)

- Lib suite: **3976 passed**, 1 ignored. The new `project_resolver/tests.rs` cases pass.
- The eval-sweep flake and the token-concurrency flake are **pre-existing**, unrelated to
  this wave.

## Carry-forward / notes

- **Integration test DEFERRED to Stage 3c:**
  `crates/unimatrix-server/tests/project_routing_integration.rs` (the real two-store
  `/v1/a` writes / `/v1/b` never sees it dispatch correctness instrument) is Stage 3c's per
  protocol — integration tests are Stage 3c. The unit suite proves the funnel structurally
  + at the resolver boundary; the two-store HTTP behavioral assertion lands in 3c.
- **`OBSERVE_PATH`** (`router.rs:39`) is a test-only-referenced const → a minor
  pre-existing-style dead-code warning; the crate already carries ~237 baseline warnings,
  so this is not a regression introduced by this wave.
- **For Wave 3 (ProjectRegistry / CLI):** `MultiProjectRouter` is built from the validated
  `[[projects]]` slugs via `from_servers`; `ProjectEntry` and `ProjectServerInput` live in
  `project_resolver.rs`; the per-slug `UnimatrixServer` is assembled by
  `http_provision::build_project_server`. The router OPENS the per-slug
  `/data/.unimatrix/{slug}/` store and fails loud if it is missing (C5, no auto-create) —
  **Wave 3's `register` is the sole creator of those data dirs.**

## Discrepancy check (code vs. locked design)

None material. Two faithful naming/return-type adaptations of the pseudocode, both
explicitly sanctioned by the flagged open questions:
- The resolver is built from assembled `UnimatrixServer`s via `from_servers` +
  `ProjectServerInput` (rather than the pseudocode's `from_registry` taking
  pre-built adapters) — the binary crate never names `pub(crate)` `McpAdapter`/`ProjectEntry`
  (the intent of OQ-PR-3). Behavior matches the design.
- `from_servers` returns `Result<Self, String>` (the duplicate-slug message) which main.rs
  maps to `ServerError::Config` (`main.rs:944`), matching the pseudocode's loud-startup-fail
  contract.

## Knowledge Stewardship

- **Queried:** `mcp__unimatrix__context_search(query: "StoreResolver per-slug adapter
  dispatch funnel seam", category: pattern, topic: vnc-034)` → **no results**.
- **Stored / Deviations:** Nothing novel to store. This wave extends the already-merged
  Wave-1 `StoreResolver` seam exactly per ADR-003 (per-slug routing inside the seam) and the
  Wave-2 funnel-elimination record; the store-identity test idiom (`Arc::ptr_eq`, `Store`
  has no `PartialEq`) is already captured as pattern #4958. No new gotcha surfaced that
  isn't already documented. No deviations from the locked design.
