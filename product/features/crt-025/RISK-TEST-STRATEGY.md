# Risk-Based Test Strategy: crt-025 — WA-1: Phase Signal + FEATURE_ENTRIES Tagging

GH #330 | Schema v14 → v15

---

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | `current_phase` mutation timing: `context_store` reads stale `None` if `SessionState` mutation is delayed behind any async dispatch | High | Med | Critical |
| R-02 | Phase snapshot skew: analytics drain `FeatureEntry` event uses live `SessionState` at flush time, not snapshot at enqueue | High | Med | Critical |
| R-03 | `outcome` category removal breaks existing tests and silently rejects callers still using that category | Med | High | High |
| R-04 | Cross-cycle comparison SQL query returns wrong baseline when 0 or 1 prior features have phase data — threshold guard missing or wrong | Med | Med | High |
| R-05 | Schema migration v14→v15 non-idempotent: second run fails on `ALTER TABLE ADD COLUMN` without `pragma_table_info` pre-check | Med | Low | High |
| R-06 | Phase string normalization inconsistency: mixed-case or underscore variants (`Scope`, `gate_review`) stored without lowercasing, fragmenting GNN labels | Med | High | High |
| R-07 | `CYCLE_EVENTS` seq duplication under concurrent cross-session writes produces incorrect phase sequence reconstruction | Low | Low | Medium |
| R-08 | `context_cycle_review` phase narrative emitted for pre-WA-1 features that have no `CYCLE_EVENTS` rows, breaking backward compatibility | Med | Low | High |
| R-09 | Hook path hard-fails on `phase-end` validation error instead of logging warning and falling through | Med | Low | Medium |
| R-10 | `create_tables_if_needed` (fresh DB path) not updated: `CYCLE_EVENTS` table or `feature_entries.phase` column absent on new installs | Med | Med | High |
| R-11 | `AnalyticsWrite::FeatureEntry` internal match arms not updated for new `phase` field — compilation error or silent field default | Med | Med | High |
| R-12 | `context_cycle_review` cross-cycle SQL query includes current feature in cross-cycle mean, inflating baseline | Med | Low | Medium |
| R-13 | `phase-end` event with no prior `start` for the same `cycle_id` causes panic or query error in phase narrative construction | Low | Low | Medium |
| R-14 | `record_feature_entries` call sites in `server.rs`, `services/usage.rs`, and tests not updated for new `phase` parameter — compilation breakage or wrong data | Med | Med | High |

---

## Risk-to-Scenario Mapping

### R-01: `current_phase` Mutation Timing
**Severity**: High
**Likelihood**: Med
**Impact**: Entries stored immediately after a `phase-end` event receive `phase = NULL` instead of the new phase. W3-1 training data silently loses phase labels for the first entries in each phase.

**Test Scenarios**:
1. In a single session: emit `context_cycle(type="phase-end", next_phase="design")`, then immediately call `context_store`. Assert `feature_entries.phase = "design"` — not NULL.
2. Emit `context_cycle(type="start", next_phase="scope")`, then `context_store` with no intervening async yield. Assert `feature_entries.phase = "scope"`.
3. Emit `context_cycle(type="stop")`, then `context_store`. Assert `feature_entries.phase IS NULL`.

**Coverage Requirement**: All three event types (`start`, `phase-end`, `stop`) verified to mutate `current_phase` before any `context_store` can observe the session state. No async delay between mutation and readable state. Mitigates SR-01 / ADR-001.

---

### R-02: Phase Snapshot Skew (Analytics Drain Path)
**Severity**: High
**Likelihood**: Med
**Impact**: An entry enqueued during `implementation` phase, drained after a `phase-end` transition to `testing`, receives `phase = "testing"` instead of `"implementation"`. Systematic skew for all high-frequency stores.

**Test Scenarios**:
1. Enqueue `AnalyticsWrite::FeatureEntry` with `phase = Some("implementation")` while session phase is still `implementation`. Advance `SessionState.current_phase` to `"testing"` before the drain fires. Assert the persisted `feature_entries.phase = "implementation"` (enqueue-time snapshot, not drain-time value).
2. Verify `AnalyticsWrite::FeatureEntry` struct contains `phase: Option<String>` as a field (compile-time, but also assert the field is populated at the enqueue call site in `server.rs` or `services/usage.rs`).
3. `UsageContext` carries `current_phase` — assert it is read from session state before any async dispatch and passed through to both write paths.

