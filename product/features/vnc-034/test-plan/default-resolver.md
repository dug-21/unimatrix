# Test Plan — DefaultResolver

> `crates/unimatrix-server/src/http/router.rs`. Wave-1 `StoreResolver` impl: `DefaultResolver { store: Arc<Store> }` — returns `store` for `ProjectKey::Default`, `RouteError::UnknownProject` for any `Slug`. **Lead risks: R-01 (Critical), R-04 (local/cloud parity).**

## AC-IDs covered
AC-W1-X1 (returns the one store through the seam), AC-W1-X2 (local-UDS parity — the local-install regression test, NFR-10), AC-CT-C4 (additive base).

---

## Unit tests (Rust)

### R-01 — Wave-1 store served THROUGH the seam (FR-X5, AC-W1-X1)
- `test_default_resolver_returns_the_one_store` — `resolve_store(ProjectKey::Default)` returns the single configured `Arc<Store>`.
- `test_default_resolver_same_arc_each_call` — repeated `Default` resolutions return clones of the **same** underlying store (one store, not a re-opened handle per request).
- `test_default_resolver_slug_returns_unknown_project` — `resolve_store(ProjectKey::Slug(_))` → `Err(RouteError::UnknownProject)`; never the default store, never a panic. (Pairs with the slug-router swap test for the full R-01 proof.)

### R-04 — local-UDS path-hash parity (AC-W1-X2, NFR-10 — IN the Wave-1 set, NOT deferred)
> This is the non-negotiable local-install regression test. It lives in the Wave-1 suite.
- `test_local_install_resolves_path_hash_store_through_seam` — construct `DefaultResolver { store: <path-hash store from compute_project_hash(/data)> }` exactly as the local UDS daemon does; assert a request resolves `ProjectKey::Default` and reaches the path-hash store **through `resolve_store`**, the same code path the cloud single-project alias uses (SR-08, A2).
- `test_path_hash_logic_unchanged_behind_trait` — assert the ADR-004 path-hash derivation (`SHA-256(canonical_project_root)[..16]`) is unchanged and lives behind the same `StoreResolver` trait as the (Wave-2) slug resolver; the slug never leaks into the local path, the path-hash never leaks into a cloud slug.
- `test_local_and_cloud_single_project_byte_identical_route` — the local UDS request and the cloud single-project request both traverse `/v1/tools/...` → `ProjectKey::Default` → `resolve_store` → the one store: assert the route is byte-identical (no cloud-only branch). (SR-08.)

## Integration (regression baseline — see OVERVIEW §4.1)
- infra-001 `smoke` + `lifecycle` over stdio: prove that inserting `DefaultResolver`/`SlugRouter` between dispatch and store did not break existing stdio tool dispatch, store/retrieval, or restart persistence. (Local-mode regression at the system level.)

## Edge cases (assigned here)
- A `Slug` request immediately after boot (before any Wave-2 config could exist) → `UnknownProject`, deterministic.

## Concrete assertions
The parity test (AC-W1-X2) asserts the **same function** (`resolve_store`) is the entry for both local and cloud single-project — a call-graph/source assertion, not just two passing behaviors. "Same code path" is the load-bearing claim of NFR-10.
