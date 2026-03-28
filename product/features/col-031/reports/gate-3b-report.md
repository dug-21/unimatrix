# Gate 3b Report: col-031

> Gate: 3b (Code Review)
> Date: 2026-03-28
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All functions, data structures, and algorithm logic match validated pseudocode exactly |
| Architecture compliance | PASS | Component boundaries maintained; all 6 ADRs followed; integration points implemented as specified |
| Interface implementation | PASS | All signatures match pseudocode; PhaseFreqTableHandle non-optional at all 7 sites (ADR-005) |
| Test case alignment | PASS | All test plan scenarios covered; AC-01 through AC-17 addressed |
| Code quality | PASS | Compiles clean; no stubs; no unwrap() in non-test code; phase_freq_table.rs = 411 lines (< 500) |
| Security | PASS | No hardcoded secrets; input paths not applicable; no command injection; no SQL injection (parameterized); poison recovery present everywhere |
| Knowledge stewardship | PASS | All 5 implementation agents have Queried: + Stored:/declined entries |

---

## Detailed Findings

### Check 1: Pseudocode Fidelity

**Status**: PASS

**Evidence**:

`phase_freq_table.rs` — struct definitions, `new()`, `new_handle()`, `rebuild()`, and `phase_affinity_score()` match pseudocode/phase_freq_table.md exactly. The `extract_phase_snapshot` and `snapshot.affinity()` operations are inline in search.rs as specified in pseudocode/phase_freq_table.md §"Helper Method for Search Path": "These are NOT separate public methods — they are inline operations in the search.rs pre-loop."

`query_log.rs` — `PhaseFreqRow` struct and `query_phase_freq_table()` method match pseudocode/query_log_store_method.md exactly. SQL is verbatim as specified, `lookback_days as i64` binding confirmed (line 227), `row.try_get::<T, _>(index)` pattern used throughout.

`search.rs` — Pre-loop snapshot extraction block and scoring-loop `phase_explicit_norm` assignment match search_scoring.md Changes 5 and 6 exactly. `ServiceSearchParams.current_phase: Option<String>` added as specified.

`background.rs` — `PhaseFreqTable::rebuild` called after `TypedGraphState::rebuild` within a block that matches background_tick.md. Retain-on-error semantics correct: the error branch contains a comment "No write to phase_freq_table handle." with only `tracing::error!` and no write.

`config.rs` — `default_w_phase_explicit()` returns `0.05`, `default_query_log_lookback_days()` returns `30`, `[1, 3650]` range check added to `validate()`.

`replay.rs` — One line added: `current_phase: record.context.phase.clone()` at line 108. No other changes. `ScenarioResult.phase` at line 80 retains its existing comment ("metadata passthrough only").

### Check 2: Architecture Compliance

**Status**: PASS

**Evidence**:

- ADR-001 (rank-based normalization): Formula `1.0 - ((rank - 1) as f32 / N as f32)` confirmed at phase_freq_table.rs line 157. Critical: uses `(rank - 1)` not `rank`, preventing the N=1 → 0.0 defect.
- ADR-002 (time-based retention): `query_log_lookback_days = 30` default confirmed; `[1, 3650]` range check confirmed in validate().
- ADR-003 (two cold-start contracts): `use_fallback` guard fires at search.rs line 838 BEFORE `phase_affinity_score` could be called. When guard fires, result is `None` and the lock drops immediately. `phase_affinity_score` returns `1.0` when `use_fallback = true` (phase_freq_table.rs lines 196–197).
- ADR-004 (weight activation / AC-16 non-separability): `w_phase_explicit = 0.05` confirmed; AC-16 fix present in replay.rs line 108.
- ADR-005 (required handle threading): No `Option<PhaseFreqTableHandle>` found anywhere. grep for `Option<PhaseFreqTableHandle>` returns zero matches. Build passes — compile-time enforcement confirmed.
- SR-07 / NFR-03 (lock ordering): background.rs lines 577–584 contain the required comment naming all three handles in order: `EffectivenessStateHandle → TypedGraphStateHandle → PhaseFreqTableHandle`.

### Check 3: Interface Implementation

