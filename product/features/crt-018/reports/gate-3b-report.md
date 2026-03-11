# Gate 3b Report: crt-018

> Gate: 3b (Code Review)
> Date: 2026-03-11
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Pseudocode fidelity | PASS | All functions, types, constants match pseudocode. Query 1 SQL improved with subquery dedup (documented rationale). |
| Architecture compliance | PASS | Three-crate boundary maintained. ADR-001 through ADR-004 decisions followed. No new dependencies. |
| Interface implementation | PASS | All 7 public functions + 5 constants + 9 types match architecture integration surface exactly. |
| Test case alignment | PASS | All 28 engine tests (E-01 through E-28) and 18 store tests (S-01 through S-18) implemented per test plans. |
| Code quality | PASS | Compiles clean, no stubs/placeholders, no unwrap() in new non-test code. File sizes within limits for new files (356, 335, 214 lines). |
| Security | PASS | Read-only SQL with parameterized queries. No external input surface. Markdown title sanitization for pipe/newline. |

## Detailed Findings

### Pseudocode Fidelity
**Status**: PASS

**Evidence**:

- **effectiveness-engine**: All 5 constants (`INEFFECTIVE_MIN_INJECTIONS`, `OUTCOME_WEIGHT_*`, `NOISY_TRUST_SOURCES`) match pseudocode values exactly. All 7 types (`EffectivenessCategory`, `EntryEffectiveness`, `SourceEffectiveness`, `CalibrationBucket`, `DataWindow`, `EffectivenessReport`) match field-for-field. All 5 functions (`utility_score`, `classify_entry`, `aggregate_by_source`, `build_calibration_buckets`, `build_report`) follow pseudocode logic precisely.

- **effectiveness-store**: Both `compute_effectiveness_aggregates()` and `load_entry_classification_meta()` match pseudocode. `EffectivenessAggregates` uses flattened scalar fields (session_count, earliest_session_at, latest_session_at) instead of embedded DataWindow, matching the pseudocode's "revised struct" decision to avoid store-to-engine dependency.

- **status-integration**: Phase 8 placement after Phase 7, spawn_blocking pattern, HashMap stats lookup, graceful degradation on Ok(Err) and Err paths, summary/markdown/JSON formatting -- all match pseudocode.

- **Query 1 SQL deviation**: Pseudocode specifies `COUNT(DISTINCT il.session_id)` directly. Implementation uses a subquery `SELECT DISTINCT entry_id, session_id FROM injection_log` before JOIN. This is a correctness improvement: the pseudocode approach would inflate outcome SUM counts when multiple injection_log rows exist for the same (entry, session) pair. The subquery deduplicates before aggregation, preventing R-03 outcome inflation. Comment on line 874-876 documents this rationale. Functionally superior to pseudocode.

- **classify_entry topic mapping**: Implementation maps empty topic to "(unattributed)" inside classify_entry (line 152-156). Pseudocode shows `topic: topic.to_string()` without mapping. However, the store already maps NULL/empty topic to "(unattributed)" in SQL (load_entry_classification_meta line 983). The engine-side mapping is redundant but harmless defensive code -- entries passed directly to classify_entry (not via store) also get correct mapping.

### Architecture Compliance
**Status**: PASS

**Evidence**:

- Component boundaries: Pure computation in unimatrix-engine (effectiveness/mod.rs), SQL aggregation in unimatrix-store (read.rs), orchestration + formatting in unimatrix-server (services/status.rs + mcp/response/status.rs). No cross-boundary violations.
- ADR-001 (consolidated Store method): `compute_effectiveness_aggregates()` runs 4 queries under single `lock_conn()` (line 871).
- ADR-002 (NULL handling): SQL CASE WHEN for "(unattributed)" (line 983), NULL/empty feature_cycle excluded from active_topics (lines 911-913).
- ADR-003 (DataWindow): Present in output via `DataWindow` struct, rendered in all three formats with session count and span.
- ADR-004 (configurable noisy sources): `NOISY_TRUST_SOURCES: &[&str] = &["auto"]` (line 34).
- No schema migration, no new tables, no writes. All read-only SELECT queries.

### Interface Implementation
**Status**: PASS

**Evidence**:

