# crt-056 Agent Report — Wave 2 per-slug tick (agent-5-wave2-tick)

Implemented Wave 2 production code as one atomic compiling change: PerSlugTickContext
(ADR-003) + BackgroundJob seam (ADR-004) + serial per-slug tick loop (ADR-005) +
retirement of the global-handle wiring on the multi-project daemon path. No integration
tests authored/modified (Stage 3c owns those).

## 1. Files modified / created (absolute)

Created:
- /workspaces/unimatrix/crates/unimatrix-server/src/background/job.rs (483 lines) — PerSlugTickContext, TickMutableState, Cadence/ResourceClass, SharedTickResources, BackgroundJob trait, build_job_registry(), context/cadence/registry unit tests
- /workspaces/unimatrix/crates/unimatrix-server/src/background/jobs.rs (384) — the 9 jobs delegating to existing ops
- /workspaces/unimatrix/crates/unimatrix-server/src/background/tick_loop.rs (328) — run_per_slug_tick_pass + spawn_per_slug_tick + loop-level tests

Modified:
- /workspaces/unimatrix/crates/unimatrix-server/Cargo.toml — added `async-trait = "0.1"` (dyn-compatible async run)
- /workspaces/unimatrix/Cargo.lock — async-trait lock entry
- /workspaces/unimatrix/crates/unimatrix-server/src/background.rs — module decls + re-exports; `read_tick_interval` made pub; extracted 4 per-slug job-delegation helpers (compaction / typed-graph rebuild / phasefreq rebuild / contradiction scan) verbatim from run_single_tick. run_single_tick + legacy spawn_background_tick UNCHANGED.
- /workspaces/unimatrix/crates/unimatrix-server/src/main.rs — daemon path: removed the 5-handle extraction + spawn_background_tick; collect Vec<PerSlugTickContext> (daemon ctx + per-slug ctxs), build SharedTickResources once, spawn_per_slug_tick. Stdio path left on spawn_background_tick (single-store, out of multi-project scope).
- /workspaces/unimatrix/crates/unimatrix-server/src/server.rs — added thin G-2 accessors adapt_service() / audit_log() (no new state).

All my new files are <500 lines. background.rs was already 4121 lines pre-crt-056 (out of scope to split; the Wave-2 seam/loop/jobs are in the new submodules per OVERVIEW.md modular budget).

## 2. Tests: pass/fail

- `cargo test -p unimatrix-server --lib background::` → 87 passed, 0 failed.
  New: 6 cadence/registry tests (job.rs), 5 context-identity tests (Arc::ptr_eq
  handle identity for all 5 handles, slug-store identity, distinct tick_metadata,
  per-slug counter independence, counter advance), 5 loop tests (empty no-op,
  registered-runs/unregistered-doesn't, visits-each-once, per-slug interval gate
  firing through the loop, failure isolation).
- `cargo build -p unimatrix-server` (lib + bin): clean. clippy clean on all changed files.
- Full `cargo test -p unimatrix-server --lib`: 4236 passed, 1 failed —
  `eval::corpus::fixtures_tests::test_ac14_scenario_search_returns_non_empty_ranked_list`,
  a search-ranking eval flake UNRELATED to crt-056 (no tick/context involvement);
  it PASSES in isolation (parallel shared-state pollution in the eval suite).
- `cargo test --workspace` (hardened) was KILLED at the LINK step by SIGKILL/OOM
  (environment memory limit during the large workspace link), not a test failure.
  Per-crate run is the relevant scope and passes (minus the unrelated eval flake).

## 3. A2 interior-immutability audit (R-04 delivery item)

Type-level audit of each shared Arc on the inference read path:
- InferenceConfig, ConfidenceParams: plain data, NO interior mutability — read-only safe.
- RayonPool: Arc<rayon::ThreadPool> + plain fields — intrinsically concurrency-safe.
- **NliServiceHandle**: `{ state: RwLock<NliState>, config: RwLock<Option<NliConfig>> }` —
  INTERIOR-MUTABLE. `get_provider()` reads on the fast path but takes `state.write()` to
  lazily transition (tokio::sync::RwLock).
- **EmbedServiceHandle**: three RwLock fields; `get_adapter()` lazily loads under write.

VERDICT: interior mutability FOUND on nli_handle + embed_handle, but it is a **Step-B
concurrency blocker, NOT a crt-056 blocker**. The serial loop ticks one slug at a time, so
no two contexts touch these handles concurrently; the locks are async-safe tokio RwLocks —
under Step B's concurrent rayon they would serialize (throughput cliff), never corrupt.
Documented (not silently accepted) in Unimatrix #5171. Step B must pre-load the model before
concurrent ticks or accept the per-handle contention envelope.

## 4. Issues / decisions / flags

- **Gap resolved (flagged):** the ADR-003 PerSlugTickContext struct does not list the
  pre-crt-056 mutable tick-loop state (ExtractionContext WATERMARK, NeuralEnhancer,
  ShadowEvaluator). These are inherently per-slug (a shared watermark would skip a slug's
  observations). Added as `TickMutableState` owned per-slug inside the context via
  Arc<Mutex<>>. Faithful to ADR-003's "the per-slug ServiceLayer IS the work-unit's state"
  intent. Pattern stored #5170.
- **MutexGuard-across-await trap:** ExtractionJob takes the per-slug state out of the
  Mutex (clone/take), runs extraction_tick().await on owned values, restores — std
  MutexGuard is not Send across await on the multi-thread runtime.
- **spawn_background_tick NOT deleted:** still the stdio (single-store) path's tick. The
  funnel-retirement is on the multi-project daemon path (the gating-audit's main.rs:965-969
  + 976-1000 site), which now has no global-handle extraction feeding a tick. Leaving the
  dormant fn avoids breaking its existing unit compile-gate tests and the stdio path.
- **Forward obligations from wave2-gating-audit.md (all met):** (1) the multi-project
  global-handle args are REMOVED not supplemented; (2) PerSlugTickContext is built from the
  ServiceLayer `*_handle()` accessors with no `let _ =` discard and no fresh
  `*StateHandle::new()`; (3) BackgroundJob::run is a required method with NO trait-default;
  (4) registry order preserves the background.rs:546-551 ordering invariant (compaction →
  co-access promotion → typed-graph rebuild), asserted by test_job_registry_preserves_op_order.
- **Shared-checkout note:** uncommitted diffs exist in router/integration test files from
  other swarm agents; I staged ONLY my 8 files and left theirs untouched (shared-worktree hazard).

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_search (pattern + decision) — surfaced #5167 (ADR-004),
  #5168 (ADR-005), #5166 (ADR-003), and #5169 (pre-clone by-value moves before per-slug
  loop), #3653 (single rayon spawn/tick), #3663 (VectorIndex sync in rayon). Applied #5169
  (Wave 1, already in tree) and the ADR shapes.
- Stored: entry #5170 "Per-slug tick: each slug owns ExtractionContext + neural enhancer"
  via context_store (pattern); entry #5171 "A2 audit: Nli/Embed handles interior-mutable —
  Step-B blocker" via context_store (lesson-learned).
