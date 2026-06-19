# Gate 3b Report: crt-056

> Gate: 3b (Code Review)
> Date: 2026-06-19
> Result: PASS
> Validated against committed HEAD on branch `feature/crt-056` (HEAD `98aa72dd`).

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| 1. Pseudocode fidelity | PASS | All 7 components implemented per pseudocode; one DRY deviation (legacy `run_single_tick` not refactored) noted as WARN under detailed findings. |
| 2. Architecture compliance | PASS | ADR-001..006 followed. ServiceLayer sole handle-set owner; PerSlugTickContext borrows; global-handle wiring retired on the multi-project path. |
| 3. Interface implementation | PASS | All new signatures (BackgroundJob, Cadence, ResourceClass, SharedTickResources, PerSlugTickContext, additive `Option<ServiceLayer>`, params-at-end threading) match the Integration Surface. |
| 4. Test case alignment | PASS | Unit/structural tests match the component test plans; AC-6, registry-order, per-slug counter, handle-identity (Arc::ptr_eq) all present and passing. N=2 behavioral trio (AC-3/4/5) deferred to 3c by protocol. |
| 5. Code quality | PASS | Builds clean; no stubs/`todo!`/`unimplemented!`/`.unwrap()` in non-test crt-056 code. New files all <500 lines. Pre-existing >500-line files unchanged in nature. |
| 6. Security | PASS | No new external input surface; no `unsafe`, no hardcoded secrets; DAEMON_SLUG validated via `ProjectSlug::try_from`. |
| 7. Knowledge stewardship | PASS | Both rust-dev reports (wave1, wave2) carry complete `## Knowledge Stewardship` blocks with Queried + Stored entries. |

## Detailed Findings

