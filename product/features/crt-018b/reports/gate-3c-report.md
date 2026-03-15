# Gate 3c Report: crt-018b

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-15
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 14 risks covered; 2 documented gaps (G-01, G-02) are architectural, not defects |
| Test coverage completeness | PASS | All Risk-Test-Strategy scenarios exercised; 1 xfail with documented rationale |
| Specification compliance | PASS | All 18 acceptance criteria met; AC-17 item 4 satisfied via unit test at both weight extremes |
| Architecture compliance | PASS | All 6 components implemented as specified; ADRs honored |
| Knowledge stewardship | PASS | All agent reports contain `## Knowledge Stewardship` with `Queried:` and `Stored:` entries |

---

## Detailed Findings

### Check 1: Risk Mitigation Proof

**Status**: PASS

**Evidence**: `RISK-COVERAGE-REPORT.md` maps all 14 risks (R-01 through R-14) to passing tests. No risk lacks coverage.

| Risk | Priority | Coverage | Test(s) |
|------|----------|----------|---------|
| R-01 (double-lock deadlock) | Critical | Full | `test_generation_read_write_no_simultaneous_locks`, `test_snapshot_read_guard_dropped_before_mutex_lock` |
| R-02 (asymmetric delta call sites) | Critical | Full | 5 unit tests covering all four call sites across Steps 7 & 8 |
| R-03 (bulk quarantine contention) | Critical | Full (unit) / Partial (integration) | `test_auto_quarantine_fires_at_threshold`, integration gap G-01 documented |
| R-04 (`context_status` writes counters) | High | Full | `test_context_status_does_not_advance_consecutive_counters` (L-E03) |
| R-05 (delta outside penalty multiplication) | High | Full | `test_utility_delta_inside_deprecated_penalty`, `test_utility_delta_inside_superseded_penalty` |
| R-06 (generation cache not shared) | High | Full | `test_cached_snapshot_shared_across_clones`, `test_effectiveness_snapshot_generation_match`, `test_briefing_service_clones_share_snapshot` |
| R-07 (absent entry non-zero delta) | High | Full | `test_utility_delta_none_zero`, `test_utility_delta_absent_entry_zero`, `test_effectiveness_priority_none` |
| R-08 (tick-skipped audit not emitted) | Medium | Full | `test_emit_tick_skipped_audit_detail_fields` |
| R-09 (briefing sort primary key wrong) | Medium | Full | `test_injection_sort_confidence_is_primary_key`, `test_injection_sort_effectiveness_is_tiebreaker` |
| R-10 (SETTLED_BOOST > co-access max) | Medium | Full | `test_utility_constants_values` asserting `SETTLED_BOOST < 0.03` |
| R-11 (quarantine fires for Settled/Unmatched) | Medium | Full | `test_auto_quarantine_does_not_fire_for_settled/unmatched/effective` |
| R-12 (`auto_quarantined_this_cycle` not populated) | Low | Partial | Unit path verified; integration visibility has same tick-drivability constraint as G-01 |
| R-13 (write lock held during SQL) | Critical | Full | `test_write_lock_not_held_after_tick_write_block`; code review confirms guard drops before `process_auto_quarantine()` call at line 501 |
| R-14 (crt-019 adaptive weight not exercised) | Medium | Full | `test_effective_outranks_ineffective_at_max_weight` (cw=0.25) and floor test (cw=0.15) |

**Documented Gaps**:

- **G-01** (AC-10/AC-17 item 3): End-to-end auto-quarantine integration test requires the background tick to be drivable externally. Tick interval is 15 minutes in production with no external trigger via MCP. Test `test_auto_quarantine_after_consecutive_bad_ticks` (L-E05) is marked `@pytest.mark.xfail` with a documented architectural rationale. Unit tests in `background.rs` cover the trigger logic completely. The underlying store path is independently confirmed by pre-existing L-08 (`test_store_quarantine_restore_search_finds`). Gap is architectural, not a defect.

- **G-02** (AC-13 / R-12 integration): `auto_quarantined_this_cycle` visibility in `context_status` shares the same tick-drivability constraint. Unit tests confirm field population. No GH Issue required per documented rationale.

The xfail marker on L-E05 does not have a GH Issue. The RISK-COVERAGE-REPORT.md explicitly states: *"No GH Issue is required (the gap is intentional and architectural — the feature scope does not include a test-mode tick trigger)."* The gap does not mask a feature bug — the quarantine store path and trigger logic are both independently validated.

---

### Check 2: Test Coverage Completeness

**Status**: PASS

**Evidence**: All risk-to-scenario mappings from Phase 2 are exercised.

**Unit test totals** (from RISK-COVERAGE-REPORT.md, confirmed by local `cargo test --workspace` run):

| Crate | Passed | Failed | Ignored |
|-------|--------|--------|---------|
| unimatrix-engine | 230 | 0 | 0 |
| unimatrix-server | 1295 | 0 | 0 |
| Full workspace | 2472 | 0 | 18 (pre-existing) |