**Coverage Requirement**: Analytics drain path proven to use enqueue-time phase value. Direct write path also tested. Mitigates SR-07 / ADR-001.

---

### R-03: `outcome` Category Removal
**Severity**: Med
**Likelihood**: High
**Impact**: All test assertions `al.validate("outcome").is_ok()` fail compilation or flip to unexpected error. Any external caller using category `outcome` receives `InvalidCategory` with no warning.

**Test Scenarios**:
1. `CategoryAllowlist::new()` contains exactly 7 categories; `"outcome"` is not in the list.
2. `al.validate("outcome")` returns `Err(...)`.
3. `context_store` MCP call with `category: "outcome"` returns `ServerError::InvalidCategory`.
4. `context_store` with each of the remaining 7 valid categories succeeds (regression — removal did not accidentally cull another category).
5. Existing database entries with `category = "outcome"` remain queryable via `context_search` (no deletion).

**Coverage Requirement**: All test functions named `test_validate_outcome`, `test_new_allows_outcome_and_decision`, `test_poison_recovery_validate`, `test_list_categories_sorted` (per ADR-005) updated and passing. Mitigates SR-03.

---

### R-04: Cross-Cycle Comparison Threshold Guard
**Severity**: Med
**Likelihood**: Med
**Impact**: Cross-cycle comparison appears in `phase_narrative` when only 0 or 1 prior features have phase data — mean is computed over a single sample, producing misleading baselines for W3-1.

**Test Scenarios**:
1. Zero prior features with phase-tagged `feature_entries`: `cross_cycle_comparison` field is `None` in the returned `PhaseNarrative`.
2. Exactly one prior feature with phase data: `cross_cycle_comparison` is `None` (below the 2-feature threshold, per FR-10.2).
3. Two prior features with phase data: `cross_cycle_comparison` is `Some(...)` with a valid mean.
4. Cross-cycle query correctly excludes the current feature being reviewed (FR-10.3): seed `feature_id = "crt-025"` entries and two prior features; assert the mean does not include crt-025's own rows.

**Coverage Requirement**: Threshold boundary tested at 0, 1, and 2 prior features. Self-exclusion from cross-cycle baseline verified.

---

### R-05: Schema Migration Idempotency
**Severity**: Med
**Likelihood**: Low
**Impact**: CI runs migration twice (common in test setups) — second run panics on `ALTER TABLE ADD COLUMN` without the `pragma_table_info` guard. Breaks all migration integration tests.

**Test Scenarios**:
1. Run `run_main_migrations` on a v14 database. Assert schema is v15, `cycle_events` table exists, `feature_entries.phase` column exists.
2. Run `run_main_migrations` again on the already-v15 database. Assert no error and schema unchanged.
3. Run migration on a fresh database (via `create_tables_if_needed`). Assert same outcome as scenario 1.
4. Assert `CURRENT_SCHEMA_VERSION = 15` is readable from the `counters` table after migration.

**Coverage Requirement**: v14→v15 migration integration test added following the pattern of existing `v13→v14` test. Both the direct migration path and fresh-DB path covered. References pattern #1264 (pragma_table_info idempotent ALTER TABLE) and #836 (new-table migration procedure).

---

### R-06: Phase String Normalization
**Severity**: Med
**Likelihood**: High
**Impact**: `"Scope"`, `"SCOPE"`, `"scope"` stored as three distinct labels in `CYCLE_EVENTS` and `feature_entries.phase`. GNN training receives fragmented class labels; rework detection misses repeated phases.

**Test Scenarios**:
1. `validate_cycle_params` with `phase = "Scope"` returns `ValidatedCycleParams` where `phase = Some("scope")`.
2. `validate_cycle_params` with `phase = "IMPLEMENTATION"` normalizes to `"implementation"`.
3. `validate_cycle_params` with `phase = "gate_review"` (underscore, no space) passes format check — note: the engine does not enforce the canonical vocabulary; this tests that underscore is accepted (it does not contain a space).
4. `validate_cycle_params` with `phase = "scope review"` (contains space) is rejected with descriptive error.
5. `validate_cycle_params` with `phase = ""` (empty string) is rejected (FR-02.4).
6. `validate_cycle_params` with `phase = "a".repeat(65)` is rejected (FR-02.3).
7. `next_phase` receives identical normalization: `"Design"` → `"design"`.

