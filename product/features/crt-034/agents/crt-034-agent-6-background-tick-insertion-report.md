# Agent Report: crt-034-agent-6-background-tick-insertion

## Component: `background_tick_insertion`

## Summary

Wired `run_co_access_promotion_tick` into the `run_single_tick` background tick loop in `crates/unimatrix-server/src/background.rs`.

## Files Modified

- `crates/unimatrix-server/src/background.rs`

## Changes Made

### 1. Import added (line ~48)
```rust
use crate::services::co_access_promotion_tick::run_co_access_promotion_tick;
```

### 2. Call site inserted with ORDERING INVARIANT anchor comment
Positioned between orphaned-edge compaction block (step 2) and `TypedGraphState::rebuild()` (step 3):

```rust
// ── ORDERING INVARIANT (crt-034, ADR-005) ─────────────────────────────────────
// co_access promotion MUST run:
//   AFTER  step 2 (orphaned-edge compaction) — so dangling entries are removed first
//   BEFORE step 3 (TypedGraphState::rebuild) — so PPR sees promoted edges this tick
// Do NOT insert new tick steps between here and TypedGraphState::rebuild() below.
// ─────────────────────────────────────────────────────────────────────────────
run_co_access_promotion_tick(store, inference_config, current_tick).await;
```

### 3. `current_tick` parameter
Already present in `run_single_tick` signature (line 432) and threaded from the loop. No new parameter or counter needed.

### 4. `PROMOTION_EARLY_RUN_WARN_TICKS` constant
NOT added to `background.rs` per Gate 3a OQ-4 resolution — it is defined as `pub(crate)` in `co_access_promotion_tick.rs` (line 41) to avoid visibility issues.

### 5. Test added: `test_promotion_early_run_warn_ticks_constant_value`
Located in `background::tests` block. Imports `PROMOTION_EARLY_RUN_WARN_TICKS` from `co_access_promotion_tick` and asserts `== 5u32`.

## AC-05 Code Review Evidence

**Anchor comment present:** Yes — ORDERING INVARIANT block immediately before the call.

**Call site position:**
- AFTER: `}` closing the orphaned-edge compaction block (`DELETE FROM graph_edges WHERE source_id NOT IN...`)
- BEFORE: `// crt-021: Rebuild typed graph state after maintenance tick completes.`
- NOT nested inside any `if` guard (unconditional — FR-07 satisfied)

**Function signature:** `run_co_access_promotion_tick(store, inference_config, current_tick)` — passes `current_tick: u32`. ✓

**No `nli_enabled` guard:** Call is at top level of `run_single_tick`, unconditional. ✓

**`PROMOTION_EARLY_RUN_WARN_TICKS` location:** `co_access_promotion_tick.rs:41` (pub(crate)), not background.rs. ✓

**`mod.rs` registration:** `services/mod.rs:28` contains `pub(crate) mod co_access_promotion_tick;`. ✓

## Test Results

- `cargo build --workspace`: PASS (zero errors)
- `cargo test -p unimatrix-server`: PASS
  - `background::tests::test_promotion_early_run_warn_ticks_constant_value`: ok
  - All other tests: no regressions

## Issues / Blockers

None.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #3824 (constants location), #3821 (write pool pattern), #3827 (tick ordering ADR-005). All confirmed alignment with implementation. Entry #3821 confirmed `write_pool_server()` direct path requirement.
- Stored: nothing novel to store — the ordering invariant and constant placement patterns are already captured in entries #3827 (ADR-005) and #3821. Gate 3a OQ-4 resolution (PROMOTION_EARLY_RUN_WARN_TICKS moved to co_access_promotion_tick.rs) is already documented in the existing pseudocode and architecture files.