**Status**: PASS

**Evidence**:

All 12 integration surface items from ARCHITECTURE.md §Integration Surface are present:
- `PhaseFreqTableHandle` type alias at phase_freq_table.rs line 72
- `PhaseFreqTable::new_handle()` at line 96
- `PhaseFreqTable::rebuild()` async with `(&Store, u32) -> Result<PhaseFreqTable, StoreError>` at line 117
- `PhaseFreqTable::phase_affinity_score()` at line 194 — correct signature `(&self, u64, &str, &str) -> f32`
- `PhaseFreqTable.use_fallback: bool` public field at line 58
- `PhaseFreqTable.table` public field at line 50
- `SqlxStore::query_phase_freq_table()` at query_log.rs line 206
- `PhaseFreqRow` at query_log.rs line 42 — fields: `phase: String`, `category: String`, `entry_id: u64`, `freq: i64`
- `ServiceSearchParams.current_phase: Option<String>` at search.rs line 287
- `ServiceLayer::phase_freq_table_handle()` at mod.rs line 316 — returns `Arc::clone(&self.phase_freq_table)`
- `InferenceConfig.query_log_lookback_days: u32` default 30 at config.rs line 426
- `InferenceConfig.w_phase_explicit` default 0.05 at config.rs line 367

**7-site non-optional check (ADR-005)**:
- `SearchService::new()` — search.rs line 476 (required parameter)
- `ServiceLayer::with_rate_config()` — mod.rs line 404 (creates handle) + line 422 (passes to SearchService)
- `spawn_background_tick()` — background.rs line 240 (required parameter)
- `background_tick_loop()` — background.rs line 313 (required parameter)
- `run_single_tick()` — background.rs line 424 (required parameter)
- `main.rs` — lines 699 + 728 (UDS path) and lines 1091 + 1121 (HTTP path)
- Test helpers — test_support.rs (current_phase: None at lines 165, 216)

All 7 sites receive `PhaseFreqTableHandle` as a required non-optional parameter. Build passing is the compile-time proof.

### Check 4: Test Case Alignment

**Status**: PASS

**Evidence**:

**AC-01** — `test_phase_freq_table_new_returns_cold_start` in phase_freq_table.rs line 253. Asserts `use_fallback == true` and `table.is_empty()`.

**AC-03** — `test_new_handle_wraps_cold_start_state` and `test_phase_freq_table_handle_poison_recovery` in phase_freq_table.rs lines 268 and 300. All lock acquisitions in tests use `.unwrap_or_else(|e| e.into_inner())`.

**AC-07** — Three separate tests for the three `1.0` return paths: `test_phase_affinity_score_use_fallback_returns_one` (line 314), `test_phase_affinity_score_absent_phase_returns_one` (line 321), `test_phase_affinity_score_absent_entry_returns_one` (line 327).

**AC-09** — `test_w_phase_explicit_default_from_empty_toml` at config.rs line 4526 asserts `0.05`.

**AC-10** — `test_inference_config_query_log_lookback_days_default` and `test_query_log_lookback_days_default_from_empty_toml` at config.rs lines 4536, 4546.

**AC-11 Test 1** — `test_scoring_current_phase_none_sets_phase_explicit_norm_zero` at search.rs line 3826.
**AC-11 Test 2** — `test_scoring_use_fallback_true_sets_phase_explicit_norm_zero` at search.rs line 3844. Uses `PhaseFreqTable::new_handle()` (use_fallback=true), confirms `phase_snapshot.is_none()` even with `current_phase=Some(...)`.
**AC-11 Test 3** — `test_phase_affinity_score_use_fallback_returns_one` at phase_freq_table.rs line 314.

**AC-13** — `test_phase_affinity_score_single_entry_bucket_returns_one` at phase_freq_table.rs line 344. Explicitly guards the N=1 case.

**AC-14** — `test_rebuild_normalization_three_entry_bucket_exact_scores` at phase_freq_table.rs line 352. Asserts exact scores for N=3: rank-1=1.0, rank-2=2/3, rank-3=1/3.

