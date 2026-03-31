# Gate 3a Report: crt-036

> Gate: 3a (Design Review — rework iteration 1)
> Date: 2026-03-31
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | WARN | `list_purgeable_cycles` signature extends architecture table from `(k) -> Vec<String>` to `(k, max_per_tick) -> (Vec<String>, Option<i64>)`; documented, justified, OVERVIEW.md and pseudocode are internally consistent |
| Specification coverage | PASS | All 12 FRs and 7 NFRs have corresponding pseudocode |
| Risk coverage (test plans) | PASS | All 16 risks map to test scenarios; AC-17 warn message now contains `"retention window"` matching SPEC FR-10 |
| Interface consistency | WARN | Same `list_purgeable_cycles` deviation as Check 1; all other types and signatures consistent across pseudocode and OVERVIEW.md |
| Knowledge stewardship compliance | PASS (with WARN) | Architect report now has `## Knowledge Stewardship` section with `Stored:` entries #3915/#3916/#3917. Pseudocode agent block present but `Stored:` line lacks `-- {reason}` suffix (WARN, non-blocking) |

## Detailed Findings

### Check 1: Architecture Alignment

**Status**: WARN

**Evidence — PASS items:**

- Component boundaries match ARCHITECTURE.md File Placement table exactly: `RetentionConfig` → `infra/config.rs`, GC store methods → `unimatrix-store/src/retention.rs`, GC block → `services/status.rs`, legacy removal → `status.rs` + `tools.rs`, alignment guard → `services/status.rs`. (OVERVIEW.md vs ARCHITECTURE.md §File Placement)
- `store_cycle_review()` with struct update syntax `{ raw_signals_available: 0, ..record }` is correctly specified in `run-maintenance-gc-block.md` step 4c. Matches ARCHITECTURE.md §CycleGcPass and SPECIFICATION.md FR-06 exactly.
- Per-cycle transaction pattern (pool.begin/txn.commit, connection released between cycles) matches ADR-001 (#3915).
- Delete order (observations → query_log → injection_log → sessions) matches ARCHITECTURE.md §FR-03 and ADR-001.
- Step 4f for audit_log correctly placed to avoid collision with sub-steps 4a–4e. Matches ARCHITECTURE.md §run_maintenance step ordering.
- Step 6 (`gc_sessions`) pseudocode note: "unchanged and continues to run" — matches ARCHITECTURE.md §System Overview.
- Both legacy DELETE sites covered: status.rs ~1372–1384 and tools.rs ~1630–1642. Matches ARCHITECTURE.md §Removal of Legacy DELETE Sites.
- PhaseFreqTable guard comparison direction `oldest <= lookback_cutoff_secs` matches ADR-003.
- Technology choices consistent with ADRs (ADR-001 per-cycle tx, ADR-002 max_cycles_per_tick in RetentionConfig, ADR-003 tick-time warn).
- Architect agent report decision 4 (`mark_signals_purged()` targeted UPDATE) contradicts ARCHITECTURE.md which specifies `store_cycle_review()` with struct update syntax. The pseudocode correctly follows ARCHITECTURE.md. This is a narrative inconsistency in the architect report only; pseudocode is authoritative. (Unchanged from previous gate; non-blocking.)

**Evidence — WARN item:**

The ARCHITECTURE.md Integration Surface table (line 215) specifies:
```
SqlxStore::list_purgeable_cycles | async fn list_purgeable_cycles(&self, k: u32) -> Result<Vec<String>>
```

The pseudocode (`cycle-gc-pass.md`) implements:
```rust
pub async fn list_purgeable_cycles(
    &self,
    k: u32,
    max_per_tick: u32,
) -> Result<(Vec<String>, Option<i64>)>
```

This is a documented, intentional deviation. The pseudocode agent report explains the rationale: the `max_per_tick` cap is applied in the SQL LIMIT clause (not at the caller), and the `Option<i64>` returns the oldest retained `computed_at` as a by-product to serve ADR-003 without a second DB round-trip. ARCHITECTURE.md prose at §Component 3 step 2 and ADR-003 ("a by-product of resolving the purgeable set") both support this extension. OVERVIEW.md reflects the extended signature correctly. Implementation agents must use the pseudocode signature.

---

### Check 2: Specification Coverage

**Status**: PASS

**Evidence:**

| FR/NFR | Pseudocode Component | Coverage |
|--------|----------------------|----------|
| FR-01 (RetentionConfig struct) | `retention-config.md` — full struct, defaults, validate(), ConfigError variant, UnimatrixConfig wiring, config.toml | Complete |
| FR-02 (K-cycle resolution) | `cycle-gc-pass.md` — `list_purgeable_cycles` with purgeable query and oldest-retained query | Complete |
| FR-03 (Per-cycle GC transaction) | `cycle-gc-pass.md` — `gc_cycle_activity` with pool.begin/commit, correct delete order, returns CycleGcStats | Complete |
| FR-04 (crt-033 gate) | `run-maintenance-gc-block.md` — `get_cycle_review` with Ok(None) and Err(_) paths, record retained in scope | Complete |
| FR-05 (Unattributed cleanup) | `cycle-gc-pass.md` — `gc_unattributed_activity` with Active status guard (status != 0) | Complete |
| FR-06 (raw_signals_available update) | `run-maintenance-gc-block.md` step 4c — `store_cycle_review(&CycleReviewRecord { raw_signals_available: 0, ..record })` outside transaction | Complete |
| FR-07 (audit_log GC) | `cycle-gc-pass.md` — `gc_audit_log` with Unix seconds arithmetic | Complete |
| FR-08 (Remove legacy DELETE sites) | `legacy-delete-removal.md` — both status.rs and tools.rs sites identified | Complete |
| FR-09 (Structured tracing) | `run-maintenance-gc-block.md` — all required events and fields present | Complete |
| FR-10 (PhaseFreqTable guard) | `phase-freq-table-guard.md` — full algorithm, correct comparison direction, warn message contains "retention window" | Complete |
| FR-11 (config.toml block) | `retention-config.md` — all three fields with documentation comments | Complete |
| FR-12 (Step ordering) | `run-maintenance-gc-block.md` — step 4/4f placement, step 6 unchanged | Complete |
| NFR-01 (Hot path non-blocking) | run_maintenance is background-tick only; no MCP call paths added | Complete |
| NFR-02 (Connection release) | pool.begin inside per-cycle loop body in gc_cycle_activity | Complete |
| NFR-03 (Performance — index use) | EXPLAIN QUERY PLAN test scenario in cycle-gc-pass test plan | Complete |
| NFR-04 (Idempotency) | gc_cycle_activity idempotency documented; zero rows affected on re-run | Complete |
| NFR-05 (No schema migration) | Pseudocode makes no schema changes | Complete |
| NFR-06 (Config loaded once) | RetentionConfig passed by ref into run_maintenance; no per-tick reload | Complete |
| NFR-07 (Observability) | Structured field=value tracing in all events | Complete |

No out-of-scope additions found. All requirements have corresponding pseudocode.

---

### Check 3: Risk Coverage (Test Plans vs Risk-Based Test Strategy)

**Status**: PASS

**Evidence:**

All 16 risks from the Risk Register map to test scenarios in the test plans. The OVERVIEW.md risk-to-AC mapping table covers all R-01 through R-16. All non-negotiable Gate 3c blockers have named test functions.

**Rework item resolved — AC-17 warn message:**

The previous gate report identified that the warn message in `phase-freq-table-guard.md` (pseudocode) did not contain `"retention window"`, causing a mismatch with the test plan assertion (which follows SPECIFICATION.md FR-10).

The updated pseudocode (lines 76-79) now reads:
```
"PhaseFreqTable lookback window ({} days) extends beyond retention window; \
 oldest retained cycle reviewed at {}, lookback cutoff is {}. \
 Consider reducing query_log_lookback_days or increasing \
 activity_detail_retention_cycles.",
```

The string `"retention window"` is present. The test plan assertion in `test-plan/phase-freq-table-guard.md` (line 41) states: "The warning message contains `"retention window"` (per AC-17 specification)." — the pseudocode now matches.

The SPECIFICATION.md FR-10 specifies the message must contain `"retention window"` (AC-17 explicitly requires both `"query_log_lookback_days"` and `"retention window"`). The pseudocode message contains both. PASS.

**Other risk coverage items confirmed:**

- R-01: Two independent grep assertions in test plan OVERVIEW.md (AC-01a, AC-01b). PASS.
- R-02: Order-inversion mutation test in `test_gc_cascade_delete_order` (AC-08). PASS.
- R-03: summary_json preservation check in `test_gc_raw_signals_flag_and_summary_json_preserved` (AC-05). PASS.
- R-04: Concurrent-write sub-assertion in `test_gc_max_cycles_per_tick_cap` (AC-16). PASS.
- R-05: Gate-skip None and Err paths in `test_gc_gate_no_review_row` (AC-04, AC-15). PASS.
- R-07: Active/Closed cases in `test_gc_unattributed_active_guard` (AC-06). PASS.
- R-08: Multi-tick drain in `test_gc_max_cycles_per_tick_cap` (AC-16). PASS.
- R-09: EXPLAIN QUERY PLAN assertions in `test_gc_query_plan_uses_index` (NFR-03). PASS.
- R-10: Boundary tests for all three fields in `test_retention_config_validate_*` (AC-11, AC-12, AC-12b). PASS.
- R-11: Both warning-fires and warning-suppressed cases in `test_gc_phase_freq_table_mismatch_warning` (AC-17). PASS.
- R-12: Both sides of audit_log retention boundary in `test_gc_audit_log_retention_boundary` (AC-09). PASS.
- R-14: Protected tables count + row-level checks (AC-03, AC-14). PASS.
- R-15: Absent `[retention]` block TOML parse test (AC-10). PASS.
- R-16: K-boundary accuracy sub-case in `test_gc_phase_freq_table_mismatch_warning`. PASS.
- R-13: Accepted as low-severity; documented as no dedicated test. PASS (by design).

---

### Check 4: Interface Consistency

**Status**: WARN

**Evidence:**

- `CycleGcStats` fields: pseudocode and OVERVIEW.md agree: `{observations_deleted, query_log_deleted, injection_log_deleted, sessions_deleted}` — matches ARCHITECTURE.md Integration Surface table exactly.
- `UnattributedGcStats` fields: pseudocode and OVERVIEW.md agree — matches ARCHITECTURE.md.
- `RetentionConfig` fields: pseudocode and OVERVIEW.md agree: `{activity_detail_retention_cycles: u32, audit_log_retention_days: u32, max_cycles_per_tick: u32}` with defaults 50/180/10 — matches ARCHITECTURE.md and SPECIFICATION.md FR-01.
- `ConfigError::RetentionFieldOutOfRange` variant: consistent across OVERVIEW.md and pseudocode.
- `run_maintenance()` new parameter `retention_config: &RetentionConfig` — consistent between `run-maintenance-gc-block.md` and ARCHITECTURE.md Integration Surface.
- `run_single_tick()` threading of `Arc<RetentionConfig>` — consistent with background.rs threading section.
- Data flow in OVERVIEW.md matches the run-maintenance pseudocode exactly.
- PhaseFreqTable guard algorithm in pseudocode matches OVERVIEW.md data flow diagram.

**WARN item**: The `list_purgeable_cycles` signature deviation (described in Check 1) is the only interface inconsistency, between the architecture's Integration Surface table and the pseudocode. OVERVIEW.md and all pseudocode files are internally consistent on the extended signature `(k, max_per_tick) -> (Vec<String>, Option<i64>)`.

---

### Check 5: Knowledge Stewardship Compliance

**Status**: PASS (with WARN)

**Evidence:**

**Architect agent** (`crt-036-agent-1-architect-report.md`): Now contains `## Knowledge Stewardship` section (rework applied). Section lists all three ADR entries stored:

| Entry ID | Title | Category |
|----------|-------|----------|
| #3915 | Per-Cycle Transaction Granularity for Activity GC Pass | decision |
| #3916 | max_cycles_per_tick Cap in RetentionConfig, Not InferenceConfig | decision |
| #3917 | PhaseFreqTable / K-cycle Alignment via Tick-Time Diagnostic Warning | decision |

This satisfies the active-storage agent requirement. PASS.

**Pseudocode agent** (`crt-036-agent-1-pseudocode-report.md`): Contains `## Knowledge Stewardship` section with two `Queried:` entries (evidence of pre-implementation Unimatrix queries). The `Stored:` block reads:

> `- Deviations from established patterns:` (followed by deviation descriptions)

There is no explicit `Stored: nothing novel to store -- {reason}` line using the conventional format. Per gate rules: "Present but no reason after 'nothing novel' = WARN." The stewardship section is present and Queried entries are substantive. WARN (non-blocking).

**Risk-strategist agent** (`crt-036-agent-3-risk-report.md`): Contains `## Knowledge Stewardship` with Queried entries and `Stored: nothing novel to store -- {reason}` (with reason). PASS.

**Spec agent** (`crt-036-agent-2-spec-report.md`): Has `## Knowledge Stewardship` with Queried entries and `Stored:` notes. PASS.

---

## Warnings (Non-Blocking)

1. **Architecture Integration Surface table under-specifies `list_purgeable_cycles`**: The table shows `(k: u32) -> Result<Vec<String>>` but the pseudocode correctly implements `(k: u32, max_per_tick: u32) -> Result<(Vec<String>, Option<i64>)>`. The deviation is documented and justified. Implementation agents must use the pseudocode signature.

2. **Pseudocode agent `Stored:` entry lacks conventional `-- {reason}` suffix**: The stewardship block lists deviations inline but does not use the `Stored: nothing novel to store -- {reason}` format. The information is present; the formatting is non-standard.

3. **Architect agent report narrative (decision 4) mentions `mark_signals_purged()` targeted UPDATE** — this contradicts ARCHITECTURE.md which specifies `store_cycle_review()` with struct update syntax. The pseudocode correctly follows ARCHITECTURE.md. Narrative inconsistency only; does not affect implementation.

---

## Knowledge Stewardship

- Stored: nothing novel to store — the warn-message-mismatch pattern (pseudocode deviating from spec's required string in a log message) identified in the initial gate pass is feature-specific. The pattern of resolving it in a rework iteration is already documented in existing gate lesson entries. No new recurrent pattern across 2+ features identified that merits a new Unimatrix lesson entry.
