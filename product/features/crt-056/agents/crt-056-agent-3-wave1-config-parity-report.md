# Agent Report — crt-056 Wave 1 (config-parity substrate)

Agent: crt-056-agent-3-wave1-config-parity
Scope: Wave 1 atomic, compiling change (ADR-001 + ADR-002 + Wave-1 half of daemon HTTP boot).

## Files modified
- `crates/unimatrix-server/src/server.rs`
- `crates/unimatrix-server/src/http_provision.rs`
- `crates/unimatrix-server/src/main.rs`
- `crates/unimatrix-server/src/uds/mcp_listener.rs`
- `crates/unimatrix-server/tests/project_routing_integration.rs` (one-line `, None` — compile-coupled call site only; no test logic changed)

## What was implemented

### 1. `UnimatrixServer::new` (ADR-001)
- Appended final param `services: Option<ServiceLayer>`.
- `Some(layer)` ⇒ use it verbatim; `None` ⇒ the EXISTING test-default body moved byte-for-byte into the `None` arm (size-1 pool, unloaded `NliServiceHandle::new()`, nli_top_k 20, nli_enabled false, `InferenceConfig::default`, `with_builtin_claude_code`, `ConfidenceParams::default`, empty `CategoryAllowlist`, `default_boosted_categories_set`).
- Single `Some`/`None` branch only — NO cloud-only branch (C-6). `effectiveness_state` extraction runs on both arms after `services` resolves (R-03 handle identity preserved).
- Updated every existing caller: appended `, None` to test/unit callers (server.rs x2, mcp_listener.rs, project_routing_integration.rs), and the stdio daemon path (main.rs ~1492).

### 2. `build_project_server` (ADR-002 + ADR-006)
- Appended 9 config-parity params at end: `rayon_pool`, `nli_handle`, `nli_top_k`, `nli_enabled`, `inference_config`, `confidence_params`, `categories`, `observation_registry`, plus `boosted_categories: &HashSet<String>` (the resolved-set 9th arg — see boosted_categories resolution below).
- Builds a config-driven `ServiceLayer` mirroring the daemon's own (main.rs:880-898) field-for-field with the threaded values; passes `Some(service_layer)`.
- Replaced the per-slug `CategoryAllowlist::new()` default (former line 181) with the threaded operator `categories`. `adapt_service` stays per-slug INDEPENDENT state (`AdaptConfig::default()`, ADR-006) — same config, not shared.
- `Arc::clone` of the ONE loaded `nli_handle` — NO `NliServiceHandle::new()` anywhere on this path (C-3, AC-2).
- `#[allow(clippy::too_many_arguments)]` added (pre-existing lint class on this function family; 15 pre-existing instances workspace-wide).

### 3. Daemon HTTP boot (Wave 1 half)
- Per-slug call site (main.rs ~1096): threads `Arc::clone`s / `&`-refs of the daemon's resolved values into `build_project_server`.
- Daemon's OWN server (main.rs ~922) switched to `Some(services.clone())` — same `Some(config-driven)` parity path as per-slug (C-6). `.clone()` keeps `services` alive for the legacy handle extraction (957-961) the Wave-1 tick still uses.
- Per the brief, the global-handle tick wiring (957-961 / 968-991) is LEFT RUNNING (Wave 2 retires it).

### G-2 / G-3 resolution (thin additive accessors, no new state)
Added three `pub` accessors on `UnimatrixServer` (server.rs) so the Wave-2 boot loop can build `PerSlugTickContext` off `input.server`:
- `service_layer(&self) -> &ServiceLayer` (G-2)
- `tick_metadata(&self) -> Arc<Mutex<TickMetadata>>` (G-2)
- `vector_index(&self) -> Arc<VectorIndex>` (G-3)
These reuse existing fields (`services`/`tick_metadata`/`vector_index` are `pub(crate)`/`pub`); no struct change, no new state. They are also the assertion surface used by the AC-6 tests now.

### boosted_categories (Gate 3a MUST-CONFIRM) — RESOLVED
Confirmed the daemon passes a RESOLVED value: `boosted_categories` is built from `config.knowledge.boosted_categories` (main.rs:681-686) and moved into the daemon ServiceLayer at main.rs:889 — NOT `default_boosted_categories_set()`. For true AC-1 category/domain parity I threaded the SAME resolved set as the explicit 9th appended param to `build_project_server` (a `&HashSet<String>`), and cloned it at line 889 (`boosted_categories.clone()`) so the binding survives to the later per-slug loop.

## Tests
- Added 2 component tests (server.rs, AC-6): `test_server_new_none_yields_test_defaults` (AC-6.1) and `test_server_new_some_uses_supplied_service_layer` (AC-6.3, R-03 handle-identity via `Arc::ptr_eq`).
- `cargo test -p unimatrix-server --lib`: **4220 passed, 0 failed, 1 ignored** (includes the 2 new tests; no regressions).
- `cargo build -p unimatrix-server --lib --bins`: clean (0 errors).
- Edited integration test target compiles+links (`--test project_routing_integration --no-run` OK).

### AC-1 (8-field) / AC-2 (Arc::ptr_eq shared model) component tests — NOT added here, by design
The field-by-field AC-1 parity + AC-2 `Arc::ptr_eq` tests require (a) an on-disk REGISTERED per-slug store, (b) the full resolved-config Arc set, and (c) observable `ServiceLayer` config accessors that do not exist publicly. That is exactly the Layer-2 / multi-slug N=2 harness reserved for Stage 3c (and forbidden to me by "do not modify integration tests"). Building isolated scaffolding for it would violate "extend existing fixtures, never create isolated scaffolding." The structural guarantees AC-1/AC-2 rest on are enforced at the type level by my impl: the 9 params are REQUIRED (a missing field is a compile error, not a silent default), and the per-slug path Arc::clones the one `nli_handle` with no `NliServiceHandle::new()` present (source-auditable). FLAGGED for the Stage-3c tester.

## Issues / blockers
- None blocking. NOTE: a full `cargo test -p unimatrix-server --no-run` / `--workspace` link step OOM-kills two heavy unrelated integration-test binaries (`cert_provisioner`, `bundle_codec`) with `ld ... signal 9` in this environment — a memory-pressure artifact, NOT a code error (lib + bins + the edited integration target all compile/link fine individually). Per-crate `--lib` is green.
- Pre-existing (not mine): `cargo fmt` wants to reformat several already-unformatted files on this branch (`http/router/.../tests.rs`, and two stray blocks at project_routing_integration.rs:663/682 that predate my one-line edit). I reverted the fmt-only noise to keep my diff scoped to crt-056; left the pre-existing unformatted lines untouched.

## A2 interior-immutability audit (Load-Bearing Item, R-04) — partial note for Wave 2
Not the focus of Wave 1, but observed while threading: `nli_handle` (`NliServiceHandle`) carries a loaded-model state machine; `inference_config`/`confidence_params` are plain config snapshots behind `Arc` (no interior mutability on the read path observed). The full type-level audit (`RwLock`/`Mutex`/`Cell`/`AtomicX`/unsynchronized cache) on the inference read path remains a Wave-2 delivery item — flagged, not closed.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing -- surfaced ADR-002 (#5165), ADR-001 (#5136), the params-at-end pattern (#2552), and the spawn_background_tick use-after-move pattern (#3779); all applied.
- Stored: entry #5169 "Threading daemon-resolved config into a LATER per-slug loop: pre-clone the by-value moves first" via /uni-store-pattern — captures the boosted_categories provenance trap, the two by-value-move keep-alive fixes (889 clone, 994 Arc::clone), and the daemon-vs-stdio identical-block disambiguation trap.