All integration surface entries from the architecture are implemented:
- `Store::compute_effectiveness_aggregates()`: `&self -> Result<EffectivenessAggregates>` (line 870)
- `Store::load_entry_classification_meta()`: `&self -> Result<Vec<EntryClassificationMeta>>` (line 977)
- `effectiveness::classify_entry()`: signature matches all 12 parameters (line 128)
- `effectiveness::build_report()`: `(Vec<EntryEffectiveness>, &[(f64, bool)], DataWindow) -> EffectivenessReport` (line 292)
- `effectiveness::build_calibration_buckets()`: `(&[(f64, bool)]) -> Vec<CalibrationBucket>` (line 249)
- `effectiveness::aggregate_by_source()`: `(&[EntryEffectiveness]) -> Vec<SourceEffectiveness>` (line 189)
- `effectiveness::utility_score()`: `(u32, u32, u32) -> f64` (line 112)
- `StatusReport.effectiveness`: `Option<EffectivenessReport>` (line 99)
- `StatusReportJson.effectiveness`: `#[serde(skip_serializing_if = "Option::is_none")] Option<EffectivenessReportJson>` (lines 678-679)

### Test Case Alignment
**Status**: PASS

**Evidence**:

Engine tests (tests_classify.rs + tests_aggregate.rs):
- E-01 through E-06: Classification priority and boundary tests -- all present
- E-07 through E-13: Calibration bucket boundary tests -- all present, plus extra clamping tests for negative and >1.0 confidence
- E-14 through E-16b: utility_score tests including overflow guard -- all present
- E-17 through E-19: Settled classification tests -- all present
- E-20 through E-21: Noisy trust source matching -- all present
- E-22 through E-24: aggregate_by_source tests -- all present
- E-25 through E-28: build_report cap/empty tests -- all present
- Extra: helpfulness_ratio computation test (line 319)

Store tests (read.rs tests):
- S-01 through S-10: All compute_effectiveness_aggregates scenarios present
- S-11: Single lock_conn scope verified by code review (line 871, all 4 queries use same conn)
- S-12 through S-15: load_entry_classification_meta tests -- all present
- S-16 through S-17: Empty database tests -- present
- S-18: Performance at scale (500 entries, 10K injections, 500ms budget) -- present

### Code Quality
**Status**: PASS

**Evidence**:

- `cargo build --workspace`: Compiles with 0 errors, 4 warnings (pre-existing in unimatrix-server)
- `cargo test --workspace`: 2115 tests pass, 0 failures
- No `todo!()`, `unimplemented!()`, `TODO`, `FIXME` in any crt-018 code
- No `.unwrap()` in new non-test code. The two `.unwrap()` calls at lines 502 and 521 of status.rs are pre-existing Phase 6/7 patterns on spawn_blocking JoinHandles, not crt-018 code. Phase 8 uses explicit match instead.
- File sizes for new files: mod.rs (356), tests_classify.rs (335), tests_aggregate.rs (214) -- all well under 500 lines
- Pre-existing files: status.rs (1013), services/status.rs (835), read.rs (1653) -- these exceed 500 lines but are pre-existing files with crt-018 additions. The pre-existing status.rs was already over 500 lines before crt-018.

### Security
**Status**: PASS

**Evidence**:

- No external input surface: crt-018 adds no new MCP tool parameters. All data comes from internal SQLite tables.
- SQL injection: All queries use parameterized statements via rusqlite (no string interpolation of user data).
- No path traversal: No file operations in crt-018 code.
- No command injection: No process invocations.
- No hardcoded secrets or credentials.
- Markdown table injection (R-12): Entry titles sanitized via `.replace('|', "/").replace('\n', " ")` in both ineffective, noisy, and unmatched entry rendering (lines 583, 596, 606-607 of status.rs).
- `cargo audit`: Not installed in this environment; not a crt-018 regression.

## Warnings

1. **Pre-existing file sizes over 500 lines**: `status.rs` (1013 lines), `services/status.rs` (835 lines), `read.rs` (1653 lines) were already over the limit before crt-018. The crt-018 additions are modest (~80 lines in status.rs formatting, ~80 lines in services/status.rs Phase 8, ~200 lines of store methods + structs + tests in read.rs). These are not blockers since the files were pre-existing.

2. **Calibration uses binary success, not weighted**: FR-04 specifies calibration actual_success_rate should use "weighted outcomes: success=1.0, rework=0.5, abandoned=0.0". The implementation uses `(f64, bool)` where bool = (outcome == 'success'), treating rework as not-success. This matches the architecture's `(f64, bool)` type specification and was explicitly noted in the pseudocode as an architecture-wins decision. The spec intent would require `(f64, f64)` type. This was a conscious design decision documented in the pseudocode review.

## Rework Required

None.