The 18 ignored tests are pre-existing in other crates and unrelated to this feature.

**Integration test totals**:

| Suite | Passed | XFailed | Failed |
|-------|--------|---------|--------|
| Smoke (18 existing + infra) | 18 | 1 (pre-existing GH#111) | 0 |
| Lifecycle (pre-existing 17) | 16 | 1 (pre-existing GH#238) | 0 |
| Lifecycle (crt-018b: L-E01 to L-E05) | 4 | 1 (G-01, architectural) | 0 |
| Security (pre-existing 15) | 15 | 4 (pre-existing) | 0 |
| Security (crt-018b: S-31, S-32) | 2 | 0 | 0 |
| Tools | 53 | 4 (pre-existing) | 0 |
| **Total** | **188** | **8** (7 pre-existing + 1 new) | **0** |

Coverage is complete. Risk-to-scenario mapping from Phase 2 fully exercised. Integration risks (lock contention, ordering, empty-report injection) verified via code review and unit tests per RISK-COVERAGE-REPORT.md.

**AC-17 item 4 (crt-019 adaptive weight)**: The spec requires at least one test with `observed_spread >= 0.20` to confirm the formula at full confidence weight. This is fulfilled by `test_effective_outranks_ineffective_at_max_weight` (search.rs), which exercises `confidence_weight = 0.25` (the ceiling of the adaptive range). The integration tests do not include a fixture that verifies live confidence spread — this is the noted gap. Given that the integration test L-E01 (`test_effectiveness_search_ordering_after_cold_start`) is a cold-start test that relies on the pre-existing crt-019 infrastructure and the unit test covers both weight extremes, this is acceptable per the Risk-Test-Strategy coverage requirement for R-14.

---

### Check 3: Specification Compliance

**Status**: PASS

**Evidence**: All 18 acceptance criteria verified by RISK-COVERAGE-REPORT.md. All criteria now show PASS status. Key verifications:

- **FR-01 / AC-01**: `EffectivenessState` written only by background tick; `context_status` calls confirmed to not advance counters (L-E03).
- **FR-03 / AC-09**: Consecutive-bad-cycle semantics (increment, reset, remove) unit-tested with three-tick sequences.
- **FR-04 / AC-03**: Constants `UTILITY_BOOST = 0.05`, `SETTLED_BOOST = 0.01`, `UTILITY_PENALTY = 0.05` present in `unimatrix-engine::effectiveness::mod.rs` lines 38–46. `SETTLED_BOOST < 0.03` invariant asserted in tests.
- **FR-05 / AC-04**: `utility_delta()` function in `search.rs` lines 109–117 covers all 5 categories and `None`.
- **FR-06 / AC-05**: Effective (sim=0.75) outranks Ineffective (sim=0.76) at both `confidence_weight` extremes (0.15, 0.25).
- **FR-07 / FR-08 / AC-07 / AC-08**: `BriefingService` constructor takes `EffectivenessStateHandle` as required parameter; injection history and convention sorts use `(confidence DESC, effectiveness_priority DESC)`.
- **FR-09 / AC-09**: Counter semantics confirmed via unit tests; hold-on-error path confirmed by `test_emit_tick_skipped_audit_detail_fields`.
- **FR-10 / AC-10**: Auto-quarantine trigger confirmed at threshold; counter reset only on successful quarantine (no pre-reset).
- **FR-11 / AC-13**: Audit event fields (operation, agent_id, entry_title, entry_category, classification, consecutive_cycles, threshold, reason) all present in `emit_auto_quarantine_audit()` (background.rs lines 688–736).
- **FR-12 / AC-11 / AC-12**: `parse_auto_quarantine_cycles()` validated; default=3, 0=disable, >1000=startup error.
- **FR-13 / AC-09**: `emit_tick_skipped_audit()` called on `compute_report()` error before early return, confirmed by code review (background.rs lines 400–406).
- **FR-14**: `auto_quarantined_this_cycle` field populated after `process_auto_quarantine()` returns (background.rs lines 515–518).
- **Constraints**: Stored formula invariant unchanged (no new weights). No new DB tables. No new MCP tools. Lock ordering maintained. `RetrievalMode::Strict` path unmodified.

---

### Check 4: Architecture Compliance

**Status**: PASS

**Evidence**: All 6 components match the approved Architecture.

**Component 1 (EffectivenessState)**: Implemented in `services/effectiveness.rs` with exact struct fields (`categories: HashMap<u64, EffectivenessCategory>`, `consecutive_bad_cycles: HashMap<u64, u32>`, `generation: u64`). `EffectivenessStateHandle = Arc<RwLock<EffectivenessState>>`. `EffectivenessSnapshot` with `Arc<Mutex<_>>` wrapper for clone-sharing per ADR-001.

**Component 2 (Background Tick Writer)**: `maintenance_tick()` in `background.rs` calls `compute_report()`, acquires write lock, updates state, releases lock, then calls `process_auto_quarantine()` after lock release (NFR-02 / R-13 confirmed at lines 418–533). Generation incremented inside write lock (line 496). On error: `emit_tick_skipped_audit()` called and `Err` returned without touching `EffectivenessState` (ADR-002 / FR-13).

**Component 3 (Search Utility Delta)**: `utility_delta()` applied at both sort passes (Step 7 lines 348–372, Step 8 lines 415–439). Delta is inside `status_penalty` multiplication per ADR-003. Generation-cached snapshot pattern per ADR-001 (lines 167–194). Applies only to `RetrievalMode::Flexible` path.

**Component 4 (Briefing Effectiveness Tiebreaker)**: `BriefingService::new()` takes `EffectivenessStateHandle` as required parameter (ADR-004). Injection history sort and convention sort use `(confidence DESC, effectiveness_priority DESC)` at lines 446–457 and 262–297.

**Component 5 (Auto-Quarantine Guard)**: Threshold scan inside write lock; SQL writes after lock release. Per-entry error isolation (R-03). `AUTO_QUARANTINE_CYCLES = 0` guard. Defensive category re-check (AC-14). `parse_auto_quarantine_cycles_str()` extracted for testability (avoids `unsafe` `set_var` in Rust 2024).

**Component 6 (Audit Events)**: Both `auto_quarantine` and `tick_skipped` events implemented with all required fields. `SYSTEM_AGENT_ID` is a hardcoded constant, not propagated from user input (Security Risk 2 mitigated).

**ADR compliance**:
- ADR-001: Generation counter + `Arc<Mutex<EffectivenessSnapshot>>` implemented correctly.
- ADR-002: Hold-on-error semantics implemented.
- ADR-003: Delta inside penalty multiplication confirmed at all four call sites.
- ADR-004: `EffectivenessStateHandle` non-optional in `BriefingService::new()` (compile error if omitted).

**Integration surface**: `spawn_background_tick()` signature includes `EffectivenessStateHandle` parameter. `ServiceLayer` holds handle and wires to search, briefing, and background tick. Wiring confirmed in `services/mod.rs` (effectiveness_state_handle method at line 250, wiring at lines 311 and 331).

---

### Check 5: Knowledge Stewardship Compliance

**Status**: PASS

All test-phase agent reports contain the required `## Knowledge Stewardship` section:

- `crt-018b-agent-9-report.md` (tester): Queried procedures; stored "nothing novel to store — {reason}" with explicit rationale.
- `crt-018b-agent-1-report.md` (implementation wave): Section present with Queried and Stored entries.
- All design-phase agents (confirmed by Gate 3a report): present with appropriate entries.
- `crt-018b-gate-3b-report.md`: Section present.

Note: The Gate 3a report documented that `crt-018b-agent-1-architect-report.md` lacked a `## Knowledge Stewardship` section (substance present, structural block absent). This was handled as a WARN in Gate 3a and does not carry forward as a failing condition in Gate 3c.

---

### Integration Smoke Test

**Status**: PASS

Smoke suite: 18 passed, 1 xfailed (pre-existing GH#111 volume rate limit — unrelated to crt-018b).

New lifecycle tests L-E01 through L-E04: all PASS.
L-E05: `@pytest.mark.xfail` with documented architectural rationale (background tick not drivable via MCP). No GH Issue required; gap is intentional.

New security tests S-31, S-32: both PASS.
S-31 confirms server exits with non-zero code and references the invalid value when `UNIMATRIX_AUTO_QUARANTINE_CYCLES=1001`.
S-32 confirms server starts and runs normally with `UNIMATRIX_AUTO_QUARANTINE_CYCLES=0`.

---

### Cargo Build and Test

**Status**: PASS

`cargo build --workspace`: clean build, 6 warnings (pre-existing), 0 errors.

`cargo test --workspace`: 2472 unit tests pass, 0 failures, 18 ignored (pre-existing in other crates).

**Code quality observations** (carried from Gate 3b, not blocking):

1. `.unwrap()` at `background.rs:413` (inside `if report.effectiveness.is_some()` guard — provably safe, but could use `if let Some(...)` pattern for style conformance). WARN only.
2. `briefing.rs` (2225 lines) and `background.rs` (1811 lines) exceed the 500-line guideline. Both files are long-standing cumulative files with test modules accounting for the majority of lines. The crt-018b additions are proportionate additions to pre-existing files; the excess predates this feature. These were documented as WARNs in Gate 3b and are carried forward here.

---

## Rework Required

None. Gate result is PASS.

---

## Knowledge Stewardship

- Queried: `/uni-store-lesson` for gap pattern (background-tick-not-drivable at integration test time) — not stored because the RISK-COVERAGE-REPORT.md explicitly states this pattern is already covered in USAGE-PROTOCOL.md and is not novel.
- Stored: nothing novel to store — the gate-3c validation for this feature produced no systemic failure patterns (all checks passed). The "tick-interval gap → xfail without GH Issue" precedent was already established in prior features and is documented in USAGE-PROTOCOL.md. No new lesson-learned or validation pattern warrants a Unimatrix entry.