**Coverage Requirement**: Normalization verified at the validation layer for both `phase` and `next_phase`. Space rejection, length rejection, and empty rejection all covered. Mitigates SR-06.

---

### R-07: `seq` Duplication Under Concurrent Sessions
**Severity**: Low
**Likelihood**: Low
**Impact**: Two sessions sharing a `cycle_id` emit simultaneous `phase-end` events, producing duplicate `seq` values. Phase narrative order is resolved by `(timestamp, seq)` — incorrect if timestamp resolution is coarser than 1 second.

**Test Scenarios**:
1. Single session: three sequential `context_cycle` calls for the same `cycle_id` produce rows with `seq = 0, 1, 2` in `CYCLE_EVENTS`.
2. Two concurrent inserts for the same `cycle_id` (simulated in test): resulting rows have distinct `id` values (AUTOINCREMENT) and are orderable by `(timestamp, seq)` without crashing. No assertion that seq is unique — advisory behavior accepted per ADR-002.
3. Phase narrative query uses `ORDER BY timestamp ASC, seq ASC` (not `seq ASC` alone) — verify query text or assert correct ordering when timestamp differs.

**Coverage Requirement**: Sequential seq generation tested. Advisory nature under concurrent writes documented in test comment. Ordering by `(timestamp, seq)` verified. Mitigates SR-02.

---

### R-08: Phase Narrative Backward Compatibility
**Severity**: Med
**Likelihood**: Low
**Impact**: Pre-WA-1 features queried via `context_cycle_review` include a `phase_narrative` field (even empty) in the JSON response, breaking callers that use strict schema validation.

**Test Scenarios**:
1. Call `context_cycle_review` for a feature cycle with zero rows in `CYCLE_EVENTS`. Assert the JSON response does not contain a `phase_narrative` key (field is omitted via `skip_serializing_if = "Option::is_none"`).
2. Call `context_cycle_review` for a feature cycle with `CYCLE_EVENTS` rows. Assert `phase_narrative` is present and non-null.
3. `RetrospectiveReport` serialized with `phase_narrative = None` produces JSON without the key (not `"phase_narrative": null`).

**Coverage Requirement**: Both present and absent cases tested. Serialization behavior of `skip_serializing_if = "Option::is_none"` verified. Mitigates AC-12/AC-13.

---

### R-09: Hook Path Hard-Failure on `phase-end` Validation Error
**Severity**: Med
**Likelihood**: Low
**Impact**: Malformed `phase` in a hook event causes the hook to return an error to the transport, violating the hook contract (FR-03.7). This blocks the agent's tool call from executing.

**Test Scenarios**:
1. Send a hook event with `type = "phase-end"` and `phase = "scope review"` (invalid — contains space). Assert hook logs a warning and does NOT return an error to the transport; falls through to generic observation path.
2. Send a hook event with `type = "phase-end"` and `phase = ""` (empty). Same assertion: warning logged, no error returned.
3. Send a hook event with a valid `type = "phase-end"` and valid `phase = "scope"`. Assert `cycle_phase_end` event is emitted and `SessionState.current_phase` is updated.

**Coverage Requirement**: Hook fallthrough on validation failure verified. No hard-failure path exists for cycle validation errors in the hook.

---

### R-10: Fresh Database Missing New Schema Elements
**Severity**: Med
**Likelihood**: Med
**Impact**: New installation creates a database via `create_tables_if_needed` without `CYCLE_EVENTS` table or `feature_entries.phase` column. First `context_cycle` call fails with "no such table".

**Test Scenarios**:
1. Create a fresh database via `create_tables_if_needed`. Assert `cycle_events` table exists.
2. Assert `feature_entries` table has a `phase` column (via `pragma_table_info`).
3. Assert `CURRENT_SCHEMA_VERSION` is 15 in the fresh database's `counters` table.

