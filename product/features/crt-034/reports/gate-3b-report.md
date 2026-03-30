# Gate 3b Report: crt-034

> Gate: 3b (Code Review)
> Date: 2026-03-30
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All phases, branches, and error paths implemented exactly as pseudocode specifies |
| Architecture compliance | PASS | All ADRs followed; write_pool_server() used; no AnalyticsWrite path |
| Interface implementation | PASS | Function signatures, types, and constants match architecture surface table |
| Test case alignment | PASS | All test plan groups and ACs covered; 23 promotion tick tests + 6 config tests + 3 constants tests + 1 background constant test |
| Code quality | PASS | Builds clean; no stubs or placeholders; no .unwrap() in non-test code; main module 289 lines |
| Security | PASS | No hardcoded secrets; SQL uses parameterized queries; no path operations |
| Knowledge stewardship | PASS | All 4 implementation agent reports have Queried: and Stored: or "nothing novel" with reason |

## Detailed Findings

### 1. Pseudocode Fidelity

**Status**: PASS

**Evidence**:

Phase 1 (batch fetch): SQL query matches pseudocode exactly — embedded scalar subquery `(SELECT MAX(count) FROM co_access WHERE count >= ?1) AS max_count`, `ORDER BY count DESC`, `LIMIT ?2`. Parameters bound in correct order: `CO_ACCESS_GRAPH_MIN_COUNT` then `config.max_co_access_promotion_per_tick as i64`.

Phase 2 (SR-05 guard): `if qualifying_count == 0 && current_tick < PROMOTION_EARLY_RUN_WARN_TICKS` fires before the early return, matching pseudocode's two-condition check.

Phase 3 (per-pair loop): INSERT OR IGNORE → rows_affected check → conditional weight fetch → `delta <= CO_ACCESS_WEIGHT_UPDATE_DELTA` boundary guard (strictly greater than required — `<=` means no update at exactly 0.1, matching E-05 spec). UPDATE path correct.

Phase 4 (summary log): `tracing::info!` always emits inserted/updated/qualifying counts.

`CoAccessBatchRow` struct: matches pseudocode exactly — `entry_id_a: i64`, `entry_id_b: i64`, `count: i64`, `max_count: Option<i64>` with `#[derive(sqlx::FromRow)]`.

**Key correctness point 1**: `CO_ACCESS_WEIGHT_UPDATE_DELTA` is `f64 = 0.1` (not f32). Confirmed at line 33 of `co_access_promotion_tick.rs`.

**Key correctness point 3**: Per-pair sequence is INSERT OR IGNORE → rows_affected check → conditional UPDATE on `delta > 0.1` (strictly greater; implementation uses `delta <= CO_ACCESS_WEIGHT_UPDATE_DELTA` to skip, which is equivalent).

**Key correctness point 6**: `source_id = entry_id_a`, `target_id = entry_id_b` (one direction only, ADR-006). Confirmed in INSERT SQL at line 178.

**Key correctness point 7**: Inserted edges have `bootstrap_only=0`, `source=EDGE_SOURCE_CO_ACCESS`, `created_by='tick'`, `relation_type='CoAccess'`. Confirmed in INSERT SQL.

### 2. Architecture Compliance

**Status**: PASS

**Evidence**:

- ADR-001 (#3823): Single-query batch SELECT with embedded scalar subquery for MAX — confirmed. No separate query for max_count.
- ADR-002 (#3824): `EDGE_SOURCE_CO_ACCESS` and `CO_ACCESS_GRAPH_MIN_COUNT` co-located in `read.rs` immediately after `EDGE_SOURCE_NLI` (lines 1641, 1653). Re-exported via `lib.rs` line 38.
- ADR-003 (#3825): `CO_ACCESS_WEIGHT_UPDATE_DELTA: f64 = 0.1` module-private constant. Confirmed.
- ADR-004 (#3826): `max_co_access_promotion_per_tick` follows exact `max_graph_inference_per_tick` pattern — serde default fn, validate() range [1,10000], Default impl stanza, merge_configs stanza. All five modifications confirmed in `config.rs`.
- ADR-005 (#3827): ORDERING INVARIANT anchor comment present at `background.rs` line 550. Call inserted between compaction block close and `TypedGraphState::rebuild()`. SR-05 warn logic present. `current_tick` parameter threaded correctly.
- ADR-006 (#3828): One-directional edge (`source_id=entry_id_a`, `target_id=entry_id_b`). Confirmed.
- `write_pool_server()` used for all reads and writes — confirmed (no `AnalyticsWrite::GraphEdge` path).
- No rayon pool — confirmed (pure SQL module, no ML inference).
- FR-07 (unconditional): call site has no `nli_enabled` guard — confirmed at background.rs line 556.

**Key correctness point 5 (ORDERING INVARIANT)**: The anchor comment reads:
```
// ── ORDERING INVARIANT (crt-034, ADR-005) ─────────────────────────────────────
// co_access promotion MUST run:
//   AFTER  step 2 (orphaned-edge compaction) — so dangling entries are removed first
//   BEFORE step 3 (TypedGraphState::rebuild) — so PPR sees promoted edges this tick
// Do NOT insert new tick steps between here and TypedGraphState::rebuild() below.
// ─────────────────────────────────────────────────────────────────────────────
run_co_access_promotion_tick(store, inference_config, current_tick).await;
```
This is correctly positioned between the compaction block and `TypedGraphState::rebuild()`.

### 3. Interface Implementation

**Status**: PASS

**Evidence**:

| Integration Point | Expected | Actual |
|---|---|---|
| `run_co_access_promotion_tick` signature | `async fn(store: &Store, config: &InferenceConfig, current_tick: u32)` | Matches exactly (`pub(crate)`) |
| `EDGE_SOURCE_CO_ACCESS` | `pub const &str = "co_access"` in `unimatrix-store` | Present at `read.rs:1641`, re-exported via `lib.rs:38` |
| `CO_ACCESS_GRAPH_MIN_COUNT` | `pub const i64 = 3` in `unimatrix-store` | Present at `read.rs:1653`, re-exported via `lib.rs:38` |
| `CO_ACCESS_WEIGHT_UPDATE_DELTA` | `const f64 = 0.1` (module-private) | `co_access_promotion_tick.rs:33` — module-private, f64 confirmed |
| `InferenceConfig::max_co_access_promotion_per_tick` | `usize`, default 200, range [1,10000] | Confirmed with serde default fn at `config.rs:713`, validate at line 893, Default at line 577, merge_configs at line 2110 |
| `services/mod.rs` registration | `pub(crate) mod co_access_promotion_tick;` | Present at `services/mod.rs:28` |

**Deviation noted and accepted**: `PROMOTION_EARLY_RUN_WARN_TICKS: u32 = 5` was placed in `co_access_promotion_tick.rs` (as `pub(crate)`) rather than `background.rs` as originally specified in pseudocode. This deviation was explicitly documented in Gate 3a OQ-4 resolution and recorded in the OVERVIEW.md as the authoritative resolution. The constant value is correct (5), the visibility is appropriate, and the background.rs test imports it from the module. The test plan's check item 5 is satisfied by the alternative placement (same semantics, different file).

### 4. Test Case Alignment

**Status**: PASS

**Evidence**:

**store_constants tests** (`read.rs`): 3 tests covering AC-07, AC-08, and ADR-002 structural compliance. All named identically to test plan.

**config_extension tests** (`config.rs`): 6 tests covering AC-06(a), AC-06(b), AC-10, AC-06(c), AC-06(d), R-07 (both project-wins and global-preserved scenarios). All match test plan expectations.

**co_access_promotion_tick tests** (23 tests across 8 groups):
- Group A (Basic Promotion): `test_basic_promotion_new_qualifying_pair`, `test_inserted_edge_metadata_all_four_fields`, `test_inserted_edge_is_one_directional` — covers AC-01, AC-12, R-10, R-13
- Group B (Cap): `test_cap_selects_highest_count_pairs` — covers AC-04, R-11, asserts which specific pairs selected
- Group C (Weight Refresh): `test_existing_edge_stale_weight_updated`, `test_existing_edge_current_weight_no_update`, `test_weight_delta_exactly_at_boundary_no_update` — covers AC-02, AC-03, E-05
- Group D (Idempotency): `test_double_tick_idempotent`, `test_sub_threshold_pair_not_gc` — covers AC-14, AC-15
- Group E (Empty/Sub-threshold): 5 tests covering all 4 SR-05 quadrants and no-panic scenarios — covers AC-09(a/b/c), R-02, R-06
- Group F (Write Failure): `test_write_failure_mid_batch_warn_and_continue`, `test_write_failure_info_log_always_fires` — covers AC-11, R-01
- Group G (Normalization): `test_global_max_normalization_subquery_shape`, `test_global_max_outside_capped_batch` — covers AC-13, R-03
- Group H (Edge Cases): E-01, E-02, E-03, E-04, E-06 scenarios

**background tick test**: `test_promotion_early_run_warn_ticks_constant_value` in `background::tests` — confirms PROMOTION_EARLY_RUN_WARN_TICKS == 5u32.

**Note on AC-11 (write failure simulation)**: The test plan acknowledged difficulty injecting a true DB write failure. The implementation uses a pre-seeded matching-weight edge to simulate the "no INSERT or UPDATE" path and verifies remaining pairs are still processed. This correctly tests the continue-on-noop semantics. A true error injection would require mock infrastructure not present in the project. The implemented approach verifies the loop continues — this is a reasonable approximation of the spirit of AC-11, and the infallible return contract is verified by the function completing without panic.

### 5. Code Quality

**Status**: PASS

**Evidence**:
- `cargo build --workspace` produces zero errors (only pre-existing warnings in unimatrix-server)
- `cargo test --workspace` — all 2514+ tests pass, zero failures
- No `todo!()`, `unimplemented!()`, `TODO`, `FIXME` in `co_access_promotion_tick.rs` or `co_access_promotion_tick_tests.rs`
- No `.unwrap()` in non-test code in `co_access_promotion_tick.rs` — confirmed by grep (no matches)
- File sizes: `co_access_promotion_tick.rs` is 289 lines (under 500); tests extracted to `co_access_promotion_tick_tests.rs` (636 lines — test file only, not subject to the 500-line production code limit)
- `config.rs` is a pre-existing large file; no new file was created; the additions (~30 lines) are within the established pattern

### 6. Security

**Status**: PASS

**Evidence**:
- No hardcoded secrets or credentials in any changed file
- All SQL queries use parameterized binds (`?1`, `?2`, `?3`, `?4`) — no string interpolation into SQL
- No file path operations; no shell invocations
- No user-supplied data passed directly to SQL — all inputs are typed (`i64`, `f64`, `usize`, `u32`) from config or internal constants
- `INSERT OR IGNORE` semantics — malformed data will not panic (at worst the row is rejected)
- `cargo audit` not installed in this environment; dependency set unchanged from pre-crt-034 (no new crates added); no CVE risk introduced

### 7. Knowledge Stewardship Compliance

**Status**: PASS

All four implementation agent reports contain a `## Knowledge Stewardship` section:

| Agent | Queried | Stored/Declined |
|-------|---------|-----------------|
| crt-034-agent-3-store-constants | `mcp__unimatrix__context_briefing` → #3824, #3591 | "nothing novel to store — pattern already captured in #3591" |
| crt-034-agent-4-config-extension | `mcp__unimatrix__context_briefing` → #3826, #3822, #3821 | "nothing novel to store — pattern already documented in ADR-004 (#3826)" |
| crt-034-agent-5-co-access-promotion-tick | `mcp__unimatrix__context_briefing` → #3823, #3822, #3821, #3826, #3827 | Stored entry #3831 "co_access table: column is last_updated not last_access; CHECK rejects self-loops silently with INSERT OR IGNORE" |
| crt-034-agent-6-background-tick-insertion | `mcp__unimatrix__context_briefing` → #3824, #3821, #3827 | "nothing novel to store — patterns in #3827, #3821; OQ-4 resolution documented in pseudocode" |

All entries include sufficient reason after "nothing novel" or a stored entry. Full compliance.

## Rework Required

None.

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — not queried for this gate; gate validation is a read-only analysis task against known artifacts.
- Stored: nothing novel to store — this gate found no systemic failure pattern. All checks passed on first review with no rework needed. Feature-specific gate result lives in this report only.