### 1. Pseudocode fidelity — PASS (with WARN on DRY)
**Evidence**: Each pseudocode component maps to live code:
- `unimatrix-server-new.md` → `server.rs` additive `services: Option<ServiceLayer>`; `None` arm holds the prior test-default body byte-for-byte (NLI off, size-1 pool, `InferenceConfig::default`, `ConfidenceParams::default`, empty `CategoryAllowlist`, unloaded `NliServiceHandle`).
- `build-project-server.md` → `http_provision.rs` 8 config Arcs + `boosted_categories` threaded (the flagged 9th thread was resolved exactly as the pseudocode's `boosted_categories` note prescribed); per-slug `CategoryAllowlist::new()` default removed.
- `daemon-http-boot.md` → `main.rs` threads resolved Arcs, collects `Vec<PerSlugTickContext>` (daemon context + per-slug), builds `SharedTickResources` once, drives `spawn_per_slug_tick`, and **deletes** the legacy `957-961`/`968-991` global-handle extraction on the HTTP daemon path.
- `per-slug-tick-context.md` → `background/job.rs::PerSlugTickContext::from_service_layer` (handles are `Arc::clone`s of `*_handle()` accessors; never freshly constructed).
- `background-job-seam.md` → `background/job.rs` (trait + Cadence + ResourceClass + SharedTickResources + registry) and `background/jobs.rs` (9 jobs delegating to existing/extracted ops; no logic copied into the jobs).
- `per-slug-tick-loop.md` → `background/tick_loop.rs` (serial `run_per_slug_tick_pass`, per-slug `next_tick()`, supervisor panic→restart, per-job/per-slug error isolation).

**WARN (DRY / latent drift)**: For the 4 ops that were *inline* in `run_single_tick` (orphaned-edge compaction, typed-graph rebuild, phase-freq rebuild, contradiction scan), the implementer extracted `pub(crate)` helper fns (`run_orphaned_edge_compaction`, `run_typed_graph_rebuild`, `run_phase_freq_rebuild`, `run_contradiction_scan`) that the new jobs call — but did **not** refactor the legacy `run_single_tick` (still used by the stdio path's `spawn_background_tick`) to call them. The op logic therefore lives in two places. The extracted helpers are documented "Extracted verbatim" and code-match the legacy blocks, so no behavioral divergence ships. This is a maintainability concern (future edits must touch both copies), not a correctness/contract violation for crt-056's deliverable (the per-slug HTTP path delegates correctly). Not blocking; flag for retro/follow-up.

### 2. Architecture compliance — PASS
**Evidence**:
- **ServiceLayer is the sole handle-set owner; PerSlugTickContext borrows** (ADR-003): `from_service_layer` Arc-clones the five `*_handle()` accessors. Unit test `test_per_slug_context_handles_are_service_layer_arcs` asserts `Arc::ptr_eq` for all five — PASS.
- **No parallel registry / no global-handle write path on the multi-project path** (FR-14): the `main.rs` diff removes the `confidence_state_handle()`/.../`phase_freq_table_handle()` global extraction and the `spawn_background_tick(...)` call from `tokio_main_daemon`. `spawn_background_tick` survives only for the **stdio** entrypoint (line 1594), which is single-store and outside the per-slug HTTP parity surface — consistent with the daemon-http-boot.md note. The wave2-gating-audit confirmed the single enumerated removal site.
- **Daemon handle identity**: the daemon passes `Some(services.clone())` to the constructor and borrows `&services` for its own context. `ServiceLayer` is `#[derive(Clone)]` (services/mod.rs:236) over `Arc<RwLock<_>>` handle aliases, so the clone Arc-shares the same underlying handles — handle identity holds for the daemon path too.
- **One shared model** (AC-2): per-slug path `Arc::clone(nli_handle)`; no `NliServiceHandle::new()` on the per-slug path (test-default `None` arm only). SharedTickResources holds the one `nli_handle`.
- **BackgroundJob seam is SHAPE-only** (C-2/R-09): explicit NOT-built list honored — no queue/pool/residency/cadence-signal/concurrent-rayon. `ResourceClass` declaration-only (no reader; only declared + asserted in tests). `Cadence` is `EveryTick`/`EveryN` only with a div-by-zero guard. Serial loop (`for ctx in contexts`), no `spawn`/`join` fan-out across slugs.
- **Registry order = ordering invariant**: `build_job_registry()` lists compaction → co-access promotion → typed-graph rebuild; `test_job_registry_preserves_op_order` locks the exact sequence — PASS.
- **A2 interior-mutability**: agent-5 found RwLock state on Nli/Embed handles, documented as a Step-B blocker (#5171), correctly reasoned as NOT a crt-056 blocker (serial loop never overlaps; async-safe tokio RwLocks). Reasoning confirmed.

### 3. Interface implementation — PASS
**Evidence**: Signatures match §6 Integration Surface: additive `Option<ServiceLayer>` as the final `UnimatrixServer::new` param; `build_project_server` 8 appended params-at-end + `boosted_categories`; `BackgroundJob::run(&self, &PerSlugTickContext, &SharedTickResources) -> Result<(), String>` with **no trait-default body** (anti-bypass, #4974 checklist 4). New accessors (`service_layer`, `tick_metadata`, `vector_index`, `adapt_service`, `audit_log`) are thin, additive, no new state.

### 4. Test case alignment — PASS
**Evidence**: 91 `background::` tests pass; 12 `background::job::tests` pass; 2 `server::tests::test_server_new_{some,none}` (AC-6) pass; `tick_loop` tests (registry dispatch AC-7a, FR-12 visit-once, R-07 per-slug gate firing, failure isolation, N=0 no-op) pass. These match the per-component test plans' unit/structural altitude. The N=2 behavioral trio (AC-3/4/5) and the harness `panic_handler` test are deferred to Stage 3c by protocol — not failed here.

### 5. Code quality — PASS
**Evidence**: `cargo build -p unimatrix-server` finishes (warnings pre-existing). `cargo clippy -p unimatrix-server` warning count is **identical on main and feature/crt-056 (256/256)** — crt-056 introduces **zero net new clippy warnings**. The `-D warnings` gate trips on a pre-existing `collapsible_if` in `unimatrix-engine/src/auth.rs` (a dependency crate, confirmed identical on `main`) — a rust-1.95.0 toolchain-drift lint, not a crt-056 regression. No crt-056 file produces any clippy error or warning. No stubs/`todo!`/`unimplemented!`; no `.unwrap()` in non-test crt-056 code (poison-tolerant `unwrap_or_else(|e| e.into_inner())` is the established convention). New files: job.rs 483, jobs.rs 384, tick_loop.rs 328 — all <500. `server.rs` (4359) and `background.rs` (4336) exceed 500 but were already 4175/4121 on `main` (pre-existing condition not introduced by crt-056; the feature deliberately decomposed its *new* code into sub-500 modules to honor the rule).

### 6. Security — PASS
**Evidence**: Per the risk strategy, crt-056 adds no new external input surface — it threads operator-resolved (trusted) config and reuses existing tick ops over vnc-034 per-slug stores with existing authz. No `unsafe`, no hardcoded secrets in the new files. `DAEMON_SLUG` constant routed through `ProjectSlug::try_from` (allowlist charset, test-confirmed). The corruption guard (AC-4, deferred to 3c) is the per-tenant data-isolation proof — its source-audit basis (sole mutation route, no surviving global write path) is established in the wave2-gating-audit and the handle-identity unit tests.

### 7. Knowledge stewardship — PASS
**Evidence**:
- `crt-056-agent-3-wave1-config-parity-report.md`: Queried (context_briefing — ADR-002/001, params-at-end #2552, #3779); Stored entry #5169.
- `crt-056-agent-5-wave2-tick-report.md`: Queried (context_search — ADR-003/004/005, #5169, #3653, #3663); Stored #5170 (pattern) + #5171 (A2 Step-B blocker lesson).
Both blocks present with Queried + Stored entries.

## Rework Required

None blocking. One non-blocking WARN for retro/follow-up:

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| DRY: 4 tick ops duplicated between legacy `run_single_tick` (stdio path) and the extracted `run_*` helpers | uni-rust-dev (future cleanup, not a 3b blocker) | Refactor `run_single_tick` to call the extracted `run_*` helpers so op logic has one home and stdio/per-slug paths cannot drift. |

## Scope Concerns

None. No SCOPE FAIL indicators — the architecture supports the requirements, the technology works, and scope is intact (Step B leakage avoided, session_capabilities correctly OUT, parity is the closed 8-field checklist).