**Coverage Requirement**: Fresh-DB path tested independently of the migration path. Both must produce identical schema at v15.

---

### R-11: `AnalyticsWrite::FeatureEntry` Match Arm Coverage
**Severity**: Med
**Likelihood**: Med
**Impact**: Internal match arms on `AnalyticsWrite` variants not updated for the new `phase` field cause a compilation error. Or if updated with `..` (struct update syntax), `phase` is silently ignored and written as `NULL` on all drain-path entries.

**Test Scenarios**:
1. Compile check (enforced by Rust): every internal match arm on `AnalyticsWrite::FeatureEntry` must destructure `phase` explicitly.
2. Integration test: enqueue `AnalyticsWrite::FeatureEntry { feature_id, entry_id, phase: Some("scope") }`, drain the queue, assert `feature_entries.phase = "scope"` in the database.
3. Enqueue with `phase: None`, drain, assert `feature_entries.phase IS NULL`.

**Coverage Requirement**: Both `Some` and `None` phase values verified through the full analytics drain path to database persistence.

---

### R-12: Current Feature Included in Cross-Cycle Mean
**Severity**: Med
**Likelihood**: Low
**Impact**: The cross-cycle comparison inflates the baseline by including the current feature's own distribution in the mean. Phase deviations appear smaller than they are.

**Test Scenarios**:
1. Seed: feature `"crt-025"` has 10 `design`-phase entries, features `"crt-001"` and `"crt-002"` each have 2. Cross-cycle mean for `design` should be 2.0 (not 4.67). Assert the returned `cross_cycle_mean` = 2.0 and `sample_features` = 2.
2. Cross-cycle SQL query explicitly filters `fe.feature_id != ?` where `?` is the current feature (per FR-10.3). Verify via query inspection or by asserting the count excludes current-feature entries.

**Coverage Requirement**: Self-exclusion from cross-cycle baseline explicitly tested with crafted data that would produce a wrong answer if self-exclusion were missing.

---

### R-13: `phase-end` with No Prior `start` — Phase Narrative Safety
**Severity**: Low
**Likelihood**: Low
**Impact**: `build_phase_narrative` panics or returns an error when `CYCLE_EVENTS` contains only `cycle_phase_end` rows with no `cycle_start` row for the same `cycle_id`.

**Test Scenarios**:
1. Insert a `cycle_phase_end` row for `cycle_id = "orphan-test"` with no corresponding `cycle_start`. Call `context_cycle_review` for that cycle. Assert no panic and a valid (possibly partial) phase narrative is returned.
2. Phase sequence derived from orphaned events should include the phases present; no crash on missing start event.

**Coverage Requirement**: Orphaned event case handled gracefully. `build_phase_narrative` is a pure function — unit-testable with a slice that starts with a `phase_end` event.

---

### R-14: `record_feature_entries` Call Site Updates
**Severity**: Med
**Likelihood**: Med
**Impact**: Call sites in `server.rs`, `services/usage.rs`, and tests still use the old two-argument signature. Compilation fails, or if using a default binding, `phase` is silently `None` on all entries regardless of active phase.

**Test Scenarios**:
1. Compile check: `record_feature_entries` signature is `(feature_cycle: &str, entry_ids: &[u64], phase: Option<&str>)` and all call sites pass the third argument.
2. Integration test: `context_store` in an active-phase session writes a non-NULL phase to `feature_entries` via the direct write path.
3. Integration test: `context_store` before any phase signal writes `NULL` via the direct write path.

**Coverage Requirement**: All three call sites verified. Direct write path and analytics drain path both tested for correct phase propagation.

---

## Integration Risks

### Phase Signal → SessionState → context_store Boundary
The critical causal chain is: `context_cycle(phase-end)` → synchronous `SessionState` mutation → `context_store` reads the updated phase. Any async interleaving between the mutation and the read breaks phase tagging for entries at phase boundaries. The synchronous-mutation design (ADR-001, ARCHITECTURE Component 5) makes this correct by design, but the test must verify the guarantee is real — not just assumed.

### `AnalyticsWrite::FeatureEntry` Enqueue-to-Drain Boundary
Phase snapshotted at enqueue time (per ADR-001) but the drain task executes later. The phase value must travel as a field on the enum variant — not be re-read from any shared state. Risk #2057 (drain task lifecycle and test isolation) from Unimatrix pattern #2057 is relevant: drain queue tests must be careful to fully drain before asserting persisted values.

