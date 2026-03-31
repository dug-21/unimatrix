# Gate 3b Report: crt-036

> Gate: 3b (Code Review)
> Date: 2026-03-31
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All four store methods match pseudocode exactly; GC block orchestration matches run-maintenance-gc-block.md |
| Architecture compliance | PASS | Component boundaries, ADR-001 (per-cycle transaction), ADR-002 (max_cycles_per_tick placement), ADR-003 (alignment guard) all followed |
| Interface implementation | PASS | All signatures from Integration Surface table match; RetentionConfig wired into UnimatrixConfig; run_maintenance() and run_single_tick() updated correctly |
| Test case alignment | WARN | AC-15 (test_gc_tracing_output) named in test plan but not implemented as a traced test; gate-skip behavior covered by test_gc_gate_no_review_row structurally |
| Code quality — compilation | PASS | `cargo build --workspace` succeeds with zero errors |
| Code quality — stubs | PASS | No todo!(), unimplemented!(), TODO, FIXME present in new files |
| Code quality — unwrap | PASS | No .unwrap() in production code (lines 1-285 of retention.rs); all .unwrap() in #[cfg(test)] blocks |
| Code quality — file size | WARN | retention.rs is 1435 lines total; production code is ~285 lines (under limit); 1145 lines are #[cfg(test)]; project convention is tests-in-file; comparable existing file cycle_review_index.rs is 858 lines |
| Security | PASS | No hardcoded secrets; input binds use parameterized queries; no path traversal; no command injection |
| cargo audit | WARN | cargo-audit not installed in this environment; cannot run CVE check |
| Key check AC-01a | PASS | `DELETE FROM observations WHERE ts_millis` absent from status.rs (grep confirms 0 matches) |
| Key check AC-01b | PASS | `DELETE FROM observations WHERE ts_millis` absent from tools.rs (grep confirms 0 matches) |
| Key check — raw_signals_available update | PASS | Uses `store_cycle_review(&CycleReviewRecord { raw_signals_available: 0, ..record })` with retained record from gate check |
| Key check — gate-check record retained | PASS | `record` set in Ok(Some(r)) => r arm; `..record` used in step 4c store_cycle_review call; never reconstructed |
| Key check — pool.begin()/tx.commit() | PASS | gc_cycle_activity uses `self.write_pool_server().begin()` / `txn.commit()` |
| Key check — AC-17 warn message | PASS | Message text contains "retention window"; structured field `query_log_lookback_days` present; test asserts both |
| Key check — step 6 gc_sessions unchanged | PASS | gc_sessions still present at line ~1600-1605 in status.rs |
| Key check — no mark_signals_purged() | PASS | grep across crates/ returns zero matches |
| All tests pass | PASS | `cargo test --workspace` passes all 2541+ tests with 0 failures |
| Knowledge stewardship | PASS | All four rust-dev agent reports have Queried: and Stored:/nothing novel entries |

## Detailed Findings

