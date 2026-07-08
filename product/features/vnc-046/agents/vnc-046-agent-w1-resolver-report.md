# vnc-046 Wave 1 — resolution-funnel + project-resolver (agent report)

**Agent:** vnc-046-agent-w1-resolver
**Wave:** Stage 3b Wave 1 of 4 (additive foundation)
**Components:** resolution-funnel (`StoreResolver` trait) + project-resolver (`MultiProjectRouter` / `ProjectEntry`)

## Scope delivered

Extended THE single per-request resolution funnel to resolve all per-slug observe
state (registry / pending / services), not just the store — one funnel, one map, no
side-map (#4974 guard), no trait default impl (FR-12 / R-06).

### Files modified
- `crates/unimatrix-server/src/http/router/seam.rs` — added `registry_for` /
  `pending_for` / `services_for` to `trait StoreResolver` (NO default impl — a default
  re-admits the split-brain bypass). Imports: `std::sync::Mutex`, `SessionRegistry`,
  `PendingEntriesAnalysis`, `ServiceLayer`.
- `crates/unimatrix-server/src/http/router/project_resolver.rs` — `ProjectEntry` gains
  `session_registry` / `pending_entries_analysis` / `services`. `from_server`
  `Arc::clone`s all three off `server` **before** it moves into `McpAdapter::new`
  (convergence-by-construction, R-03 — ordering enforced by the borrow checker). Impl'd
  the 3 methods on `MultiProjectRouter`: O(1) `HashMap` lookup + `Arc::clone` from the
  SAME `slugs` map `resolve_store` reads; `UnknownProject` for an unregistered slug.
- `crates/unimatrix-server/src/http/router/tests.rs` — updated all 4 `StoreResolver`
  test doubles + added 3 unit tests.

### Test doubles (R-06 — fail-closed, never a fresh/global handle)
- `EmptyResolver`, `CountingResolver` — `resolve_store` is `UnknownProject` for every
  key; the 3 methods resolve from that same empty domain → `UnknownProject`. Mint nothing.
- `StubProjectRouter` — seam-only, models the store funnel only, holds no observe-state
  handles; fails closed with `UnknownProject` (unreachable in its route-grammar tests)
  rather than fabricate a handle.
- `RecordingResolver` — total delegation to the wrapped real `MultiProjectRouter`
  (`self.inner.*`), so resolution still comes from the one real per-entry map.

### New unit tests (per test-plan/resolution-funnel.md + project-resolver.md)
- `test_registry_for_resolves_same_instance_as_server` — `Arc::ptr_eq` proof that
  `registry_for`/`pending_for` hand back the slug server's OWN handle (clone-before-move,
  R-03); `services_for` resolves for a registered slug.
- `test_star_for_unknown_slug_is_unknown_project` — all three return `UnknownProject`
  for an unregistered slug (R-14 domain).
- `test_registry_for_n2_slugs_are_distinct` — N=2: A's registry `!ptr_eq` B's (#4974).

## Results
- **Crate builds green:** yes (`cargo build -p unimatrix-server` — Finished, 0 errors).
- **Tests:** `cargo test -p unimatrix-server --lib` → **4512 passed, 0 failed, 1 ignored**
  (3 new tests included).
- **Clippy:** my touched files are clean. 2 pre-existing `repeat().take()` warnings remain
  in `mcp/response/verbosity.rs` (NOT mine — out of scope, not touched).
- **fmt:** ran `cargo fmt`; reverted out-of-scope fmt churn on
  `mcp/edge_write_delete_agent_tests.rs` (I did not modify it).

## Issues / flags for later waves
- **`ObserveContext` still holds the OLD shape** (`vector_store`, `adapt_service`,
  `session_registry`, `pending_entries_analysis`, `services`) — the `observe_ctx_over`
  test helper (tests.rs ~2671) and `route_observe`/`dispatch_request` still consume them.
  That reshape is the observe-context / observe-handler wave (`http/router.rs`,
  `handlers.rs`). FLAGGED, not touched — my new resolver methods are additive and do not
  require it to compile.
- **Runtime-flavor trap (stored as pattern #5637):** unit tests building
  `make_server()` / `from_servers()` MUST use `#[tokio::test(flavor = "multi_thread")]`;
  plain `#[tokio::test]` compiles but panics ("can call blocking only when running on the
  multi-threaded runtime") from per-slug registry construction. Later waves adding
  server/router unit tests should apply the same flavor.
- No blockers for downstream waves.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_search (pattern) — surfaced ADR-001
  (#5630), the #4974 side-map guard, #5629 construction-parity governing pattern, and the
  crt-056 config-parity idiom. Applied: no side-map, no trait default, clone-before-move,
  fail-closed doubles.
- Stored: entry #5637 "make_server/from_servers unit tests need #[tokio::test(flavor =
  \"multi_thread\")]" via /uni-store-pattern (novel runtime-invisible trap hit while adding
  the unit tests).