### `context_cycle_review` — Three New SQL Queries
Three queries execute in the hot path of `context_cycle_review`: cycle events, current distribution, cross-cycle distribution. The cross-cycle query joins `feature_entries`, `entries`, and a subquery for feature filtering. Under a large `entries` table, this join could be slow. The index on `cycle_events(cycle_id)` and the `feature_entries(feature_id, entry_id)` primary key provide coverage; no new index risk, but query correctness against empty intermediate result sets must be tested.

### `CategoryAllowlist` Removal and Poison Recovery
Pattern #2312 shows that changes to `CategoryAllowlist` default categories cause validation failures in tests that build a config with `boosted_categories`. After removing `"outcome"` from `INITIAL_CATEGORIES`, any config fixture that lists `"outcome"` as a boosted category will fail. Audit all test fixtures that reference `"outcome"` in config structs.

---

## Edge Cases

| Edge Case | Risk ID | Test Scenario |
|-----------|---------|---------------|
| `context_cycle(type="start")` with no `next_phase` — `current_phase` stays `None` | R-01 | Assert `feature_entries.phase IS NULL` for stores in this session |
| `context_cycle(type="phase-end")` with no `next_phase` — `current_phase` unchanged | R-01 | Assert `current_phase` retains previous value, not cleared |
| `phase = " scope"` (leading space, trimmed to `"scope"` before space check) | R-06 | Assert `" scope"` normalizes and passes (trim before space check) |
| `phase = "scope "` (trailing space) | R-06 | Assert trailing space trimmed, result `"scope"` passes |
| `phase = "a b"` (internal space after trim) | R-06 | Assert rejected |
| Multiple sessions for same `cycle_id` interleaved | R-07 | Assert no panic; `(timestamp, seq)` ordering still produces sensible narrative |
| Zero entries in `feature_entries` for a feature that has `CYCLE_EVENTS` rows | R-04 | `per_phase_categories` is empty `HashMap`; no panic |
| `phase` value exactly 64 chars — boundary | R-06 | Assert accepted |
| `phase` value exactly 65 chars — boundary | R-06 | Assert rejected |
| `context_cycle_review` for a feature with `CYCLE_EVENTS` but no phase-tagged `feature_entries` | R-08 | Phase narrative present (events exist), `per_phase_categories` is empty |
| `outcome` field on `context_cycle` call — free-form string, no format restriction | R-09 | Assert any string value accepted; not validated for format |

---

## Security Risks

### Untrusted Input: `phase`, `outcome`, `next_phase` Fields
These fields arrive from MCP tool calls and hook events. `phase` and `next_phase` are normalized and format-validated (length, space check) before storage — injection risk is low for SQL via parameterized queries. However:
- **`outcome` field**: stored as-is (free-form string) with no length limit specified in the spec. A very long `outcome` string (e.g., 1MB) would be stored in `CYCLE_EVENTS.outcome TEXT`. Recommendation: add a reasonable max length (e.g., 512 chars) in `validate_cycle_params` for `outcome` to match the pattern of other free-form fields.
- **SQL injection**: all queries use parameterized statements via rusqlite/sqlx. No string interpolation in SQL. Risk is negligible given the codebase pattern.
- **Blast radius**: `CYCLE_EVENTS` is an append-only audit table. An attacker supplying malformed phase strings that pass validation would produce garbage training labels — a data quality attack, not a confidentiality breach. The engine has no cross-user data model; the blast radius is limited to W3-1 training data quality.

### Hook Path Surface
The hook intercepts `context_cycle` pre-tool-use events. Validation failure must fall through (FR-03.7) — this means a malicious or malformed `phase` in a hook event cannot block tool execution. The fallthrough is a feature, not a security gap: it prevents hook events from being used as a denial-of-service vector against tool calls.

---

## Failure Modes