### Pseudocode Fidelity
**Status**: PASS
**Evidence**: All four store methods in `retention.rs` match the pseudocode in `cycle-gc-pass.md`:
- `list_purgeable_cycles(k, max_per_tick)` — SQL, parameter binding, and return type `(Vec<String>, Option<i64>)` match exactly
- `gc_cycle_activity(feature_cycle)` — pool.begin()/txn.commit() transaction, delete order (obs → qlog → ilog → sessions), CycleGcStats return match
- `gc_unattributed_activity()` — steps 1-4 SQL statements match; implementation uses a single acquired connection across all 4 operations (deviation from pseudocode's per-call `self.write_pool_server()`) but this is a valid optimization for a max_connections=1 pool
- `gc_audit_log(retention_days)` — SQL and return type match exactly

The `run_maintenance()` GC block in `status.rs` matches `run-maintenance-gc-block.md` including the labeled block `'gc_cycle_block`, the error handling for list_purgeable_cycles failure (break to step 4f), the gate check, the gc_cycle_activity call, the store_cycle_review step outside the transaction, and the audit_log 4f step.

### Architecture Compliance
**Status**: PASS
**Evidence**:
- ADR-001: Per-cycle transaction via pool.begin()/tx.commit() confirmed in gc_cycle_activity() lines 120-179 of retention.rs
- ADR-002: max_cycles_per_tick is a field of RetentionConfig (not InferenceConfig) as specified
- ADR-003: PhaseFreqTable alignment guard implemented as private function `run_phase_freq_table_alignment_check` called at start of step 4
- ADR-001 consequence: raw_signals_available update runs OUTSIDE the per-cycle transaction using store_cycle_review with struct update syntax
- Both legacy 60-day DELETE sites removed unconditionally (AC-01a, AC-01b verified)
- Schema stays at v19 — no migration (NFR-05 confirmed: no migration files added)
- write_pool_server() used for all GC writes; read_pool() used for list_purgeable_cycles (read-only)

### Interface Implementation
**Status**: PASS
**Evidence** (checked against Integration Surface table in ARCHITECTURE.md):
- `RetentionConfig` struct: three fields with correct types, defaults, and serde attributes — config.rs lines 1062-1098
- `RetentionConfig::validate`: validates all three fields with named error variant — config.rs lines 1130-1161
- `UnimatrixConfig::retention`: `pub retention: RetentionConfig` with `#[serde(default)]` — config.rs line 82
- `list_purgeable_cycles`: signature `async fn (&self, k: u32, max_per_tick: u32) -> Result<(Vec<String>, Option<i64>)>` matches
- `gc_cycle_activity`: signature `async fn (&self, feature_cycle: &str) -> Result<CycleGcStats>` matches
- `gc_unattributed_activity`: signature `async fn (&self) -> Result<UnattributedGcStats>` matches
- `gc_audit_log`: signature `async fn (&self, retention_days: u32) -> Result<u64>` matches
- `run_maintenance()` adds `retention_config: &RetentionConfig` as final parameter
- `run_single_tick()` in background.rs adds `retention_config: &Arc<RetentionConfig>` parameter
- main.rs creates `Arc::new(config.retention.clone())` and passes it to run_tick_loop
- `validate_config()` calls `config.retention.validate(path)?` alongside `config.inference.validate(path)?`

### Test Case Alignment
**Status**: WARN
**Evidence**: All AC tests from test-plan/OVERVIEW.md are present and passing, with one gap:

**Present and passing (cargo test confirms all run):**
- AC-02: test_gc_cycle_based_pruning_correctness (retention.rs)
- AC-03: test_gc_protected_tables_regression (retention.rs)
- AC-04: test_gc_gate_no_review_row (status.rs)
- AC-05: test_gc_raw_signals_flag_and_summary_json_preserved (status.rs)
- AC-06: test_gc_unattributed_active_guard (retention.rs)
- AC-07: test_gc_query_log_pruned_with_cycle (retention.rs)
- AC-08: test_gc_cascade_delete_order (retention.rs)
- AC-09: test_gc_audit_log_retention_boundary (retention.rs)
- AC-10: test_retention_config_defaults_and_override (config.rs)
- AC-11: test_retention_config_validate_rejects_zero_retention_cycles (config.rs)
- AC-12: test_retention_config_validate_rejects_zero_audit_days (config.rs)
- AC-12b: test_retention_config_validate_rejects_invalid_max_cycles (config.rs)
- AC-13: doc comment on activity_detail_retention_cycles contains "PhaseFreqTable lookback" and "GNN training window" — confirmed at config.rs lines 1066-1067
- AC-14: test_gc_protected_tables_row_level (retention.rs)
- AC-16: test_gc_max_cycles_per_tick_cap (status.rs)
- AC-17: test_gc_phase_freq_table_mismatch_warning_fires + 3 related tests (status.rs, using #[tracing_test::traced_test])

**Gap — AC-15 (test_gc_tracing_output):**
The test plan names `test_gc_tracing_output` as the test for AC-15. No such test function exists in the codebase. AC-15 requires assertions that: (1) `info` log with `purgeable_count` fires at pass start, (2) `info` log with `observations_deleted` and `cycle_id` fires per pruned cycle, (3) `info` log with `cycles_pruned` fires at pass completion, (4) `warn` log with cycle ID fires on gate skip.

The warn path (item 4) is tested structurally by `test_gc_gate_no_review_row` (AC-04 covers data protection but not the log assertion). Items 1-3 have no test coverage. AC-15 is not listed as a "non-negotiable Gate 3c blocker" in RISK-TEST-STRATEGY, so this does not block Gate 3b, but should be addressed before Gate 3c.

### Code Quality — Compilation
**Status**: PASS
**Evidence**: `cargo build --workspace` completes with `Finished dev profile` and 0 errors. Warnings are pre-existing (UsageService derived Clone dead code warning in unimatrix-server).

### Code Quality — File Size
**Status**: WARN
**Evidence**: `retention.rs` is 1435 lines total. Production code ends at line 285 (well under 500). Lines 291-1435 are `#[cfg(test)]` test module. This is consistent with the project convention (cycle_review_index.rs is 858 lines; sessions.rs is 350 lines). The pre-existing modified files (status.rs at 3411, config.rs at 6714, background.rs at 4229, tools.rs at 6609) were all over the limit before this feature. The 500-line rule is intended to prevent production code bloat; the inline test pattern is established project convention. Not a blocking failure.

### Security
**Status**: PASS
**Evidence**:
- All SQL uses parameterized binding (`bind(feature_cycle)`, `bind(k as i64)`, etc.) — no string interpolation in SQL
- No hardcoded secrets, credentials, or API keys
- No file path operations (retention.rs is pure SQL)
- No command injection surfaces
- No panic-capable deserialization paths

### cargo audit
**Status**: WARN
**Evidence**: `cargo-audit` binary is not installed in this development environment (`cargo audit` returns "no such command"). No CVE verification possible at this gate. Should be run in CI before merge.

### Key Check: raw_signals_available update pattern
**Status**: PASS
**Evidence** (status.rs lines 1468-1473):
```rust
if let Err(e) = self
    .store
    .store_cycle_review(&CycleReviewRecord {
        raw_signals_available: 0,
        ..record
    })
    .await
```
The `record` variable is set at the gate check step (line 1422: `Ok(Some(r)) => r`), retained through `gc_cycle_activity`, and consumed here with struct update syntax. No mark_signals_purged() method exists anywhere in the codebase (grep confirmed zero matches across all crates).

### Key Check: gate-check record retained in scope
**Status**: PASS
**Evidence**: The `record` variable from the gate check (`Ok(Some(r)) => r`) is used in the `store_cycle_review` call with `..record`. The `continue` paths (gate skip and gc_cycle_activity error) both bypass the `store_cycle_review` call, which is correct per architecture (data not deleted = flag must not be updated).

### Key Check: per-cycle transaction uses pool.begin()/tx.commit()
**Status**: PASS
**Evidence** (retention.rs lines 120-179): `self.write_pool_server().begin().await?` acquires the transaction; `txn.commit().await?` commits. All four DELETEs execute on `&mut *txn`. This satisfies Architecture constraint 5 (entry #2159 pattern) and ADR-001.

### Key Check: AC-17 warn message content
**Status**: PASS
**Evidence** (status.rs lines 1660-1672):
```rust
tracing::warn!(
    query_log_lookback_days = query_log_lookback_days,
    ...
    "PhaseFreqTable lookback window ({} days) extends beyond retention window; ...",
```
Structured field `query_log_lookback_days` is present. Message text contains "retention window". The test `test_gc_phase_freq_table_mismatch_warning_fires` uses `#[tracing_test::traced_test]` and asserts `logs_contain("query_log_lookback_days")` and `logs_contain("retention window")`. Both assertions pass.

### Key Check: step 6 gc_sessions unchanged
**Status**: PASS
**Evidence** (status.rs lines 1600-1605): `gc_sessions(TIMED_OUT_THRESHOLD_SECS, DELETE_THRESHOLD_SECS)` still present. The architecture specifies step 6 is unchanged because it handles sessions with no feature_cycle attribution — the cycle-based GC at step 4 and time-based GC at step 6 target disjoint session populations.

### Key Check: no mark_signals_purged() method
**Status**: PASS
**Evidence**: `grep` across `/workspaces/unimatrix/crates` for `mark_signals_purged` returns zero matches. The architecture specifies this method should NOT be added; `store_cycle_review` with struct update syntax is the correct approach.

### Knowledge Stewardship
**Status**: PASS
**Evidence**: All four rust-dev implementation agent reports contain `## Knowledge Stewardship` sections:
- crt-036-agent-3-retention-config-report.md: Queried context_briefing; Stored entry #3928 (nested config test pattern)
- crt-036-agent-4-cycle-gc-pass-report.md: Queried context_briefing (entries #3914, #3799, #3915 applied); Stored entry #3929 (observation_phase_metrics FK constraint)
- crt-036-agent-5-legacy-delete-tools-report.md: Queried context_briefing; Stored: nothing novel (pure deletion, with reason)
- crt-036-agent-6-run-maintenance-report.md: Queried context_briefing; Stored entry #3930 (list_purgeable_cycles includes already-purged cycles behavioral trap)

## Rework Required

None. All FAIL-level checks passed. Two WARNs do not block progress:

| Issue | Severity | Recommendation |
|-------|----------|----------------|
| AC-15 test_gc_tracing_output missing | WARN | Add a #[tracing_test::traced_test] test that asserts structured log fields: purgeable_count at pass start, cycle_id+observations_deleted per cycle, cycles_pruned at pass complete, and warn with cycle_id on gate skip. Not required before Gate 3c per RISK-TEST-STRATEGY blockers list. |
| cargo-audit not available | WARN | Run `cargo audit` in CI environment before merge to verify no known CVEs in dependencies. |
| retention.rs 1435 lines (tests included) | WARN | Project convention is tests-in-file; production code is 285 lines. No action required. |

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- entry #3914 (two-hop join pattern), #3799 (acquire before execute), #3793 (write_pool_server constraint), #3686 (PhaseFreqTable lookback) confirmed as directly applicable context for gate review
- Stored: nothing novel to store -- gate 3b pass with only minor warnings; no systemic failure patterns discovered; specific findings (AC-15 gap, inline test file size) are feature-specific
