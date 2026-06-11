# Agent Report — vnc-034-agent-4-default-resolver

**Component:** DefaultResolver (Wave 1, C4 Wave-1 resolver)
**Status:** Complete

## What I built

The real Wave-1 `StoreResolver` impl behind the C4 isolation seam (ADR-003/005):

- `struct DefaultResolver { store: Arc<Store> }` + `DefaultResolver::new(store)`
- `impl StoreResolver for DefaultResolver`:
  - `ProjectKey::Default` → `Ok(Arc::clone(&self.store))` — the one store, served THROUGH the funnel (FR-X5, AC-W1-X1)
  - `ProjectKey::Slug(_)` → `Err(RouteError::UnknownProject)` — slug routes parse but are inert in Wave 1; never a panic, never a fall-through to the default store (R-01 sc.3)

Total over `ProjectKey`. No I/O, no locking, no per-slug map — the degenerate-but-genuine case of the trait. The same struct/method is the identical resolver the local UDS install (path-hash store) and the cloud single-project install (one project store) use — local parity exercised on the very seam the cloud depends on (R-04 / NFR-10).

Implemented against the LOCKED `StoreResolver`/`ProjectKey`/`RouteError` types in `seam.rs` (sub-wave 1) — did not redefine them. Did NOT wire into main.rs (Sub-wave 3); module carries `#![allow(dead_code)]` mirroring the seam until then.

## Files modified

1. `crates/unimatrix-server/src/http/router/default_resolver.rs` — NEW (component, ~75 lines, well under 500)
2. `crates/unimatrix-server/src/http/router.rs` — added `pub(crate) mod default_resolver;` + `pub use default_resolver::DefaultResolver;` (allowed-unused until Sub-wave 3)
3. `crates/unimatrix-server/src/http/router/tests.rs` — appended 8 component tests to the cumulative vnc-034 seam test region

Touched ONLY my scope. Did not edit tls.rs / public_url.rs / main.rs / client_bundle.rs / seam.rs's locked types.

## Tests (per test-plan/default-resolver.md)

- `test_default_resolver_returns_the_one_store` (AC-W1-X1, Arc identity)
- `test_default_resolver_same_arc_each_call` (one store, not re-opened per request)
- `test_default_resolver_slug_returns_unknown_project` (R-01 sc.3)
- `test_default_resolver_slug_immediately_after_boot_is_deterministic` (edge case)
- `test_local_install_resolves_path_hash_store_through_seam` (AC-W1-X2 / NFR-10 — local-install regression)
- `test_local_and_cloud_single_project_byte_identical_route` (SR-08, no cloud-only branch)
- `test_default_resolver_is_the_same_trait_as_wave2_resolver` (R-04 sc.2 — path-hash behind the trait, no parallel cloud path)

**Result:** all DefaultResolver tests pass; `cargo test -p unimatrix-server router` → 88 passed / 0 failed. `cargo fmt` applied. `cargo clippy -p unimatrix-server --lib` → 0 warnings referencing default_resolver.

## Issues / blockers

- **Transient cross-agent build state (not mine, resolved):** the working tree carries other sub-wave agents' uncommitted work (`cert_provisioner.rs`, `client_bundle.rs`, `Cargo.toml` adding `rcgen`/`base64`). Initial build hit `E0432: unresolved import rcgen` because the lockfile/index was stale for the newly-added dep. A `cargo update` refreshed the index and the workspace builds clean. No action needed from me — flagging for the leader that the sibling Cargo.toml deps (`rcgen`, `base64`) must land with their lockfile update.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced ADR-005 (#4949), ADR-003 (#4950), and the deferred-seam pattern (#4957/#4869); applied the "Wave-1 seam present-but-unwired, allow(dead_code) until Sub-wave 3" convention.
- Stored: entry #4958 "Store/SqlxStore has no PartialEq: assert on error path, not Result<Arc<Store>>" via context_store (pattern) — compile-time trap (E0369) hit while writing the resolver tests; fix is `.expect_err()` + `Arc::ptr_eq`, matching the existing seam-test idiom.
