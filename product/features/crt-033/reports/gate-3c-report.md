# Gate 3c Report: crt-033

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-29
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | All 13 risks mapped to passing tests in RISK-COVERAGE-REPORT.md |
| Test coverage completeness | PASS | All 39 risk scenarios from RISK-TEST-STRATEGY.md covered; integration smoke passed |
| Specification compliance | PASS | All 17 AC items verified; FR-01 through FR-15 and all NFRs addressed |
| Architecture compliance | PASS | All 7 schema cascade touchpoints updated; component boundaries match; ADRs followed |
| Knowledge stewardship compliance | PASS | Tester report has Queried and Stored entries |
| Integration smoke suite | PASS | 20/20 smoke tests pass |
| Integration tools suite | PASS | 94 pass, 2 pre-existing xfails (GH#405, GH#305) |
| Integration lifecycle suite | PASS | 40 pass, 2 pre-existing xfails (GH#291), 1 XPASS (GH#406 pre-existing) |
| No integration tests deleted/commented out | PASS | All test functions intact; new crt-033 tests added |
| XPASS on test_search_multihop_injects_terminal_active | PASS | Confirmed pre-existing GH#406; not caused by crt-033 |
| Cargo build (no compilation errors) | PASS | `cargo build --workspace` finishes with 0 errors, 14 warnings |
| Unit test suite | PASS | 4032 passed, 0 failed |
| No .unwrap() in production code | PASS | All .unwrap() calls in cycle_review_index.rs are inside #[cfg(test)] |
| Security — SQL parameterization | PASS | All queries use ?1/?2 bind parameters; no string interpolation |
| Xfail inventory complete with GH issues | WARN | tick-interval xfails reference GH#291 (valid); RISK-COVERAGE-REPORT labels them without GH number |

---

## Detailed Findings

### Risk Mitigation Proof

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md maps all 13 risks to named, passing tests:

- R-01 (schema cascade): MIG-U-01 through MIG-U-06 + 12 mandatory grep gates all pass. Cascade grep gate `grep -r 'schema_version.*== 17' crates/` returns zero matches — confirmed by independent validation.
- R-02 (pool starvation): CRS-I-10 concurrent test + TH-I-10 concurrent first-call test pass.
- R-03 (evidence_limit at storage): TH-I-07 verified — raw summary_json retains full evidence, render-time truncation confirmed.
- R-04 (force=true + purged signals fallthrough): TH-I-05 and TH-I-06 cover both sub-cases (stored record exists / absent).
- R-05 (memoization hit recomputes): TH-I-02 `computed_at` unchanged; TH-I-08 confirms force=true skips step 2.5.
- R-06 (serde deserialization failure): CRS-U-05 round-trip, CRS-U-06 missing-optional-fields, TH-U-06 corrupted-JSON-no-panic.
- R-07 (pending_cycle_reviews query): CRS-I-04 through CRS-I-09 cover K-window, DISTINCT, cycle_end-only exclusion, boundary inclusion, and multiple cycle_start deduplication.
- R-08 (version advisory): TH-U-03 (matching version, no advisory), TH-U-04 (mismatch, advisory present), TH-U-05 (future version, advisory present).
- R-09 (spawn_blocking violation): TH-G-01 static grep — no spawn_blocking in memoization functions; confirmed independently.
- R-10 (concurrent INSERT OR REPLACE): CRS-I-10 — both writers succeed, exactly one row exists.
- R-11 (4MB ceiling panic): CRS-U-03 (ceiling+1 returns Err, not panic), CRS-U-04 (exactly at ceiling returns Ok).
- R-12 (wrong pool for reads): CRS-G-02 static grep confirms `get_cycle_review` and `pending_cycle_reviews` use `read_pool()`, only `store_cycle_review` uses `write_pool_server()`.
- R-13 (SUMMARY_SCHEMA_VERSION location): CRS-G-01 grep — single `pub const` definition in `cycle_review_index.rs`; no numeric literal in `unimatrix-server`.

All failure mode scenarios from RISK-TEST-STRATEGY.md are covered (store write failure, get read failure, pending_cycle_reviews failure, deserialization failure, 4MB ceiling).

### Test Coverage Completeness

**Status**: PASS

**Evidence**:

Unit tests: 4032 passed, 0 failed (verified by independent `cargo test --workspace` run; matches RISK-COVERAGE-REPORT.md count).

New crt-033 unit test groups:
- `cycle_review_index.rs`: CRS-U-01 through CRS-U-06 (store/retrieve, 4MB ceiling, SUMMARY_SCHEMA_VERSION constant) + CRS-I-01 through CRS-I-10 (integration tests for all store methods)
- `migration_v17_to_v18.rs`: MIG-U-01 through MIG-U-06 (schema version, fresh DB, migration table creation, 5 columns, data preservation, idempotency)
- `sqlite_parity.rs`: `test_create_tables_cycle_review_index_exists` + `test_create_tables_cycle_review_index_schema` (5-column assertion)
- `tools.rs`: TH-U-01 through TH-U-07 (helper unit tests) + TH-I-01 through TH-I-10 (handler integration tests)
- `response/status.rs`: SR-U-01 through SR-U-08 + SR-I-01 (StatusReport struct, JSON/summary formatters, Default)
- `services/status.rs`: SS-U-01 + SS-I-01 through SS-I-03 (Phase 7b compute_report integration)

Integration tests (infra-001):
- smoke: 20/20 pass (mandatory gate)
- tools: 94 pass, 2 pre-existing xfails (GH#405, GH#305); new test `test_cycle_review_force_param_accepted` and `test_status_pending_cycle_reviews_field_present` both pass
- lifecycle: 40 pass, 2 pre-existing xfails (GH#291), 1 XPASS (GH#406); new test `test_cycle_review_persists_across_restart` passes

Risks requiring new tests beyond explicit ACs (per RISK-TEST-STRATEGY.md):
- R-06 scenario 3 (corrupted-JSON fallthrough): covered by TH-U-06
- R-07 scenarios 3–6 (exclusion correctness): covered by CRS-I-06, CRS-I-07, CRS-I-08, CRS-I-09
- R-08 scenario 3 (future schema_version): covered by TH-U-05
- R-09 scenario 1 (spawn_blocking static check): covered by TH-G-01
- R-11 scenarios 1–3 (4MB ceiling): covered by CRS-U-03, CRS-U-04
- R-12 scenario 1 (pool selection static check): covered by CRS-G-02

All 39 required scenarios accounted for.

### Specification Compliance

**Status**: PASS

**Evidence**: All 17 AC items verified in RISK-COVERAGE-REPORT.md with named passing tests. Spot checks:

- **FR-01 / AC-03 / AC-04**: First call writes row (TH-I-01); second call returns stored record without recompute (TH-I-02 — `computed_at` unchanged).
- **FR-02 / AC-04b**: Version advisory produced when `schema_version` differs (TH-U-04, TH-I-03); no silent recompute.
- **FR-03 / AC-11**: Written row has `schema_version = SUMMARY_SCHEMA_VERSION = 1` (verified by AC-03 test).
- **FR-05 / AC-06 / AC-15**: force=true + purged signals + stored record → Ok with explanatory note (TH-I-05).
- **FR-06 / AC-07**: force=true + no stored record + no observations → ERROR_NO_OBSERVATION_DATA (TH-I-06).
- **FR-08 / AC-08**: Stored summary_json has full evidence (5 items); render-time truncation to 2 items confirmed (TH-I-07).
- **FR-09 / AC-09**: pending_cycle_reviews in StatusReport contains unreviewed K-window cycles (SS-I-01, integration test).
- **FR-12 / AC-17**: SUMMARY_SCHEMA_VERSION defined only in cycle_review_index.rs — grep gate confirms.
- **NFR-03**: 4MB ceiling enforced with Err return (CRS-U-03, CRS-U-04).
- **NFR-05 / C-11**: `PENDING_REVIEWS_K_WINDOW_SECS` is a named constant (`90 * 24 * 3600`) in `services/status.rs` line 60 — confirmed.
- **C-02**: `store_cycle_review` uses `write_pool_server()` (confirmed in cycle_review_index.rs line 125).
- **C-04**: No cross-crate coupling for SUMMARY_SCHEMA_VERSION — no numeric literal in unimatrix-server or unimatrix-observe.

The `CycleReviewRecord.raw_signals_available` is `i32` (not `bool`) to match sqlx's SQLite INTEGER→i32 binding — consistent with RISK-TEST-STRATEGY edge case note and documented in the struct comment.

### Architecture Compliance

**Status**: PASS

**Evidence**:

- **Schema cascade (7 touchpoints)**: All updated per AC-02b.
  1. `migration.rs`: `CURRENT_SCHEMA_VERSION = 18` (confirmed at line 19).
  2. `migration.rs`: `if current_version < 18` block at line 601 with `CREATE TABLE IF NOT EXISTS cycle_review_index`.
  3. `db.rs`: `create_tables_if_needed()` includes cycle_review_index DDL (2 matches confirmed); schema_version INSERT updated to 18.
  4. `sqlite_parity.rs`: `test_create_tables_cycle_review_index_exists` and `test_create_tables_cycle_review_index_schema` (5-column assertion) present.
  5. `server.rs`: Both version assertions updated to `18` (lines 2137 and 2162 confirmed).
  6. `migration_v16_to_v17.rs`: `test_current_schema_version_is_at_least_17` with `>= 17` predicate present.
  7. Migration test files: no column-count assertions referencing the old count found.

- **Component boundaries**: `cycle_review_index.rs` is a separate module from `db.rs`, `write.rs`, `read.rs`, `analytics.rs` — matches architecture decomposition.

- **ADR-001 compliance**: `store_cycle_review` uses `write_pool_server()` directly in the async context — no `spawn_blocking` wrapper. `get_cycle_review` and `pending_cycle_reviews` use `read_pool()`.

- **ADR-003 compliance**: Direct serde serialization of `RetrospectiveReport` — no DTO shim. `build_cycle_review_record` helper serializes with `serde_json::to_string`. Corrupted-JSON fallthrough implemented in `check_stored_review` (TH-U-06 confirms no panic on corrupted JSON).

- **ADR-004 compliance**: `pending_cycle_reviews` uses `cycle_events` with `event_type = 'cycle_start'` (confirmed in cycle_review_index.rs lines 160-168). SQL matches architecture spec exactly.

- **Handler control flow**: `check_stored_review` helper extracted (lines 2253+), `build_cycle_review_record` extracted (lines 2279+), `dispatch_review_with_advisory` extracted — tools.rs additions are minimal; production code path is minimal.

- **StatusReport extension**: `pending_cycle_reviews: Vec<String>` field present in `StatusReport` (line 137), `StatusReport::default()` initializes to `Vec::new()` (line 196), `StatusReportJson` has corresponding field (line 884), `From<&StatusReport>` maps the field (line 1663), summary formatter renders non-empty list (line 391), JSON formatter includes array (line 802).

### Knowledge Stewardship Compliance

**Status**: PASS

**Evidence**: `crt-033-agent-8-tester-report.md` contains:
```
## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — found entries on schema-cascade failures (#3539), spawn_blocking prohibition (#2266, #2249), and read_pool for status aggregates (#3619). All applied to verification criteria.
- Stored: nothing novel to store — the MCPResponse error attribute pattern (resp.error vs tool-level result.is_error) is an existing harness convention, not a new discovery.
```

The block is present, has Queried entries with evidence of briefing queries, and has Stored with explicit reason ("existing harness convention"). Satisfies stewardship requirements.

RISK-COVERAGE-REPORT.md also includes a Knowledge Stewardship section with Queried and Stored entries.

### Integration Smoke Validation

**Status**: PASS

**Evidence**: RISK-COVERAGE-REPORT.md: smoke suite — 20 passed, 0 failed, 0 xfailed. The smoke suite is the mandatory integration gate and passes.

### Xfail Inventory Validation

**Status**: WARN (minor)

**Evidence**:
All xfail markers have corresponding GH issues:
- `test_confidence_deprecated_score_in_range`: GH#405
- `test_retrospective_baseline_present`: GH#305
- `test_auto_quarantine_after_consecutive_bad_ticks`: GH#291 (confirmed in test file line 566)
- `test_dead_knowledge_entries_deprecated_by_tick`: GH#291 (confirmed in test file line 1500)

The RISK-COVERAGE-REPORT.md xfail table labels the last two as "(tick-interval env var)" and "(tick timing)" without their GH#291 numbers, making it harder to cross-reference. The GH issue numbers are present in the actual test decorators. Minor documentation inconsistency only — no functional gap.

**XPASS confirmation**: `test_search_multihop_injects_terminal_active` (GH#406) is XPASS. The xfail reason says "Pre-existing: GH#406 — find_terminal_active multi-hop traversal not implemented." The XPASS indicates this bug has been fixed upstream (the test now passes). Confirmed not caused by crt-033: the commit history shows GH#406 was addressed in `a334214 impl(graph): supersession DAG, graph_penalty, find_terminal_active (#crt-014)`, which predates crt-033. The xfail marker should be removed and GH#406 closed, but this is a post-merge cleanup task, not a gate blocker.

### No Integration Tests Deleted or Commented Out

**Status**: PASS

**Evidence**: test_tools.py and test_lifecycle.py were inspected. No test functions removed or commented. Three new crt-033 tests appended. All pre-existing xfail markers intact.

### Security Review

**Status**: PASS

**Evidence**:
- `feature_cycle` is bound via parameterized SQL (`?1`) in all three store methods — no SQL injection surface.
- `force: Option<bool>` is a boolean value — no injection surface.
- `summary_json` is written from server-computed `RetrospectiveReport` (not from caller-supplied content) — no untrusted data in stored JSON.
- No hardcoded secrets or credentials.
- 4MB ceiling check prevents oversized blobs from causing OOM.
- No `.unwrap()` in production code (all `.unwrap()` calls in cycle_review_index.rs are inside `#[cfg(test)]` module at line 185+).

### Cargo Build and Test

**Status**: PASS

**Evidence**:
- `cargo build --workspace`: finishes successfully, 0 errors, 14 warnings (pre-existing warnings, not new).
- `cargo test --workspace`: 4032 passed, 0 failed (confirmed by independent test run).
- `cargo audit`: not installed in this environment — pre-existing limitation, not introduced by crt-033.

---

## Rework Required

None.

---

## Knowledge Stewardship

- Stored: nothing novel to store — no new recurring gate failure pattern emerged. All patterns observed (schema cascade miss, spawn_blocking prohibition, read_pool for aggregates) are already captured in Unimatrix entries #3539, #2266, #2249, and #3619. The crt-033 test suite demonstrates clean execution of the full risk-to-scenario traceability discipline, but this is expected behavior, not a new discovery.
