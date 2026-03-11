# Gate 3a Report: crt-018

> Gate: 3a (Component Design Review)
> Date: 2026-03-11
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All three components match architecture decomposition, interfaces, ADRs |
| Specification coverage | WARN | Calibration weighted outcomes (FR-04) vs architecture bool type -- flagged by pseudocode, architecture wins |
| Risk coverage | PASS | All 13 risks mapped to test scenarios with correct emphasis |
| Interface consistency | PASS | Shared types, data flow, and boundaries consistent across all pseudocode files |

## Detailed Findings

### 1. Architecture Alignment
**Status**: PASS

**Evidence**:

Component boundaries match architecture exactly:
- `effectiveness-engine` in `unimatrix-engine/src/effectiveness.rs` -- pure computation, zero I/O (Architecture Component 1)
- `effectiveness-store` in `unimatrix-store/src/read.rs` -- SQL aggregation via `compute_effectiveness_aggregates()` + `load_entry_classification_meta()` (Architecture Component 2)
- `status-integration` in `unimatrix-server` -- Phase 8 in `compute_report`, formatting in all three response formats (Architecture Component 3)

ADR compliance verified:
- **ADR-001** (consolidated query): Pseudocode uses single `compute_effectiveness_aggregates()` with one `lock_conn()` and 4 sequential SQL queries. Store pseudocode lines 63-142 match ADR-001's pattern.
- **ADR-002** (NULL topic handling): Store pseudocode uses `CASE WHEN topic IS NULL OR topic = '' THEN '(unattributed)' ELSE topic END` in SQL (line 164). Active topics query excludes NULL/empty feature_cycle (line 96).
- **ADR-003** (data window): `DataWindow` struct present in engine types and populated in store query 4. Output includes session count and span in all three formats.
- **ADR-004** (configurable noisy sources): `NOISY_TRUST_SOURCES: &[&str] = &["auto"]` constant with `.contains()` check in `classify_entry` (line 128).

Technology choices consistent: rusqlite for SQL, spawn_blocking for async safety, serde for JSON serialization, no new external dependencies.

Data flow matches architecture diagram: `StatusService -> spawn_blocking -> Store (SQL) -> Engine (classify) -> build_report -> StatusReport.effectiveness -> format_status_report`.

The store pseudocode correctly revised `EffectivenessAggregates` to use raw scalars (session_count, earliest_session_at, latest_session_at) instead of importing `DataWindow` from engine, avoiding a store-to-engine compile-time dependency. The server constructs `DataWindow` at the integration point. This is a sound design choice consistent with the architecture's statement that store and engine have no compile-time dependency on each other.

### 2. Specification Coverage
**Status**: WARN

**Evidence**:

All functional requirements have corresponding pseudocode:

| FR | Pseudocode Coverage |
|----|-------------------|
| FR-01 (classification) | `classify_entry` function with priority chain Noisy > Ineffective > Unmatched > Settled > Effective |
| FR-02 (weighted success) | `utility_score` function with named constants |
| FR-03 (aggregate by source) | `aggregate_by_source` function |
| FR-04 (calibration) | `build_calibration_buckets` function -- see WARN below |
| FR-05 (summary format) | Summary format pseudocode with one-liner |
| FR-06 (markdown format) | Markdown format pseudocode with all tables |
| FR-07 (JSON format) | JSON format pseudocode with EffectivenessReportJson and sub-structs |
| FR-08 (pure module) | effectiveness.rs is pure, no I/O |
| FR-09 (store methods) | Two Store methods with SQL queries |
| FR-10 (StatusService Phase 8) | Phase 8 pseudocode with spawn_blocking |

Non-functional requirements addressed:
- NFR-01 (performance): SQL-side GROUP BY, existing indexes used, test S-18 covers 500ms budget
- NFR-02 (no migration): No new tables/columns, queries use existing schema
- NFR-03 (read-only): All queries are SELECT, test I-03/AC-13 verifies no writes
- NFR-04 (async safety): spawn_blocking wrapping verified
- NFR-05 (output size): skip_serializing_if on JSON, top-10 caps on lists
- NFR-06 (graceful degradation): Three-way match on spawn_blocking result, None on failure

No scope additions detected -- pseudocode implements only what the specification requires.

**WARN**: FR-04 specifies calibration buckets use "weighted outcomes: success=1.0, rework=0.5, abandoned=0.0" for actual_success_rate. However, the architecture defines `calibration_rows: Vec<(f64, bool)>` -- a bool cannot represent the 0.5 weight for rework. The pseudocode correctly identifies this discrepancy (effectiveness-engine.md lines 244-246) and follows the architecture (bool approach: rework = false). This means rework sessions are treated as failures in calibration, not weighted at 0.5. The pseudocode flags this as an open question for review. This is a minor fidelity gap between spec and architecture that does not block progress -- the architecture's type was an intentional simplification, and the pseudocode follows it correctly.

Acceptance criteria coverage is thorough -- the test plan OVERVIEW maps every AC to specific test IDs.

### 3. Risk Coverage
**Status**: PASS

**Evidence**:

All 13 risks from the Risk-Based Test Strategy are mapped to test scenarios:

| Risk | Priority | Test IDs | Adequate? |
|------|----------|----------|-----------|
| R-01 (priority ordering) | Critical | E-01 through E-05 | Yes -- all pairwise overlaps tested, boundary at 30% and INEFFECTIVE_MIN_INJECTIONS covered |
| R-02 (NULL topic/feature_cycle) | Critical | S-05 through S-08, E-06, S-13 | Yes -- full NULL/empty matrix for both entries and sessions |
| R-03 (COUNT DISTINCT) | High | S-01 through S-03 | Yes -- duplicate injection dedup, distinct session counting, NULL outcome exclusion |
| R-04 (calibration boundaries) | High | E-07 through E-13 | Yes -- every 0.1 boundary plus floating point edge and empty data |
| R-05 (division by zero) | High | E-14 through E-16b | Yes -- zero denominator, pure success, mixed, large values |
| R-06 (query performance) | Medium | S-09, S-10, S-18 | Yes -- calibration row count verified, data window tested, 500ms benchmark |
| R-07 (GC race) | Medium | S-11 | Yes -- code review verification of single lock_conn scope |
| R-08 (JSON compatibility) | Medium | I-04 through I-07 | Yes -- both present and absent cases, all three format outputs |
| R-09 (Settled logic) | Medium | E-17 through E-19 | Yes -- inactive topic with/without success injection, zero injections |
| R-10 (case sensitivity) | Low | E-20, E-21 | Yes -- matching and non-matching trust sources |
| R-11 (spawn_blocking failure) | Medium | I-08, I-09 | Yes -- store error path, code review for unwrap(), JoinError handling |
| R-12 (markdown injection) | Low | I-10 | Yes -- pipe character in title |
| R-13 (NaN in aggregate) | Medium | E-22 through E-24 | Yes -- zero-injection source, empty entries |

Integration risks from RISK-TEST-STRATEGY are also covered:
- Store-to-Engine data contract: verified by full pipeline test I-01
- Entry metadata JOIN consistency: noted in test plan (orphaned entry_id handling)
- Phase 8 ordering independence: test I-09

Edge cases from RISK-TEST-STRATEGY are covered:
- Empty knowledge base: S-16, S-17, E-28
- All entries Unmatched: covered by classification logic
- Single entry/session: covered by S-01/S-02 variants
- NULL outcome sessions excluded: S-03
- Confidence boundaries: E-07 through E-13
- u32 overflow: E-16b

Risk priorities are correctly reflected in test plan emphasis -- R-01 and R-02 (Critical) have the most test scenarios (5 and 6+ respectively), while Low risks have 2-3 scenarios each.

### 4. Interface Consistency
**Status**: PASS

**Evidence**:

Shared types defined in OVERVIEW.md match per-component usage:

**Store -> Engine boundary** (server is the integration point):
- `EffectivenessAggregates` defined in effectiveness-store pseudocode matches OVERVIEW.md description. The revised struct uses raw scalars instead of DataWindow to avoid cross-crate dependency -- documented and consistent.
- `EntryInjectionStats` fields (entry_id, injection_count, success_count, rework_count, abandoned_count) consistent between store definition and server usage in Phase 8.
- `EntryClassificationMeta` fields (entry_id, title, topic, trust_source, helpful_count, unhelpful_count) consistent between store definition and classify_entry parameter list.

**Engine -> Server boundary**:
- `EffectivenessCategory` enum used in engine and referenced in server formatting code
- `EntryEffectiveness` fields match between engine definition and server/JSON mapping
- `SourceEffectiveness` fields match between engine definition and SourceEffectivenessJson mapping
- `CalibrationBucket` fields match between engine definition and CalibrationBucketJson mapping
- `DataWindow` constructed in server from raw store aggregates, used by engine's build_report
- `EffectivenessReport` fields match between engine definition and server consumption

**Constants** defined once in engine, used by engine functions:
- INEFFECTIVE_MIN_INJECTIONS used in classify_entry
- OUTCOME_WEIGHT_* used in utility_score
- NOISY_TRUST_SOURCES used in classify_entry (passed as parameter, referenced by server via import)

Data flow coherence verified:
1. Store produces `EffectivenessAggregates` (entry_stats, active_topics, calibration_rows, session_count, earliest/latest)
2. Store produces `Vec<EntryClassificationMeta>` (entry metadata)
3. Server builds HashMap from entry_stats, iterates entry_meta, calls classify_entry per entry
4. Server constructs DataWindow from raw aggregates
5. Engine's build_report assembles EffectivenessReport from classifications + calibration_rows + DataWindow
6. Server sets StatusReport.effectiveness = Some(report)
7. Format functions map engine types to output-specific types (JSON) or direct string formatting (summary, markdown)

No contradictions found between component pseudocode files. The OVERVIEW.md sequencing constraints (store first, engine second, integration last) are consistent with the compile-time dependencies (store and engine independent, server depends on both).

Minor note: E-06 test plan for the engine says "Input: topic='' -> Assert: result.topic == '(unattributed)'" but the engine's `classify_entry` does not perform NULL/empty-to-unattributed mapping -- that happens in the store SQL. At the engine unit test level, the caller would need to pass "(unattributed)" as the topic, not "". This is a test plan description issue, not an interface inconsistency. The actual interface is correct: store maps empty to "(unattributed)" in SQL, engine receives the already-mapped string. The test should either be moved to store integration tests or the input should be "(unattributed)".

## Rework Required

None. The WARN items are minor and do not require rework before proceeding to implementation:

1. **FR-04 calibration weighting**: The pseudocode correctly identifies the spec-vs-architecture discrepancy and follows the architecture. If weighted calibration is desired, this is a design decision to revisit, not a pseudocode bug.

2. **E-06 test description**: The test intent is correct (verify unattributed entries work) but the input/assert description doesn't match the engine's responsibility boundary. The implementer should write the engine unit test with `topic="(unattributed)"` as input (since the store does the mapping) and verify the store integration test covers the empty-to-unattributed SQL mapping separately. This is already implicit in store test S-13.