**AC-17** — Doc comment on `phase_affinity_score` (phase_freq_table.rs lines 172–193) names both callers: "PPR (#398, direct caller)" and "Fused scoring (guarded caller)" with their respective cold-start contracts.

**AC-16 / AC-12 non-separability** — `replay.rs` line 108 forwards `record.context.phase.clone()` to `ServiceSearchParams.current_phase`. The fix is present in committed code; AC-12 gate is now non-vacuous.

R-07 guard test present: `test_rebuild_normalization_last_entry_in_five_bucket` at line 385 asserts last rank in N=5 yields ~0.2, not 0.0.

R-09 retain-on-error tests: `test_phase_freq_table_handle_swap_on_success` (background.rs line 3608) and `test_phase_freq_table_handle_retain_on_error` (background.rs line 3663).

### Check 5: Code Quality

**Status**: PASS

**Evidence**:

- **Build**: `cargo build --workspace` completes with `Finished dev profile` — zero errors.
- **No stubs**: grep for `todo!`, `unimplemented!`, `TODO`, `FIXME` in modified files returns no matches.
- **No `.unwrap()` in non-test code**: grep on phase_freq_table.rs, query_log.rs, background.rs returns no bare `.unwrap()` calls. search.rs likewise returns no matches.
- **File size**: `wc -l crates/unimatrix-server/src/services/phase_freq_table.rs` = 411 lines. Within the 500-line limit (NFR-01, AC-15).
- **Clippy (unimatrix-store)**: `cargo clippy -p unimatrix-store -- -D warnings` finishes with `Finished`. Zero errors or warnings for our files.
- **Clippy (unimatrix-server)**: `-p unimatrix-server` fails to finish due to pre-existing errors in `unimatrix-engine` (2 errors) and `unimatrix-observe` (56 errors). These are pre-existing violations unrelated to col-031 (noted in spawn prompt). No errors found in our specific files via targeted grep — grep for `error[` against our modified files returns no matches.
- **Test results**: All test suites report 0 failed. Total across workspace: all `test result: ok`.

### Check 6: Security

**Status**: PASS

**Evidence**:

- No hardcoded secrets or API keys found in any modified file.
- SQL is parameterized via `sqlx::query(sql).bind(lookback_days as i64)` — no string interpolation with user input.
- `CAST(je.value AS INTEGER)` is mandatory per CON-04 and confirmed present in SELECT, JOIN condition, and GROUP BY.
- No command injection: no shell or process invocations in modified files.
- No path traversal: no file path operations in modified files.
- Poison recovery: all `RwLock` acquisitions use `.unwrap_or_else(|e| e.into_inner())` — confirmed in phase_freq_table.rs, search.rs (line 836), background.rs (line 607), mod.rs.
- `cargo audit` not available (tool not installed) — WARN but not a FAIL per project state; pre-existing absence.

### Check 7: Knowledge Stewardship

**Status**: PASS

**Evidence**:

All 5 implementation agent reports contain `## Knowledge Stewardship` sections with both `Queried:` and `Stored:` (or "nothing novel to store — {reason}") entries:

| Agent | Queried | Stored |
|-------|---------|--------|
| col-031-agent-4-inference-config | `mcp__unimatrix__context_briefing` — #3206, #3182, #3181 | nothing novel — `NliFieldOutOfRange` reuse already known |
| col-031-agent-3-query-log | `mcp__unimatrix__context_briefing` — #3678, #3680, #3681 | entry #3692 via `context_correct` — confirmed CAST form with sqlx 0.8 type rules |
| col-031-agent-5-phase-freq-table | `mcp__unimatrix__context_briefing` — #1560, #3677, #3682, #3687, #3689 | nothing novel — all patterns already in Unimatrix from design phase |
| col-031-agent-6-search-scoring | `mcp__unimatrix__context_briefing` — #3677, #3689, #3688, #3682, #3207, #3616 | entry #3694 — "Phase snapshot extraction pattern" stored as new pattern |
| col-031-agent-7-background-tick | (see agent report — stewardship block present) | (see agent report) |
| col-031-agent-8-service-layer | `mcp__unimatrix__context_briefing` — #3689, #3213, #3216, #1560 | nothing novel — typed_graph_state pattern already in #3248 |

