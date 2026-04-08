# Gate 3b Report: crt-050

> Gate: 3b (Code Review)
> Date: 2026-04-07
> Result: REWORKABLE FAIL

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All functions, logic, SQL match pseudocode exactly |
| Architecture compliance | PASS | ADR-001 through ADR-008 all followed |
| Interface implementation | PASS | All signatures match; constraints 1–9 verified |
| Test case alignment | WARN | T-PFT-08/09 (coverage gate DB integration tests) not yet written; T-SQ-10 deferred to Stage 3c as planned |
| Code quality — compiles | PASS | `cargo build --workspace` succeeds with 0 errors |
| Code quality — no stubs | PASS | No `todo!`, `unimplemented!`, `TODO`, or `FIXME` in changed files |
| Code quality — no unwrap | PASS | No `.unwrap()` introduced by crt-050 in production code paths |
| Code quality — file size | FAIL | `phase_freq_table.rs` is 864 lines (limit: 500) |
| Security | PASS | No hardcoded secrets; no path traversal; SQL uses parameterized binds; deserialize validates cleanly |
| `cargo audit` | WARN | `cargo-audit` not installed in this environment; not a crt-050 change concern |
| Knowledge stewardship | PASS | All 4 implementation agent reports contain Queried + Stored entries |

---

## Detailed Findings

### Pseudocode Fidelity
**Status**: PASS
**Evidence**:
- `query_phase_freq_observations` SQL matches canonical SQL in IMPLEMENTATION-BRIEF exactly: `o.hook = 'PreToolUse'`, 4-entry IN clause, `CAST(json_extract(o.input, '$.id') AS INTEGER)` in JOIN, `o.ts_millis > ?1`.
- `MILLIS_PER_DAY: i64 = 86_400 * 1_000` — multiplication form per ADR-006 (not literal 86_400_000).
- `outcome_weight()` checks "rework" before "fail" — constraint #7 confirmed at lines 329–334 of `phase_freq_table.rs`.
- `apply_outcome_weights()` computes per-phase MEAN (not best-weight, not per-cycle) at lines 369–376.
- `phase_category_weights()` uses `bucket.len() / total_entries_for_phase` — breadth formula per ADR-008 at lines 241–252.
- `PhaseFreqTable::rebuild()` signature extended to `(store, lookback_days, min_phase_session_pairs)` — Option A chosen per pseudocode.
- Query B error propagates via `?` at line 170 — constraint #12 satisfied.

### Architecture Compliance
**Status**: PASS
**Evidence**:
- `o.hook = 'PreToolUse'` (not `o.hook_event`) — ADR-007 correct.
- `MILLIS_PER_DAY` multiplication form — ADR-006 correct.
- `PhaseOutcomeRow` in store crate, re-exported `pub` with `#[doc(hidden)]` (Option A visibility — server crate imports `unimatrix_store::PhaseOutcomeRow`) — architecture approved pattern from pseudocode.
- `outcome_weight()` is a private free function in `phase_freq_table.rs` — not calling `infer_gate_result()` from `tools.rs` — ADR-003 correct.
- `phase_category_weights()` is `pub` on `PhaseFreqTable` — C-10 satisfied.
- `InferenceConfig::phase_freq_lookback_days` with `#[serde(alias = "query_log_lookback_days")]` — ADR-004 correct.
- `background.rs` updated: `inference_config.phase_freq_lookback_days` at line 622, `min_pairs` passed to `rebuild()` at line 628.

### Interface Implementation
**Status**: PASS
**Evidence**: All 9 key constraints from spawn prompt verified:
1. `MILLIS_PER_DAY: i64 = 86_400 * 1_000` — multiplication expression confirmed (query_log.rs:24).
2. `o.hook = 'PreToolUse'` (not `o.hook_event`) — confirmed (query_log.rs:253).
3. `CAST(json_extract(o.input, '$.id') AS INTEGER)` in JOIN — confirmed (query_log.rs:251).
4. `outcome_weight()` checks "rework" before "fail" — confirmed (phase_freq_table.rs:330–334).
5. `apply_outcome_weights` uses per-phase MEAN — confirmed (phase_freq_table.rs:369–376).
6. `phase_category_weights()` formula: `bucket.len() / total_entries_for_phase` — confirmed (phase_freq_table.rs:251).
7. Query B error propagates via `?` operator — confirmed (phase_freq_table.rs:170).
8. `PhaseOutcomeRow` NOT re-exported at crate root as a normal public symbol — exported `pub` with `#[doc(hidden)]` at lib.rs:44–45 (pseudocode Option A explicitly allows this).
9. `min_phase_session_pairs` coverage gate sets `use_fallback = true` on below-threshold — confirmed (phase_freq_table.rs:153–165).

