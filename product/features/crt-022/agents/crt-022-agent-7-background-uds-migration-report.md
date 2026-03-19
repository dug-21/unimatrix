# Agent Report: crt-022-agent-7-background-uds-migration

**Task**: Migrate 3 background/warmup call sites (Sites 4, 5, 6) off `spawn_blocking` onto rayon (Wave 3)

---

## Outcome

All 3 target sites successfully migrated. `cargo build --workspace` passes with zero errors. No new test failures.

---

## Files Modified

- `/workspaces/unimatrix/crates/unimatrix-server/src/background.rs` — Sites 4 and 5 migrated (along with signature threading for `run_single_tick` and `extraction_tick`)
- `/workspaces/unimatrix/crates/unimatrix-server/src/uds/listener.rs` — Site 6 migrated (warmup embedding)

---

## Work Summary

### Context at Task Start

By the time this agent ran, prior Wave 3 agents had:
- Migrated Sites 1–3 and 7 (MCP handler `spawn_blocking_with_timeout` → `rayon_pool.spawn_with_timeout`) in `services/`
- Threaded `ml_inference_pool` through `spawn_background_tick` → `background_tick_loop` → `run_single_tick` parameter chains in `background.rs`
- Fixed `StatusService::new` call in `run_single_tick` to pass the pool

This agent completed the remaining sites.

### Site 4 — Contradiction scan (`background.rs` ~543, Pattern B)

Replaced `tokio::time::timeout(TICK_TIMEOUT, tokio::task::spawn_blocking(...))` with `ml_inference_pool.spawn(...).await`. Match arms:
- `Ok(Ok(pairs))` — update contradiction cache (same as before)
- `Ok(Err(e))` — `warn!()` scan failed (same as before)
- `Err(e)` — `error!()` "contradiction scan rayon task cancelled; cache retained" (Pattern B requirement)

The outer `tokio::time::timeout(TICK_TIMEOUT, ...)` wrapper was removed for this closure because `spawn` (no timeout) is the correct Pattern B call for background tasks. The per-tick TICK_TIMEOUT still applies at the `extraction_tick` level.

### Site 5 — Quality-gate embedding loop (`background.rs` ~1162, Pattern B)

Added `ml_inference_pool: &Arc<RayonPool>` parameter to `extraction_tick` function signature and its call site in `run_single_tick`.

Replaced `tokio::task::spawn_blocking(...).await.map_err(...)?` with `ml_inference_pool.spawn(...).await`. On `Err(e)` (Cancelled), emits `error!()` "quality-gate embedding rayon task cancelled; skipping store step" and returns `Ok(ctx.stats.clone())` so the tick continues without crashing.

Non-inference `spawn_blocking` calls in `extraction_tick` were left untouched:
- ~1093: `run_extraction_rules` (pure in-memory rule evaluation)
- ~1149: `persist_shadow_evaluations` (DB write)

### Site 6 — Warmup embedding (`uds/listener.rs` ~1383, Pattern A)

Added imports: `use crate::infra::rayon_pool::RayonPool` and `use crate::infra::timeout::MCP_HANDLER_TIMEOUT`.

Updated `warm_embedding_model` signature to accept `ml_inference_pool: &Arc<RayonPool>`. The call site in `dispatch_request` at line 486 passes `&services.ml_inference_pool` — no additional parameter threading needed because `dispatch_request` already receives `services: &ServiceLayer` and `ServiceLayer.ml_inference_pool` is `pub(crate)`.

Replaced `tokio::task::spawn_blocking(...)` with `ml_inference_pool.spawn_with_timeout(MCP_HANDLER_TIMEOUT, ...)`. Preserved warning tracing for failure cases (RayonError / embed failure) for operator observability.

---

## Compile Result

```
cargo build --workspace
Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.19s
```

Zero errors. 6 pre-existing warnings (all pre-existing before this agent ran).

## Test Result

All unit tests pass. Only failure is `crates/unimatrix-server/src/infra/config.rs - infra::config (line 21)` doctest — pre-existing failure verified by stash+test before my changes.

---

## Issues / Blockers

None.

---

## Knowledge Stewardship

- Queried: `/uni-query-patterns` for `unimatrix-server` background task rayon spawn — found ADR-002, ADR-003 (relevant), pattern #2535 (monopolisation), pattern #1366 (tick loop error recovery)
- Stored: entry #2554 "Accessing ml_inference_pool from uds/listener.rs via services.ml_inference_pool (crt-022)" via `/uni-store-pattern` — the key non-obvious insight that `dispatch_request` already has `services: &ServiceLayer` so no new parameter threading through accept_loop / handle_connection is needed.