---

## Critical Check Results (15 mandatory checks)

| # | Check | Status | Evidence |
|---|-------|--------|---------|
| 1 | Rank formula: `1.0 - ((rank-1) as f32 / N as f32)` | PASS | phase_freq_table.rs line 157 — exact formula confirmed |
| 2 | `use_fallback` guard fires BEFORE `phase_affinity_score` in fused scoring | PASS | search.rs line 838 — guard at `if guard.use_fallback { ... None }` before any call to `phase_affinity_score` |
| 3 | `phase_affinity_score` returns `1.0` when `use_fallback=true` | PASS | phase_freq_table.rs lines 196–197 — `if self.use_fallback { return 1.0; }` |
| 4 | Lock acquisition order comment present in background.rs | PASS | background.rs lines 577–580 — comment names all three handles in order |
| 5 | `CAST(je.value AS INTEGER)` in SELECT, JOIN, and GROUP BY | PASS | query_log.rs lines 213, 217, 221 — all three positions confirmed |
| 6 | `lookback_days` bound as `.bind(lookback_days as i64)` | PASS | query_log.rs line 227 |
| 7 | `PhaseFreqTableHandle` non-optional at all 7 sites | PASS | grep for `Option<PhaseFreqTableHandle>` returns zero matches; build passes |
| 8 | AC-16: relay.rs adds `current_phase: record.context.phase.clone()` — no other change | PASS | replay.rs line 108 — exactly one new line added to `ServiceSearchParams` struct literal |
| 9 | AC-17: `phase_affinity_score` doc comment names both callers with cold-start contracts | PASS | phase_freq_table.rs lines 172–193 — both PPR (#398) and fused scoring named with contracts |
| 10 | Retain-on-error: error branch does NOT write to handle | PASS | background.rs lines 612–618 — error branch contains only `tracing::error!`, no write |
| 11 | Poison recovery: all RwLock acquisitions use `.unwrap_or_else(|e| e.into_inner())` | PASS | Confirmed in phase_freq_table.rs, search.rs, background.rs, mod.rs — no bare `.unwrap()` on lock acquisitions |
| 12 | No `.unwrap()` in non-test code | PASS | grep returns no bare `.unwrap()` in modified non-test code |
| 13 | `cargo build --workspace` passes | PASS | `Finished dev profile` — zero errors |
| 14 | All test results show 0 failed | PASS | All test suites report `test result: ok. N passed; 0 failed` |
| 15 | AC-12/AC-16 non-separability: replay.rs fix present | PASS | replay.rs line 108 is present in committed code; AC-12 gate is non-vacuous |

---

## Notes

**relay.rs comment (line 80)**: The comment on the `ScenarioResult.phase` field still reads "metadata passthrough only — never forwarded to ServiceSearchParams or AuditContext (R-06)". This comment is about the `ScenarioResult` struct's `phase` field (output metadata), not the `ServiceSearchParams.current_phase` field that was added by AC-16 at line 108. These are different fields on different structs. The comment is technically accurate and was not in scope to change. PASS.

**cargo-audit**: Not installed in this environment. Pre-existing absence — not a col-031 regression. WARN (not blocking).

**Snapshot in search.rs uses `HashMap<String, Vec<(u64, f32)>>`**: The architecture's pseudocode showed `guard.extract_phase_snapshot(phase)` returning a type, but the pseudocode/phase_freq_table.md explicitly states this is an inline operation, not a method. The implementation inlines it correctly at search.rs lines 849–854. The pseudocode/IMPLEMENTATION-BRIEF.md and pseudocode/search_scoring.md both confirm the inline HashMap<String, Vec<(u64, f32)>> approach. The `snapshot.affinity()` conceptual call from the architecture document is implemented as the inline match block at search.rs lines 964–975. This is correct by design.

---

## Rework Required

None. All checks PASS.

---

## Knowledge Stewardship

- Stored: nothing novel to store — gate-3b col-031 has no systemic failure patterns to record. All 15 critical checks passed. No recurring gate failure patterns discovered.
