# vnc-034 Agent 6 — Seam Wiring Report

Closes the cross-file integration gap left by the Wave-1 file-scoping split: makes
`SlugRouter` the REAL per-request MCP edge so every MCP request flows
PathRouter -> SlugRouter -> resolve_store(Default) -> dispatch (AC-W1-X1/X3, AC-CT-C4).
Resolves the build-but-unwireable blocker documented by Agent 5 (#4962).

## Files modified

- `crates/unimatrix-server/src/http/router.rs` — `PathRouter` now holds
  `slug_router: SlugRouter<ReqBody>` (was a bare `ProjectRouter<ReqBody>`); `Debug`,
  `Clone`, and `PathRouter::new` updated accordingly. `PathRouter::new` now takes
  `(resolver: Arc<dyn StoreResolver>, project_router, observe_ctx)` and builds the
  `SlugRouter` internally (one call site, resolver is the sole Wave1<->Wave2 swap
  point). The MCP fall-through arm dispatches through `self.slug_router...route_mcp`
  instead of `self.project_router...route_mcp`. `/health` and `/observe` arms
  unchanged. Removed the placeholder `#[allow(unused_imports)]` on the
  seam/default_resolver re-exports (now have real production callers).
- `crates/unimatrix-server/src/http/router/seam.rs` — removed the placeholder
  `#![allow(dead_code)]` (line 18); updated the module doc to reflect that
  `SlugRouter` is now wired as the per-request edge.
- `crates/unimatrix-server/src/http/router/default_resolver.rs` — removed the
  placeholder `#![allow(dead_code)]` (line 25); updated the module doc.
- `crates/unimatrix-server/src/main.rs` — construct `DefaultResolver` as
  `Arc<dyn StoreResolver>` and pass it into `PathRouter::new`. The boot-time
  `resolve_store(&ProjectKey::Default)` is RETAINED only to acquire the `/observe`
  store handle (which is NOT on the per-request seam per ADR-005); MCP is now served
  THROUGH the per-request funnel. Removed the `SEAM INSERTION BLOCKER` comment.
- `crates/unimatrix-server/src/http/router/tests.rs` — added the per-request-funnel
  test block (3 tests, cumulative; no fragmentation).

NOT touched (per constraints): `tls.rs`, `public_url.rs`, `client_bundle.rs`,
`config.rs`, any JS. No new crates, no `unsafe`, no `.unwrap()`/`.expect()` in
non-test code, errors via `ServerError`.

## Tests — 91 passed / 0 failed (`router` filter)

New per-request-funnel tests (vnc-034 Agent-6 block, AC-W1-X1/X3):
- `test_path_router_mcp_edge_is_the_slug_router_seam` — STRUCTURAL (the load-bearing
  no-bypass guard): `PathRouter::new` accepts `Arc<dyn StoreResolver>` at the MCP
  edge. A reverted bypass (PathRouter holding a bare `ProjectRouter`) would take no
  resolver here and fail to compile — making the bypass loud, not silent (R-01 sc.1).
- `test_per_request_funnel_consults_resolver_with_transport_key` — BEHAVIORAL: a
  counting `StoreResolver` is consulted exactly once on the per-request path with the
  transport-derived `ProjectKey::Default` (no payload-named project — AC-W1-X3/FR-X2).
- `test_per_request_slug_rejected_at_funnel_not_default_store` — a slug request is
  rejected AT the funnel (`UnknownProject`), never silently served the default store
  (R-01 sc.3); the resolver call count proves no bypass.

Why not a full PathRouter->dispatch behavioral test: `SlugRouter::route_mcp` reaches
dispatch only through a real `ProjectRouter`, which requires a heavyweight
`UnimatrixServer` (no test helper exposed to the router test module). The
structural + counting-resolver pair proves the funnel is on the per-request path
(parse_project_key -> resolve_store BEFORE dispatch) without that cost — the dispatch
tail itself is covered by the existing MCP routing tests (`MockMcpAdapter`).

## Validation (container link-OOM limit respected — binary target NOT built)

- `cargo check -p unimatrix-server` — PASS (0 errors; 25 pre-existing lib warnings in
  eval/services/embed modules; none reference the touched files except the
  pre-existing `OBSERVE_PATH` test-only-constant warning at router.rs:39).
- `cargo build -p unimatrix-server --lib` — PASS.
- `cargo test -p unimatrix-server --lib router` — 91 passed, 0 failed.
- `cargo clippy -p unimatrix-server --lib` — no findings on the touched files
  (only pre-existing warnings in `anndists`/`unimatrix-engine` and the
  `OBSERVE_PATH` constant).
- `cargo fmt -p unimatrix-server` — applied; `--check` clean.
- Removing the `#[allow(dead_code)]`/`#[allow(unused_imports)]` placeholders surfaced
  NO new dead_code/unused warnings — confirming the seam is now genuinely wired.
- KNOWN CONTAINER LIMIT (not a failure): the full `bin "unimatrix"` link is OOM-killed
  by `ld` (signal 9); not attempted per spawn instructions.

## File size note

`router.rs` is 562 lines (HEAD baseline 548 — already over the 500 cap as a
pre-existing condition the prompt acknowledged). My net delta is +14 lines, almost
entirely the multiline `PathRouter::new` signature (forced by the added `resolver`
param) and condensed doc prose; the non-comment functional change is net-neutral
(added `slug_router` lines replace removed `project_router` lines). No fragmentation
was warranted — extracting impls would split a single-responsibility router for a
handful of lines.

## Issues / blockers

None. The Agent-5 blocker (#4962: PathRouter concrete over ProjectRouter, private
`route_mcp`, no insertion point) is fully resolved: the field/constructor/dispatch arm
now route through `SlugRouter`, and the no-bypass guarantee holds on the per-request
path, not just at boot.

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` -- surfaced #4962 (the exact blocker
  lesson) and #4957 (Wave-1-define / later-wire seam trap) plus ADR-003 (#4950).
  Applied #4962's resolution guidance directly.
- Stored: entry #4963 "Resolving the build-but-unwireable seam: generalize the host
  layer's constructor to accept the injected funnel, not just the concrete inner
  router" via context_store (pattern), with a `Supersedes` edge to #4962 (the
  blocker is now resolved, not just documented).
