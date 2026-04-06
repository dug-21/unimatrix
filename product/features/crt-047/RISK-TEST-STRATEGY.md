# Risk-Based Test Strategy: crt-047

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | ADR-003 (ENTRIES-only query) and SPECIFICATION (AUDIT_LOG join for FR-05/FR-06/AC-04/NFR-03) contradict each other; implementor receives conflicting SQL blueprints | High | High | Critical |
| R-02 | FR-10 spec says `ORDER BY feature_cycle DESC` but ADR-001 decision is `ORDER BY first_computed_at DESC` with a new `first_computed_at` column; spec never absorbed ADR-001 | High | High | Critical |
| R-03 | Schema cascade: seven new columns across three migration paths (migration.rs, db.rs, DDL); any missed path leaves migrated and fresh-schema databases diverged | High | Med | High |
| R-04 | `corrections_total` defined differently in FR-03/FR-04 vs ADR-002: spec says `corrections_agent + corrections_human + corrections_system = corrections_total`; ADR-002 says `corrections_total = corrections_agent + corrections_human` (system excluded) | High | Med | High |
| R-05 | `compute_curation_baseline` receiving all-zero DEFAULT rows (pre-v24 migrated cycles) as real zero-correction data produces a biased baseline; NFR-01 requires exclusion logic but AC-15 test cases may not enforce it | High | Med | High |
| R-06 | Division by zero in orphan ratio (`orphan_deprecations / deprecations_total`) when `deprecations_total = 0`; NaN propagation into σ baseline or output | High | Med | High |
| R-07 | `store_cycle_review()` upsert must preserve `first_computed_at` on overwrite; naive `INSERT OR REPLACE` deletes and reinserts the row, resetting `first_computed_at` to `now` — the entire fix from ADR-001 is negated | High | Med | High |
| R-08 | `OQ-SPEC-01` unresolved: AUDIT_LOG orphan query does not specify whether to filter `outcome = Success`; failed deprecation attempts silently inflate `orphan_deprecations` | Med | Med | Medium |
| R-09 | `OQ-SPEC-02` unresolved: `corrections_system` field disposition not definitively decided; implementor may omit column from `cycle_review_index` DDL or `CurationSnapshot` struct inconsistently | Med | Med | Medium |
| R-10 | Schema cascade test failures: existing migration tests assert exact `schema_version == N` and column counts; bumping to v24 cascades to multiple test files not listed in AC-14 (entry #3894 pattern) | Med | High | Medium |
| R-11 | Cold-start trend/σ boundary conditions: trend requires 6 cycles, σ requires 3 — off-by-one in boundary check produces premature or suppressed σ output | Med | Med | Medium |
| R-12 | `SUMMARY_SCHEMA_VERSION` bump blast radius: all historical cycles show advisory on `force=false`; operators unaware of the scope may assume the system is broken | Med | High | Medium |
| R-13 | `updated_at` used as orphan deprecation timestamp proxy; if any future path modifies a deprecated orphan entry after deprecation, attribution window shifts — silent mis-attribution | Low | Low | Low |
| R-14 | Unattributed orphan deprecations (outside cycle windows) silently excluded; operators may interpret missing count as absence of orphans rather than absence of attribution | Low | High | Low |

---

## Risk-to-Scenario Mapping

### R-01: ADR-003 vs. SPECIFICATION Contradiction on Orphan Attribution Query

**Severity**: High
**Likelihood**: High
**Impact**: Implementor receives two incompatible SQL blueprints. ADR-003 final resolution says ENTRIES-only (`updated_at` window, no AUDIT_LOG join). SPECIFICATION FR-05, FR-06, AC-04, NFR-03, Domain Models, and Workflow 3 all specify an AUDIT_LOG join (`operation = 'context_deprecate'`, `timestamp BETWEEN cycle_start_ts AND review_call_ts`). Whichever path an implementor chooses, it will fail gate review against the other artifact. If the AUDIT_LOG path is chosen, `deprecations_total` is also AUDIT_LOG-sourced (FR-06), diverging from ADR-003's ENTRIES approach for `deprecations_total` as well.

**Test Scenarios**:
1. Implement and call the orphan count query; verify it produces the same result whether AUDIT_LOG join or ENTRIES-only path is used against a known test fixture. This is the reconciliation test — it should pass on either approach given the write-path invariant (all orphans go through `context_deprecate`).
2. Seed a `context_correct` chain-deprecation and verify it does NOT appear in `orphan_deprecations` regardless of which query path is used.
3. Seed a `context_deprecate` call; verify the orphan appears in the cycle window count.

**Coverage Requirement**: The implementation brief must state the chosen approach definitively before implementation begins. A reconciliation comment in the query explaining why the two approaches are equivalent must be present in code review.

---

### R-02: Ordering Key Mismatch — Spec vs. ADR-001

**Severity**: High
**Likelihood**: High
**Impact**: FR-10 says `ORDER BY feature_cycle DESC LIMIT N`; ADR-001 resolves to add `first_computed_at` column and `ORDER BY first_computed_at DESC WHERE first_computed_at > 0`. If the implementor follows the spec, `force=true` on historical cycles will perturb the baseline window — the problem ADR-001 exists to solve. If the implementor follows ADR-001, the migration has seven columns (not five per FR-08, not six per the ARCHITECTURE.md summary), and the SPECIFICATION's DDL description is wrong.

**Test Scenarios**:
1. Call `context_cycle_review force=true` on a historical cycle; then call `context_cycle_review` for the current cycle; verify the baseline window contains the expected N most-recent-by-insertion cycles, not the force-recomputed historical one.
2. Verify `cycle_review_index` has `first_computed_at` column after v24 migration.
3. Verify `get_curation_baseline_window()` excludes rows where `first_computed_at = 0` (legacy migrated rows).

**Coverage Requirement**: The implementation brief must pick ADR-001 over FR-10 and document the column count discrepancy (5 in FR-08 vs 7 in ADR-001+ADR-002). The ordering key must be covered by a specific integration test.

---

### R-03: Schema Migration — Three-Path Update and Column Count

**Severity**: High
**Likelihood**: Med
**Impact**: Missing any of the three paths (migration.rs, db.rs DDL, `CURRENT_SCHEMA_VERSION` constant) leaves fresh-schema and migrated databases diverged. Entry #4092 (multi-column migration pattern) warns that all `pragma_table_info` checks must run before any `ALTER TABLE` executes. Column count disagreement between ADR-001/ADR-002 (seven columns) and SCOPE/SPEC (five columns) creates a mismatch risk in DDL synchronization checks.

**Test Scenarios**:
1. Open a synthetic v23 database via `Store::open()` (not the migration function in isolation); assert all new columns present with DEFAULT 0 on pre-existing rows; assert `CURRENT_SCHEMA_VERSION = 24` (AC-14).
2. Open a fresh (no prior rows) database; assert `pragma_table_info('cycle_review_index')` includes all new columns; assert column count matches `db.rs` DDL.
3. Simulate mid-migration crash: manually add three of seven columns, then re-run `Store::open()`; assert idempotent completion.

**Coverage Requirement**: AC-14 must use `Store::open()` not the migration function in isolation. All new columns must be enumerated in the test assertion.

---

### R-04: `corrections_total` Accounting Contradiction

**Severity**: High
**Likelihood**: Med
**Impact**: FR-03/FR-04 states `corrections_agent + corrections_human + corrections_system = corrections_total`; ADR-002 states `corrections_total = corrections_agent + corrections_human` (system excluded, total = intentional-only). These definitions produce different numbers when `corrections_system > 0`. Operators relying on the ratio `corrections_agent / corrections_total` will get different values depending on which definition was implemented. The σ baseline is also affected.

**Test Scenarios**:
1. Seed entries with `trust_source IN ('agent', 'human', 'system', 'direct')`; verify `corrections_total` matches the chosen definition.
2. Verify `corrections_total` is not stored as a column (ADR-002 says it is computed, not stored) — round-trip test on `cycle_review_index` should not show a `corrections_total` column if ADR-002 is followed.
3. Seed a fixture where system corrections are non-zero; verify the σ baseline is not polluted by system-source noise.

**Coverage Requirement**: Implementation brief must resolve FR-04 vs ADR-002 before pseudocode. AC-03 test must assert exact values for all four buckets using the settled definition.

---

### R-05: Zero-DEFAULT Legacy Rows Biasing Baseline

**Severity**: High
**Likelihood**: Med
**Impact**: Pre-v24 rows migrated from v23 have DEFAULT 0 for all snapshot columns, indistinguishable from a genuine zero-correction cycle. Including them in baseline computation anchors the mean at zero and reduces stddev, making every real cycle appear as a high σ outlier. NFR-01 specifies an exclusion rule (rows with all-five-columns-zero AND schema_version < 2 treated as missing), but this exclusion logic is complex and easy to misimplement.

**Test Scenarios**:
1. AC-15(f): rows with all-zero snapshot data from `schema_version < 2` are excluded from the `n` count toward `MIN_HISTORY = 3`; direct unit test on the pure function.
2. Seed a window with 5 legacy (all-zero) rows and 3 real rows; verify baseline uses only 3 rows and `history_cycles = 3` in output annotation.
3. Seed a window where a cycle legitimately had zero corrections (real zero); verify it IS included in the baseline (the exclusion applies only to schema_version < 2 rows, not genuine zero cycles).

**Coverage Requirement**: The distinction between a real zero-correction cycle and a DEFAULT-0 legacy row must be encoded in `CurationBaselineRow` (e.g., a `has_real_data: bool` flag or using the stored `schema_version`). AC-15(f) must be present.

---

### R-06: Division by Zero in Orphan Ratio

**Severity**: High
**Likelihood**: Med
**Impact**: `orphan_ratio = orphan_deprecations / deprecations_total`; when `deprecations_total = 0` this is a divide-by-zero. NaN propagates silently through `f64` arithmetic in Rust unless explicitly handled, and will poison the mean/stddev computation, producing NaN in output.

**Test Scenarios**:
1. AC-15(e): direct unit test — `deprecations_total = 0` produces `orphan_ratio = 0.0`.
2. Seed a baseline window where all rows have `deprecations_total = 0`; verify `CurationBaseline.orphan_ratio_stddev` is `0.0`, not NaN.
3. Mix zero and non-zero `deprecations_total` rows in the window; verify mean/stddev are finite numbers.

**Coverage Requirement**: `compute_curation_baseline` must have explicit zero-denominator guard. NaN check: `assert!(!result.orphan_ratio_mean.is_nan())` in unit tests.

---

### R-07: `store_cycle_review()` Upsert Clobbering `first_computed_at`

**Severity**: High
**Likelihood**: Med
**Impact**: ADR-001 requires `first_computed_at` to be set on first insert and preserved on subsequent `INSERT OR REPLACE` (force=true overwrites). Naive `INSERT OR REPLACE` in SQLite deletes the old row and inserts a new one, resetting `first_computed_at` to `now`. This nullifies the entire ADR-001 fix. The architecture specifies a two-step pattern (read-then-preserve or `INSERT OR IGNORE` + `UPDATE`) but this is complex to implement correctly.

**Test Scenarios**:
1. Insert a `cycle_review_index` row; record `first_computed_at`; call `store_cycle_review` again for the same cycle (force=true path); assert `first_computed_at` is unchanged.
2. Insert a new cycle row (first write); assert `first_computed_at` equals the cycle's `cycle_events` start timestamp (not `now`).
3. Baseline window test: after force-recomputing a historical cycle, assert it does not appear as the most-recent entry in `get_curation_baseline_window()`.

**Coverage Requirement**: Round-trip test for `first_computed_at` preservation is mandatory. Code review must verify the upsert implementation does not use plain `INSERT OR REPLACE`.

---

### R-08: AUDIT_LOG Outcome Filter Not Specified (OQ-SPEC-01)

**Severity**: Med
**Likelihood**: Med
**Impact**: If AUDIT_LOG rows from failed `context_deprecate` calls (where the tool returned an error) are counted as orphan deprecations, the metric is incorrect. The SPECIFICATION calls this out in the "NOT In Scope" section as in-scope-but-ADR-gated, yet leaves OQ-SPEC-01 open.

**Test Scenarios**:
1. If AUDIT_LOG join approach is chosen: seed an audit row with `operation = 'context_deprecate'` and `outcome != 'Success'`; verify it does NOT appear in `orphan_deprecations`.
2. If ENTRIES-only approach is chosen (ADR-003): the AUDIT_LOG outcome filter is irrelevant; verify no AUDIT_LOG query is issued.

**Coverage Requirement**: If the AUDIT_LOG join is used, an `outcome = 'Success'` filter must be present and tested. If the ENTRIES-only path is used, this risk is vacuous — document the resolution.

---

### R-09: `corrections_system` Field Disposition Unresolved (OQ-SPEC-02)

**Severity**: Med
**Likelihood**: Med
**Impact**: If `corrections_system` is included in `CurationSnapshot` but omitted from `cycle_review_index` DDL (or vice versa), a round-trip store/retrieve will silently lose the field. ADR-002 decided to include it as a stored column; SPECIFICATION FR-08 lists only five columns and does not include `corrections_system`.

**Test Scenarios**:
1. Round-trip test: store a `CurationSnapshot` with non-zero `corrections_system`; retrieve from `cycle_review_index`; assert value survives.
2. If `corrections_system` is omitted from storage: verify the struct field is always computed at query time and never stored (no stale-read risk).

**Coverage Requirement**: AC-03 must enumerate the `corrections_system` bucket behavior. The DDL in `db.rs` and `migration.rs` must agree on whether the column exists.

---

### R-10: Schema Cascade Test Failures (Migration Test Files)

**Severity**: Med
**Likelihood**: High
**Impact**: Entry #3894 documents that bumping `CURRENT_SCHEMA_VERSION` cascades to multiple test files that assert exact `schema_version == N` and column counts: `sqlite_parity.rs`, `server.rs` (multiple assertion sites), and all `migration_vX_to_vY.rs` files. Missing any cascade touchpoint produces test failures at gate. The previous migration test (`migration_v23_to_v24.rs` does not exist yet, but the prior test file for v22→v23 will need its exact-version assertion changed to `>= 23`).

**Test Scenarios**:
1. Run `cargo test --workspace` after bumping `CURRENT_SCHEMA_VERSION = 24`; all pre-existing migration tests pass.
2. `sqlite_parity.rs::test_schema_version_is_N` asserts 24.
3. `sqlite_parity.rs::test_schema_column_count` is updated for the new column count.
4. `server.rs` schema version assertion sites updated to 24.
5. The v22→v23 migration test renames its exact-version assertion to `>= 23`.

**Coverage Requirement**: Pre-delivery cascade grep check: `grep -r 'schema_version.*== 23' crates/` must return zero matches after bumping. This should be a pre-merge gate check.

---

### R-11: Cold-Start Threshold Boundary Conditions

**Severity**: Med
**Likelihood**: Med
**Impact**: σ comparison activates at 3 prior cycles; trend activates at 6. Off-by-one in either boundary causes: σ output on 2 cycles (premature, unreliable), or suppressed σ on 3 cycles (regression). The two thresholds are independent, adding a third boundary at 10 (full window).

**Test Scenarios**:
1. AC-08: seed 2 prior rows with snapshot data; verify no σ field in response.
2. AC-07: seed exactly 3 prior rows; verify σ is present and annotated `"(3 cycles of history)"`.
3. AC-10 (5-cycle case): seed 5 rows; verify trend absent, σ present.
4. AC-10 (7-cycle case): seed 7 rows; verify both σ and trend present.
5. Seed exactly 6 rows; verify trend is present (boundary is inclusive at 6).
6. Seed 10 rows; verify window is capped at 10, older rows excluded.

**Coverage Requirement**: Explicit test for each boundary: 2, 3, 5, 6, 7, 10. Boundary tests are unit-level (pure function).

---

### R-12: SUMMARY_SCHEMA_VERSION Advisory Blast Radius

**Severity**: Med
**Likelihood**: High
**Impact**: Every historical `cycle_review_index` row has `schema_version = 1`. After v24 deploys, every `context_cycle_review force=false` call on any historical cycle returns the advisory. Operators may interpret this as a system error rather than expected behavior. AC-11 tests the advisory path but does not verify the absence of silent recomputation.

**Test Scenarios**:
1. AC-11: store a row with `schema_version = 1`; call `context_cycle_review force=false`; assert advisory string present.
2. AC-12: same setup; assert snapshot columns in `cycle_review_index` remain unchanged after the call (no silent recompute).
3. AC-12: call `context_cycle_review force=true` after the advisory; assert row is now `schema_version = 2` and snapshot columns have real values.

**Coverage Requirement**: Negative assertion in AC-12 (no side-effect on force=false) is mandatory. Advisory string must be tested for exact format match.

---

## Integration Risks

**I-01: `context_cycle_review` step ordering.** `compute_curation_snapshot()` must execute before `store_cycle_review()` (read before write). If the snapshot SQL reads from the same `cycle_review_index` row being written, and the write happens first due to refactor, the snapshot will read stale data on the next call instead of computing fresh. The interaction diagram in ARCHITECTURE.md shows the correct order; the implementation must maintain it.

**I-02: Pool discipline across compute and store.** `compute_curation_snapshot()` uses `read_pool()`; `store_cycle_review()` uses `write_pool_server()`. Mixing pools or passing the wrong pool connection through the call chain produces either a deadlock (on `write_pool_server` contention) or a dirty read. Existing `write_pool_server` is a single-connection serializer — any `read_pool()` call inside the write's async context is safe but must not hold the write connection simultaneously.

**I-03: Cycle window derivation dependency.** `compute_curation_snapshot()` requires `cycle_start_ts` from `cycle_events`. The `context_cycle_review` handler already queries `cycle_events` for `get_cycle_start_goal`. If this query is refactored, the `cycle_start_ts` derivation must not break. A cycle with no `cycle_start` event (malformed cycle) must not panic — fallback to `0` per ADR-003.

**I-04: `CycleReviewRecord` field addition.** Adding `first_computed_at`, the snapshot columns, and `corrections_system` to `CycleReviewRecord` changes the `INSERT OR REPLACE` SQL. Any other code path that calls `store_cycle_review()` with the old struct signature will fail to compile. Confirm there is only one call site.

---

## Edge Cases

**EC-01: Empty cycle.** `context_cycle_review` called for a cycle with no entries written (`corrections_total = 0`, `deprecations_total = 0`). Snapshot should be all zeros. No divide-by-zero. σ comparison should treat zero as a valid data point.

**EC-02: Cycle with no `cycle_start` event in `cycle_events`.** `cycle_start_ts` is undefined. Fallback to `0` means the cycle window is `[0, review_ts]` — the entire history is in-window. This over-counts historical orphans. The risk is acceptable if documented; the tester must verify the fallback does not panic.

**EC-03: Very large `target_ids` LIKE pattern (if AUDIT_LOG join is used).** Entry IDs as large integers embedded in a JSON array; the `LIKE '%' || e.id || '%'` match could false-positive if an ID is a substring of a larger ID (e.g., ID `12` matching inside `[123, 456]`). ADR-003 notes this and recommends `json_each()` for robustness. Test with IDs where one is a prefix of another.

**EC-04: Concurrent `force=true` calls for the same cycle.** Two concurrent `force=true` calls race on `INSERT OR REPLACE`. The `write_pool_server` serializes them, but the second call will overwrite the first's `first_computed_at` if the upsert is not implemented as read-preserve. Outcome must be deterministic (last write wins for snapshot data, `first_computed_at` always preserved).

**EC-05: `feature_cycle` ordering across phase prefixes.** ADR-001 rejects `feature_cycle DESC` as the ordering key precisely because alphabetical phase prefixes (`alc`, `col`, `crt`, `nxs`, `vnc`) do not sort temporally. However, FR-10 in the spec still uses `feature_cycle DESC`. This edge case manifests when cycles from different phases are in the window — the window will be incorrectly ordered. The `first_computed_at` resolution in ADR-001 must prevail.

**EC-06: Window contains only legacy rows (`first_computed_at = 0`).** `get_curation_baseline_window()` with `WHERE first_computed_at > 0` returns zero rows. `compute_curation_baseline()` receives an empty slice and returns `None`. `context_status` curation health block shows raw numbers only. Must not error.

---

## Security Risks

**SEC-01: SQL injection via `feature_cycle` parameter.** `compute_curation_snapshot()` takes `feature_cycle: &str` and uses it in `WHERE feature_cycle = ?`. NFR-05 requires parameterized binds. Risk is low if sqlx parameterized queries are used throughout. Test: verify no string interpolation in the SQL.

**SEC-02: LIKE pattern with user-derived ID (if AUDIT_LOG join is used).** `al.target_ids LIKE '%' || e.id || '%'` embeds a database-internal integer (not user input), so SQL injection is not a direct concern. However, the pattern is fragile for ID substrings (EC-03). This is a correctness risk with security-adjacent implications if incorrect orphan counts are exploited to obscure malicious deprecations.

**SEC-03: `corrections_system` visibility.** Surfacing `corrections_system` in `context_status` or `context_cycle_review` output exposes that system/cortical-implant writes are occurring at a given rate. This is informational — no secrets are exposed — but operators should be aware the field reveals internal write volume. Not a blocking security risk.

---

## Failure Modes

**FM-01: Migration fails mid-run.** Outer transaction atomicity (ADR-004) ensures the schema stays at v23. On retry, `pragma_table_info` pre-checks skip already-added columns. Recovery: re-deploy and `Store::open()` will complete the migration.

**FM-02: `compute_curation_snapshot()` SQL fails.** Should return `ServiceError`, which must not abort the entire `context_cycle_review` response. The curation health block should be absent or contain an error annotation rather than causing a tool-call failure.

**FM-03: `get_curation_baseline_window()` returns zero rows.** `compute_curation_baseline()` returns `None`; `context_cycle_review` emits raw counts only; `context_status` curation block shows no σ or trend. No error returned to caller.

**FM-04: NaN in σ output.** Zero stddev in the baseline (all cycles identical) should produce `NoVariance` status analogous to `unimatrix_observe::baseline::BaselineStatus`. The σ value should be `None` or `0.0`, not NaN. Downstream formatters must guard against NaN display.

**FM-05: Missing `cycle_start` event.** Fallback to `cycle_start_ts = 0` over-counts orphans (EC-02). A warning should be logged. The snapshot is still stored — it is marked as potentially over-counted via the history-length annotation if the `first_computed_at` is also unreliable.

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: AUDIT_LOG operation string inconsistency | R-01 | ADR-003 source code analysis confirms `"context_deprecate"` is consistent; additionally resolves that ENTRIES-only query suffices. However, SPECIFICATION contradicts ADR-003 — conflict is open at implementation handoff. |
| SR-02: Schema version conflict with parallel in-flight feature | R-03 | Addressed: ADR-004 documents the pre-delivery check (`grep CURRENT_SCHEMA_VERSION`). SM must execute before pseudocode phase. |
| SR-03: Three migration paths must all be updated | R-03 | Addressed: ADR-004 explicitly covers migration.rs + db.rs; confirms the legacy static DDL array (`migration_compat.rs`) is not relevant for v24. AC-14 requires `Store::open()` integration test. |
| SR-04: SUMMARY_SCHEMA_VERSION blast radius | R-12 | Addressed: SPECIFICATION documents the blast radius and the batch `force=true` recommendation. Behavior is intentional per crt-033 ADR-002. |
| SR-05: `force=true` dual semantics | — | Resolved: SPECIFICATION Constraints § force=true semantics defines all three cases explicitly. No residual risk. |
| SR-06: `services/status.rs` 500-line cap | — | Resolved: ADR-005 pre-plans extraction to `services/curation_health.rs`. No residual risk. |
| SR-07: `computed_at` ordering non-determinism | R-02 | Addressed by ADR-001 with `first_computed_at` column. Residual risk: SPECIFICATION FR-10 was not updated to reflect ADR-001 — ordering key mismatch remains at implementation handoff. |
| SR-08: Out-of-cycle deprecations silently excluded | R-14 | Documented: ARCHITECTURE.md and SPECIFICATION both explicitly acknowledge the exclusion. Accepted as known gap; no separate unattributed count in this feature. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | Implementation brief must resolve both before pseudocode; reconciliation tests required |
| High | 5 (R-03–R-07) | 5+ integration tests + 6+ unit tests (AC-14, AC-15, round-trip upsert) |
| Medium | 5 (R-08–R-12) | 10+ unit tests covering boundary conditions, cascade test updates, advisory path |
| Low | 2 (R-13, R-14) | Documentation + 1 unit test (out-of-window timestamp exclusion, AC-18) |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found entries #4076 (gate-3b test omission), #4177 (tautological assertion), #4147 (testability AC gap). Gate-3b test omission pattern informed R-03 and R-10 emphasis on mandatory integration test delivery.
- Queried: `/uni-knowledge-search` for "risk pattern" (category: pattern) — found entries #3426 (formatter regression), #4041 (write_graph_edge return contract). No direct applicability to crt-047.
- Queried: `/uni-knowledge-search` for "SQLite migration schema version" — found entries #4092 (multi-column pragma_table_info pattern) and #3894 (schema cascade checklist). Both informed R-03 and R-10.
- Queried: `/uni-knowledge-search` for "AUDIT_LOG join orphan attribution" — found entry #4181 (ADR-003 for crt-047 itself, confirming ENTRIES-only resolution was stored). Entry #3894 is deprecated but its cascade test content remains applicable.
- Stored: nothing novel — the ADR-003 vs. SPECIFICATION conflict (R-01) is feature-specific, not a cross-feature pattern. The `first_computed_at` upsert preservation risk (R-07) is specific to this `INSERT OR REPLACE` design. Will re-evaluate after delivery for pattern extraction.
