# vnc-038 Agent Report — Component 7 Boot Wiring + ADR-004 Default-deletion completion

**Agent:** vnc-038-agent-3-boot-wiring
**ADRs:** ADR-003 (#5082), ADR-004 (#5083), ADR-006 (#5087) · pattern #5090

## Summary

Completed the ADR-004 `ProjectKey::Default` / `DefaultResolver` deletion that Wave-1
Component 5 started, and implemented Component 7 boot wiring. Because removing the
boot-bound `resolve_store(&ProjectKey::Default)` for observe leaves no key to resolve a
single store at boot, the ObserveContext + observe route (Component 6, ADR-003) had to
land in the same diff — they are inseparable from the Default deletion. The crate now
compiles green (lib + bins + lib unit tests).

## Files modified / deleted

- DELETED `crates/unimatrix-server/src/http/router/default_resolver.rs` (whole `DefaultResolver` type, ADR-004 §C)
- `crates/unimatrix-server/src/main.rs` — boot swap: removed `DefaultResolver` import + branch; empty `[[projects]]` now builds an empty-slug-map `MultiProjectRouter` + loud `tracing::error!("register a project to begin")` (AC-09/R-10), gated to the cloud HTTP served path (local STDIO :1158 / UDS :859 UNTOUCHED); `from_servers` now 3-arg; deleted boot-bound observe `resolve_store(&ProjectKey::Default)`; ObserveContext now holds `Arc<dyn StoreResolver>`.
- `crates/unimatrix-server/src/http/router.rs` — ObserveContext holds the resolver (removed `store`/`entry_store`); top-level `POST /observe` arm DELETED; new `route_observe` resolves per-request via `seam::parse_project_key` + `resolver.resolve_store` (ADR-003); removed `default_resolver` mod decl + re-export; removed dead `OBSERVE_PATH` const; refreshed module doc.
- `crates/unimatrix-server/src/http/mod.rs` — removed `DefaultResolver` re-export.
- `crates/unimatrix-server/src/http_provision.rs` — fixed `DefaultResolver`/`resolve_store(Default)` doc reference.
- `crates/unimatrix-server/src/infra/projects_config_tests.rs` — inverted `test_projects_absent_default_alias_unchanged` -> `test_projects_absent_no_default_alias` (asserts Slug candidate, not Default).
- `crates/unimatrix-server/src/http/router/tests.rs` — inverted ~30 Default refs: stub resolvers lose the `Default` arm (`DefaultLikeResolver`->`EmptyResolver`); grammar tests assert `/v1/tools/...`->Slug candidate and `/`,`/messages`->`Err(UnknownProject)`; whole `DefaultResolver` test block repointed to slug-keyed/empty-map stubs + an ADR-006 "path-hash is not a resolver key" assertion; `CountingResolver.last_was_default`->`last_was_slug`; mock `mock_dispatch_request` updated to per-slug observe; stale top-level `/observe` routing tests inverted.

## Build / tests

- `cargo build -p unimatrix-server`: **PASS** (lib + bins, 0 errors).
- `cargo test -p unimatrix-server --lib`: **4209 passed, 2 failed** — both pre-existing/out-of-scope:
  1. `http::router::seam::grammar_tests::test_parse_invalid_slug_rejected_at_edge` — in `seam.rs` (Component 5, NOT touched by me). Test-data defect: `trail-` (trailing hyphen) is genuinely accepted by the documented allowlist `^[a-z0-9][a-z0-9-]{0,62}$`. Flag for Component 5 owner.
  2. `eval::runner::sweep_tests::test_ac14_correlated_sweep_non_vacuous` — unrelated eval module; **flaky under full-suite parallelism, passes in isolation (verified 2x)**. Not a routing/Default change.
- All MY touched tests pass: router module 108/108, projects_config 22/22.
- `cargo fmt --check`: clean. `cargo clippy --lib`: zero warnings in any touched file (total warnings 25->24, removed dead `OBSERVE_PATH`).

## Scope notes / blockers

- Integration target `tests/project_routing_integration.rs` LEFT UNTOUCHED for Stage 3c (per spawn). It and `tests/client_bundle_e2e.rs` (bundle-codec component, `base_url`/v:1 field) remain red as integration targets — expected, not my scope.
- `git diff --name-only` also shows sibling-agent uncommitted Wave-1 files (`bundle.js`, `bundle-golden.json`, `remote-client.test.js`, `tests/fixtures/...`) — NOT mine; I ran NO git commands and touched none of them.
- Ran NO git operations (Delivery Leader owns git). Branch confirmed `feature/vnc-038`.
- Component 6 (observe-route) was folded into this diff out of necessity (the Default deletion forces it). Confirmed against `pseudocode/observe-route.md` Section A/B/C/D.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing + context_get #5083/#5082/#5087/#5090 -- found pattern #5090 (cross-component Default-deletion call-site audit) which enumerated the exact fan-out; ADR-003/004/006 bounded the observe + local-binding behavior.
- Stored: entry #5093 "Deleting ProjectKey::Default forces the observe handler off a boot-bound store onto a per-request resolver — the two changes are inseparable" via /uni-store-pattern (edges: Supports #5090, Supports #5082).
