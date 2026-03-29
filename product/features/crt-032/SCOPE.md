# crt-032: w_coac Reduction to 0.0 (PPR Transition Phase 2)

## Problem Statement

The direct co-access boost weight `w_coac` was set to 0.10 in the fused scoring formula when PPR (Personalized PageRank) was introduced in crt-030 (#398). Phase 1 measurement (2026-03-29) ran two stable eval rounds (4,349 and 4,467 scenarios) comparing three scoring profiles — pre-ppr, ppr-plus-direct, and ppr-only — and found zero measurable difference between ppr-plus-direct and ppr-only in both aggregate metrics (CC@5, ICD) and per-query analysis. The direct co-access boost contributes nothing PPR does not already provide through GRAPH_EDGES.CoAccess edges in the graph traversal.

With `w_coac=0.10`, every search query incurs a `compute_search_boost` call (spawn_blocking) that loads co-access pairs from the database, normalises them, and inserts them into `boost_map` — a result that is then multiplied by `0.10 * coac_norm` in the fused score. With PPR absorbing all co-access signal, this work is substantively wasted.

Phase 2 task: change `default_w_coac()` to return `0.0`, update hardcoded test expectations, write the ADR, and confirm no regression. Phase 3 (code removal of `compute_search_boost` call and `compute_briefing_boost`) is explicitly deferred.

## Goals

1. **G-01**: Change `default_w_coac()` in `crates/unimatrix-server/src/infra/config.rs` to return `0.0`.
2. **G-02**: Update the compiled-defaults struct literal (`w_coac: 0.10` at line 549) to `w_coac: 0.0`.
3. **G-03**: Update all test assertions that hard-code `w_coac=0.10` as the expected default to reflect `0.0`.
4. **G-04**: Update the comment in `FusionWeights` (`search.rs` line 118) to remove the "default 0.10" annotation.
5. **G-05**: Write ADR documenting why `w_coac=0.0` is correct (PPR subsumes co-access signal) and storing it in Unimatrix.
6. **G-06**: Confirm the fusion weight sum invariant holds with the new default (0.25+0.35+0.15+0.0+0.05+0.05 = 0.85 ≤ 1.0).

## Non-Goals

- **Phase 3 removal** of `compute_search_boost` call from `search.rs`, `compute_briefing_boost` from `coaccess.rs`, or related dead-path plumbing — tracked separately as follow-on issue.
- **CO_ACCESS_STALENESS_SECONDS** removal or modification — this constant governs co-access pair cleanup in the maintenance tick and stats in `context_status`; it is NOT tied to `w_coac` and must be retained.
- **CO_ACCESS table removal** or schema changes — co-access pairs are still recorded and used as PPR graph edges; only the direct additive boost term is zeroed.
- **w_coac field removal** from `InferenceConfig` — the field remains valid (operators can still set it above 0.0 via config if desired); only the compiled default changes.
- **`compute_briefing_boost` call sites** — this function is already dead (defined in `coaccess.rs`, never called from any service). No change in this feature.
- Re-running the eval harness as part of delivery — Phase 1 measurement is the gate evidence; delivery only requires build + test pass.

## Background Research

### Phase 1 Measurement Results (product/research/ass-032/ROADMAP.md)

Two eval runs on 2026-03-29 compared three profiles:

| Metric | pre-ppr | ppr-plus-direct | ppr-only |
|--------|---------|-----------------|----------|
| CC@5   | 0.4252  | 0.4252          | 0.4252   |
| ICD    | 0.6376  | 0.6376          | 0.6376   |
| Avg latency | 7.8ms | 7.9ms | 7.8ms |

All three profiles produce **identical aggregate distribution metrics**. The per-query zero-regression check surfaces 5 queries where pre-ppr regresses vs ppr-only (PPR is contributing), but ppr-plus-direct and ppr-only are indistinguishable. The direct co-access boost is redundant.

### Codebase Analysis

**`default_w_coac()` — config.rs line 621**: Returns `0.10`. This is the `#[serde(default)]` function. Two places set `w_coac: 0.10` in production structs: the compiled defaults at line 549 and the `default_w_coac()` fn at line 621.

**Test assertions hardcoding `w_coac=0.10`**:
- `config.rs`: `make_weight_config()` helper (line 4729), default test assertion (lines 4755-4756), one test fixture (line 4828)
- `search.rs`: Multiple `FusionWeights` struct literals across unit tests (lines 2041, 2057, 2070, 2091, 2116, 2128, 2197, 2218, 2225, 2239, 2264, 2295, 2478, 3015, 3376, 3586, 3631, 3679, 4041)

Most test usages in `search.rs` are **intentional test inputs** (constructing `FusionWeights` with specific values to test scoring behaviour) and do NOT need to change — they are testing the scoring math with `w_coac=0.10`, not testing that the default is `0.10`. Only the tests that specifically assert the compiled default must change.

**Specific tests that must change**:
- `config.rs` line 4755-4756: `assert!((inf.w_coac - 0.10).abs() < 1e-9, "w_coac default must be 0.10")` → change expected value to `0.0`
- `config.rs` line 4729: `make_weight_config()` helper sets `w_coac: 0.10` — this is used in multiple weight tests; the helper value should be updated to reflect the new default, and any test that calculates a sum expecting 0.95 must be updated accordingly
- `config.rs` line 4766-4768: sum assertion checking `sum <= 0.95 + 1e-9` — if `make_weight_config()` is updated to `w_coac: 0.0`, sum becomes 0.85 and the assertion naturally passes (no change needed, it's an upper bound)
- `config.rs` line 4828: `cfg.w_coac = 0.10` inside a specific test — context determines whether this is a default test or an intentional non-zero fixture

**`compute_search_boost` call (search.rs line 991)**: This call remains. With `w_coac=0.0`, `coac_norm` is computed and stored in the scoring input but `weights.w_coac * inputs.coac_norm` evaluates to 0.0. The call is wasteful but not incorrect. Removal is Phase 3.

**`CO_ACCESS_STALENESS_SECONDS`**: Used in 3 places:
1. `search.rs` line 972: staleness cutoff for the boost prefetch (co-access table query)
2. `status.rs` line 625: staleness window for co-access stats display in `context_status`
3. `status.rs` line 1147: cleanup cutoff in maintenance tick (deletes stale pairs)

All three uses are independent of `w_coac`. The constant governs data lifecycle, not scoring weight. Must NOT be removed.

**Fusion weight sum invariant**: The `validate()` method at config.rs line 920-933 enforces `sum ≤ 1.0`. With `w_coac=0.0`, sum = 0.25+0.35+0.15+0.0+0.05+0.05 = 0.85. Constraint satisfied.

### ADR Context

ADR-001 from crt-013 already existed for a `W_COAC` constant removal (Unimatrix entry #701). crt-032 is a continuation in the same direction — zeroing the additive term is the logical predecessor to Phase 3 code removal.

## Proposed Approach

1. In `config.rs`: change `default_w_coac()` to return `0.0` and the compiled-defaults struct literal from `w_coac: 0.10` to `w_coac: 0.0`.
2. In `config.rs` tests: update `make_weight_config()` helper and the one explicit default-assertion test.
3. In `search.rs` comment: update the inline comment from "default 0.10" to "default 0.0 (zeroed in crt-032; PPR subsumes co-access signal)".
4. Write ADR-001 for crt-032 documenting the rationale. Store in Unimatrix.
5. Run `cargo test --workspace` to confirm all tests pass.

## Acceptance Criteria

- **AC-01**: `default_w_coac()` returns `0.0` in `crates/unimatrix-server/src/infra/config.rs`.
- **AC-02**: The compiled defaults struct literal uses `w_coac: 0.0`.
- **AC-03**: `cargo test --workspace` passes with no failures.
- **AC-04**: No test asserts that the default value of `w_coac` is `0.10`.
- **AC-05**: Fusion weight sum with all defaults = 0.85 ≤ 1.0 (invariant holds).
- **AC-06**: ADR-001 written and stored in Unimatrix documenting the transition rationale.
- **AC-07**: `CO_ACCESS_STALENESS_SECONDS` constant is unchanged and still used in status.rs and search.rs.
- **AC-08**: `compute_search_boost` and `compute_briefing_boost` functions remain in place (Phase 3 handles removal).
- **AC-09**: No changes to the CO_ACCESS table schema or co-access recording logic.
- **AC-10**: `w_coac` field remains in `InferenceConfig` struct (only the default value changes).

## Constraints

- **Single-crate change**: All changes are in `crates/unimatrix-server/`. No cross-crate changes needed.
- **Backward compatibility**: Operators who have explicitly set `w_coac` in their config file are unaffected — only the compiled default changes. The field is still valid [0.0, 1.0].
- **No schema migration**: No database changes.
- **No eval run required**: Phase 1 measurement is the gate evidence. Delivery gate = build + test.
- **Phase 3 boundary**: `compute_search_boost` call in `search.rs` stays. `compute_briefing_boost` stays. Only the default weight value changes.

## Open Questions

1. **`make_weight_config()` helper test impact**: Multiple weight tests use this helper and compute sums expecting 0.95. If the helper is updated to `w_coac: 0.0`, sum assertions expecting 0.95 will need updating. The architect/spec should enumerate which sum-based assertions need updating vs which pass naturally (sum ≤ 1.0 bound will still pass with 0.85).

2. **`w_coac=0.10` in `search.rs` test fixtures**: Most `FusionWeights { w_coac: 0.10, ... }` test struct literals are intentional inputs for testing scoring math — they should NOT change. Only one test (`test_fusion_weights_effective_nli_active_unchanged` testing the NLI pass-through) might need attention if it was intended to exercise default values. The spec should clarify the intent.

3. **PPR blend weight interaction**: With `w_coac=0.0`, PPR via `ppr_blend_weight` (crt-030) is the sole carrier of co-access signal. No action needed, but the ADR should document this dependency explicitly.

## Tracking

GH Issue: #415 (Phase 2 — co_access direct boost → PPR deprecation plan)