| Failure | Expected Behavior | Test |
|---------|------------------|------|
| `insert_cycle_event` DB write fails (pool timeout, disk full) | Fire-and-forget: error is logged, not propagated to MCP response. Tool call succeeds. | Unit: assert `context_cycle` returns Ok even when `insert_cycle_event` returns Err |
| `record_feature_entries` fails (DB error) | Existing error handling in `context_store` — entry not recorded, error returned to caller | Covered by existing store tests |
| `context_cycle_review` cycle_events query returns DB error | Handler returns error to caller; no partial report emitted | Integration test with closed/broken DB connection |
| Phase narrative SQL query returns 0 rows | `phase_narrative = None`; no crash; backward-compatible empty response | R-08 test scenario 1 |
| `SessionState.set_current_phase` called on unknown session_id | Depends on `SessionRegistry` implementation — must not panic; should be a no-op or logged error | Unit test: call `set_current_phase` with non-existent session_id |
| `AnalyticsWrite` channel full (backpressure) | `try_send` drops the `FeatureEntry` event; `CYCLE_EVENTS` write is unaffected (direct pool) | Documented behavior; test by filling channel and asserting no panic |

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01: `current_phase` fire-and-forget mutation causes stale reads | R-01 | ADR-001 + ARCHITECTURE Component 5: mutation is synchronous within handler's task, before any DB spawn. Test in R-01 verifies guarantee. |
| SR-02: `seq` monotonicity under concurrent sessions | R-07 | ADR-002: seq is advisory; ordering at query time uses `(timestamp ASC, seq ASC)`. Test in R-07 verifies advisory behavior is safe and query uses correct ORDER BY. |
| SR-03: `outcome` removal breaks existing tests and callers | R-03 | ADR-005: remove from `INITIAL_CATEGORIES`, update test assertions per enumerated list. Test in R-03 verifies 7-category count and rejection behavior. |
| SR-04: `keywords` removal backward compatibility | — | Accepted. `deny_unknown_fields` is not set; unknown fields silently discarded. Code search confirmed no consumer reads `sessions.keywords`. No architecture-level risk. |
| SR-05: scope/vision boundary mismatch on cross-cycle comparison | — | Resolved in spec: cross-cycle is explicitly in scope as FR-10. SCOPE-RISK-ASSESSMENT concern is closed. |
| SR-06: phase label consistency across sessions | R-06 | Architecture normalizes phase strings at ingest (`validate_cycle_params`). SPECIFICATION defines canonical vocabulary. Engine does not enforce vocabulary membership. Test in R-06 verifies normalization and format rejection. |
| SR-07: phase snapshot at enqueue vs. drain-flush time | R-02 | ADR-001: phase baked into `AnalyticsWrite::FeatureEntry` struct at enqueue time. `UsageContext` carries `current_phase`. Test in R-02 verifies drain-path uses enqueue-time value. |
| SR-08: schema migration path completeness | R-05, R-10 | ADR-003 + ARCHITECTURE Component 7: both `run_main_migrations` and `create_tables_if_needed` updated. Tests in R-05 and R-10 verify both paths. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 2 (R-01, R-02) | 6 scenarios minimum |
| High | 6 (R-03, R-04, R-05, R-06, R-08, R-10, R-11, R-14) | 22 scenarios minimum |
| Medium | 4 (R-07, R-09, R-12, R-13) | 9 scenarios minimum |
| Low | 2 (R-07 concurrent aspect, R-13) | 4 scenarios minimum |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "lesson-learned failures gate rejection" — found #1688 (spawn_blocking hot-path lesson, bugfix-277), confirmed session state mutation risk is real in this codebase.
- Queried: `/uni-knowledge-search` for schema migration patterns — found #1264 (pragma_table_info idempotent ALTER TABLE) and #836 (new-table migration procedure), both directly applicable to R-05.
- Queried: `/uni-knowledge-search` for analytics drain risk patterns — found #2125 (analytics drain unsuitable for immediate-read writes), #2057 (drain task lifecycle risk), directly applicable to R-02.
- Queried: `/uni-knowledge-search` for CategoryAllowlist changes — found #2312 (boosted_categories config gotcha after category changes), directly applicable to R-03 edge case.
- Stored: nothing novel to store — R-01/R-02 risk pattern (phase snapshot at enqueue) is already captured by #2125. The specific phase-tagging risk for GNN training labels is feature-specific, not a cross-feature pattern yet.