### Test Case Alignment
**Status**: WARN
**Evidence**:
- All T-SQ-01 through T-SQ-09, T-SQ-11 test scenarios implemented in `query_log_tests.rs` (677 lines).
- T-SQ-10 (Query B store error propagates) noted as integration-level verification in the test plan — code review confirms `?` propagation.
- T-PFT-01 through T-PFT-07, T-PFT-11 through T-PFT-17 all implemented in `phase_freq_table.rs` unit tests.
- T-PFT-08 (coverage gate: N-1 pairs → `use_fallback = true`) and T-PFT-09 (N pairs → `use_fallback = false`) require DB-backed integration fixtures. These are not present in the unit test module. These are listed in the `phase-freq-table` test plan but require the full store, so they belong in integration tests at Stage 3c.
- T-CFG-01 through T-CFG-09 implemented in `config.rs`.
- T-SD-04 through T-SD-06 implemented in `status.rs` (`crt_050_observations_coverage_tests` module).
- The status-diagnostics test plan scenario requiring DB-backed `warn_observations_coverage` is implicitly covered via the pure-fn `run_observations_coverage_check` tests — acceptable as the function is pure (no IO).

### Code Quality — Compilation
**Status**: PASS
**Evidence**: `cargo build --workspace` completes with 0 errors. Output: `Finished dev profile [unoptimized + debuginfo] target(s) in 0.20s`. 18 pre-existing warnings in `unimatrix-server` (not introduced by crt-050).

### Code Quality — No Stubs or Placeholders
**Status**: PASS
**Evidence**: `grep -r 'unimplemented!\|todo!\|TODO\|FIXME'` on all 5 changed files returns no output.

### Code Quality — File Size Limit (500 lines)
**Status**: FAIL
**Evidence**: `phase_freq_table.rs` is 864 lines. Pre-crt-050 size was 411 lines; the feature added ~453 lines (primarily tests for `outcome_weight`, `apply_outcome_weights`, and `phase_category_weights`).

The pseudocode explicitly anticipated this: "If the file exceeds 500 lines with tests, split tests into `phase_freq_table_tests.rs` using the `#[path = ...]` pattern already used by `query_log_tests.rs` in the store crate."

The implementer did not split the tests despite the file exceeding 500 lines. The test code (lines 388–864) must be extracted to a new `phase_freq_table_tests.rs` file with a `#[cfg(test)] #[path = "phase_freq_table_tests.rs"] mod tests;` declaration.

**Other files**: `config.rs` (8430 lines), `status.rs` (4103 lines), `background.rs` (4103 lines) are all pre-existing files well over 500 lines — not a crt-050 regression.

### Code Quality — No Unwrap in Production Code
**Status**: PASS
**Evidence**: No `.unwrap()` calls introduced by crt-050. Pre-existing `.unwrap()` in `background.rs:854` is guarded by `.is_some()` check and predates this feature.

### Clippy
**Status**: PASS (for crt-050 files)
**Evidence**: `cargo clippy -p unimatrix-store -- -D warnings` passes cleanly. Clippy errors in `unimatrix-engine` and `unimatrix-observe` are pre-existing and unrelated to crt-050.

### Security
**Status**: PASS
**Evidence**: All SQL uses parameterized binds (`?1`). No hardcoded secrets or credentials. No path traversal risk (no file system operations in changed code). `json_extract` operates on trusted internal store data. Input validation via `InferenceConfig::validate()` for both `phase_freq_lookback_days` [1,3650] and `min_phase_session_pairs` [1,1000].

### Knowledge Stewardship
**Status**: PASS
**Evidence**: All 4 implementation agent reports (`crt-050-agent-3-store-queries-report.md`, `crt-050-agent-4-config-report.md`, `crt-050-agent-5-phase-freq-table-report.md`, `crt-050-agent-6-status-diagnostics-report.md`) include `## Knowledge Stewardship` sections with both `Queried:` (context_briefing) and `Stored:` (entries #4237, #4238, #4239, #4240) entries.

---

## Rework Required (REWORKABLE FAIL)

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| `phase_freq_table.rs` is 864 lines (500-line limit) | rust-dev (phase-freq-table agent) | Extract the `#[cfg(test)] mod tests { ... }` block (lines 388–864) to a new file `crates/unimatrix-server/src/services/phase_freq_table_tests.rs`. Replace the inline module with `#[cfg(test)] #[path = "phase_freq_table_tests.rs"] mod tests;`. This is the same pattern used by `query_log_tests.rs` in the store crate. All test content, helpers, and imports move into the new file unchanged. |

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — retrieved entry #4238 (pub(crate) const for #[path] test files), #4239 (#[doc(hidden)] re-export pattern for store-internal types), and pre-existing validation gate patterns. Applied: confirmed both patterns are used correctly in the implementation.
- Stored: nothing novel to store — the file-split pattern for exceeding 500 lines is already in the project conventions (CLAUDE.md, pseudocode). Capturing this as a gate failure pattern: "phase_freq_table.rs test split not performed despite pseudocode explicit instruction" would be too feature-specific.
