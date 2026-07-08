# Gate 3b Report: vnc-046

> Gate: 3b (Code Review)
> Date: 2026-07-08
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | Matches per-component pseudocode; two documented departures (scanner-as-param, IsolationProbe param type) both anticipated in pseudocode/ADR with rationale |
| 2. Architecture compliance | PASS | ADR-001..005 honored: funnel completeness, construction parity, real boot assertion, no side-map, #925 not subsumed |
| 3. Interface implementation | PASS | No-default trait methods, clone-before-move, ObserveContext 3-field, boot assertion returns `Result`, dispatch_request vestigial params deleted |
| 4. Test case alignment | PASS | R-06 fail-closed doubles, R-14 500-not-404, R-03 boot assertion, hollow-signals guard, construction parity all covered by unit tests |
| 5. Code quality | PASS (1 WARN) | Builds clean; no stubs; no `.unwrap()`/`.expect()` in production; `seam.rs` at 509 lines crosses the 500 cap (WARN) |
| 6. Security | PASS | Slug allowlist-validated, single path-join (escape unrepresentable), serde error → 400, no secrets, zero dependency changes (audit surface == main) |
| 7. Knowledge stewardship | PASS | All 4 wave reports carry `## Knowledge Stewardship` with Queried + Stored entries (#5637–#5640) |

Build: `cargo build -p unimatrix-server` exit 0.
Clippy: `cargo clippy -p unimatrix-server --tests` exit 0 — 2 warnings, both pre-existing
`repeat().take()` in `mcp/response/verbosity.rs:192/208` (not vnc-046 code; noted, not failing).

## Detailed Findings

### 1. Pseudocode fidelity
**Status**: PASS
**Evidence**:
- `resolution-funnel.md` → `seam.rs:155-187`: three no-default methods `registry_for`/`pending_for`/`services_for` added beside `resolve_store`/`adapter_for`, exact signatures, `RouteError` domain, no trait default body.
- `project-resolver.md` → `project_resolver.rs:98-121`: `from_server` clones `session_registry`/`pending_entries_analysis`/`services` off `server` BEFORE `McpAdapter::new` consumes it (clone-before-move). Methods at 259-292 are total-over-`ProjectKey`, lookup + `Arc::clone`, `UnknownProject` otherwise — no `.unwrap()`, no panic.
- `observe-context.md` → `router.rs:76-92`: `ObserveContext` reshaped to exactly `{ resolver, embed_service, server_version }`; the 5 split-brain/vestigial fields removed.
- `observe-handler.md` → `handlers.rs:82-99, 170-181`: per-request resolution from the same `key`; `*_for` Err → `internal_error_response()` (500), 404 stays at `resolve_store`. `dispatch_request` (`listener.rs:775-784`) drops the two `_vector_store`/`_adapt_service` params.
- `project-provisioner.md` → `http_provision.rs:286-326`: P1 registry+hold+scanner triple + pending, P3 five config-snapshot fields, all set on `let mut server` before the `Ok(...)`.
- `boot-assertion.md` → `main.rs:150-239` + `server_field_census.rs`: real `Result`-returning assertion + exhaustive no-`..` census.

**Documented departures (both PASS, not FAIL)**:
- `build_project_server` takes a **4th** new param `signature_scanner: &Arc<SignatureScanner>` (compiled at the `main.rs:1357` call site) rather than the pseudocode's "3 params + internal `SignatureScanner::compile`". Rationale is in-code (`http_provision.rs:159-168`) and stored as pattern #5638: the function takes resolved config as explicit params, not the raw `r`, so compiling from `r.transcript_signals` belongs at the call site. The compile-error-at-call-site anti-Defect-1 property is preserved.
- `assert_per_slug_isolation` signature refined from ADR-003's literal `input: &ProjectServerInput` to `probe: &IsolationProbe` (`main.rs:119-137, 165`). This is the ratified OQ-1 param-type refinement flagged for architect sign-off in `boot-assertion.md`; the `Arc::ptr_eq` handles are identical instances captured pre-move.

### 2. Architecture compliance
**Status**: PASS
**Evidence**: No parallel side-map introduced (the #4974 guard) — all four resolves read the one `slugs` map. No trait default impls (ADR-001). Full construction parity in `build_project_server` mirroring `main.rs:830-994` (ADR-002). Boot assertion is a real runtime check, not `debug_assert`, compiled into release (ADR-003/NFR-2). `#925` untouched (ADR-005). Local UDS/stdio construction paths unchanged (NG-4); `listener.rs` net -178 lines.

### 3. Interface implementation
**Status**: PASS
**Evidence**: Trait methods have no default body (`seam.rs:171-187`) — every impl including the two test doubles (`tests.rs:2550, 2749`) must implement them, and they resolve from their own map (R-06 fail-closed). `from_server` ordering enforced by the borrow checker. `ObserveContext` is exactly 3 fields. `assert_per_slug_isolation` returns `Result<(), ServerError>` and is `?`-propagated at `main.rs:1438`. `dispatch_request` signature is the 9-param cleaned form; all ~call sites updated (compiles).

### 4. Test case alignment
**Status**: PASS
**Evidence**:
- R-06 / no-default / fail-closed doubles: `tests.rs` doubles at 2550/2749 resolve from their own map; `test_star_for_unknown_slug_is_unknown_project`, `test_registry_for_resolves_same_instance_as_server`, `test_registry_for_n2_slugs_are_distinct`.
- R-14 500-not-404: `test_post_store_star_for_err_maps_to_500_not_404`.
- R-03 real boot assertion: `test_assert_per_slug_isolation_fully_wired_returns_ok`, `_unwired_registry_returns_err`, `_unpaired_hold_returns_err`, `_unset_config_sentinels_return_err`, `test_registry_for_ptr_eq_slug_server_registry`.
- Hollow-signals / non-zero-count guard (unit half): `test_assert_per_slug_isolation_unset_config_sentinels_return_err` builds a server with empty class names + `declares_signals: true` and asserts the "empty despite declared" error — proving the P3 guard is not tautological (`declares_signals` from config `r`, `signal_class_names` from the built server field; they diverge exactly in the regression case).
- Construction parity: `test_build_project_server_sets_five_config_snapshot_fields`, `test_build_project_server_constructs_registry_hold_pair`, `test_pending_entries_analysis_constructed_per_slug`.
- Observe N2 isolation: `test_observe_per_slug_funnel_isolation_n2`, `test_observe_unregistered_slug_is_loud_404_not_default`.

Behavioral non-zero `signal_class_counts` and cross-slug INV-C/T/K suites are Stage 3c's owned crate (`isolation-suite.md`) — correctly out of scope for 3b unit review.

### 5. Code quality
**Status**: PASS (1 WARN)
**Evidence**: `cargo build -p unimatrix-server` exit 0. No `todo!()`/`unimplemented!()`/`TODO`/`FIXME` in added lines. No `.unwrap()`/`.expect()` in production paths — every `.expect(` in the diff is in `#[cfg(test)]` modules (test doubles, `construction_parity_tests`, `main_boot_assertion_tests`, `slug_config_tests`); the two source `.unwrap()` mentions are doc comments asserting their absence. `build_project_server` uses `map_err(...)?`; the scanner compile maps `ScannerError → ServerError::Config` and aborts that slug's provision loudly (R-10).

**WARN — 500-line cap**: `seam.rs` is now 509 lines (the feature's 3 trait-method declarations pushed it 9 over). The feature otherwise respected the cap by splitting new code into focused modules (`slug_config.rs` 189, `server_field_census.rs` 100, `construction_parity_tests.rs` 200, `main_boot_assertion_tests.rs` 275; `http_provision.rs` reduced to 348). The other >500-line files (`server.rs` 4443, `main.rs` 2406, `listener.rs` 9630, `tests.rs` 3110) are pre-existing monoliths not introduced by this feature — `listener.rs` was reduced by 178 lines. Non-blocking; recommend a follow-up to relocate the trait doc/impl split if `seam.rs` grows further.

### 6. Security
**Status**: PASS
**Evidence**: Slug is an allowlist-validated `ProjectSlug`; the single path-join `{base_dir}/{slug}/` cannot escape (AC-W2-R6, escape unrepresentable). `route_observe` validates at the boundary: invalid slug → 400, unknown → 404, oversize body → 413, serde error → 400 — no panic on malformed input. No hardcoded secrets. No shell/process invocation added. Zero `Cargo.toml`/`Cargo.lock` changes vs `main` → the `cargo audit` / CVE surface is identical to the base branch; no new dependency risk introduced by this feature.

### 7. Knowledge stewardship
**Status**: PASS
**Evidence**: All four wave reports contain a `## Knowledge Stewardship` block with `Queried:` (context_briefing/search/get) and `Stored:` entries — #5637 (multi_thread test flavor), #5638 (scanner compiles at call site), #5639 (mechanical positional-arg-removal scoping), #5640 (field-census must live in lib crate). #5640 precisely captures carried-item (a)'s rationale.

## Carried-Item Assessment

**(a) Census relocated from `main.rs` to lib-crate `server_field_census.rs`** — JUSTIFICATION SOUND, CLASS STILL CLOSED.
`main.rs` is a separate binary crate importing `unimatrix-server` externally; it can only see `pub` fields, but the exhaustive destructure needs every field including module-private `tool_router`/`server_info`. The census is a `#[path]` child module of `server` (`server.rs:4439`), so it names all fields; it is deliberately NOT `#[cfg(test)]`, so a new field breaks the RELEASE build too. The exhaustive no-`..` pattern (`server_field_census.rs:35-72`) is compiler-checked regardless of the fn ever being called. The whole "constructor-default never overwritten" class remains closed on the shipped binary.

**(b) `store_config`/`inference_config` covered by census + wiring-pin now, behavioral coverage deferred to 3c** — ACCEPTABLE AT THIS GATE.
These two fields have no clean runtime sentinel, so they are the documented AC-06 white-box exception (R-04). At 3b the wiring is verified present: `test_build_project_server_sets_five_config_snapshot_fields` pins all five snapshot fields, the census forces classification, and `test_registry_for_ptr_eq_slug_server_registry` is the value-pin home. Behavioral (cross-slug) observation is legitimately Stage 3c's concern.

**(c) Pre-existing `repeat().take()` clippy warnings in `verbosity.rs`** — NOTED, NOT FAILED.
Confirmed both at `mcp/response/verbosity.rs:192/208`, a file untouched by vnc-046 (which touches `http/router/*`, `http_provision*`, `main.rs`, `server.rs`, `uds/listener.rs`). Pre-existing; not introduced by this feature.

## Rework Required

None.
