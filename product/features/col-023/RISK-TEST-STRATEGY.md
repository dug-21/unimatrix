# Risk-Based Test Strategy: col-023

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Cross-domain false findings: a claude-code rule fires on `source_domain = "unknown"` or future domain records if the `source_domain` guard preamble is missing from any of the 21 rewritten rules | High | High | Critical |
| R-02 | Backward compatibility regression: the claude-code retrospective produces different findings or metric values post-refactor due to a string comparison error in any of the 21 rewritten rules or `compute_universal()` | High | Med | Critical |
| R-03 | Wave 4 test fixture gap: integration tests that construct `ObservationRecord` directly (bypassing the hook path) are not updated to supply both `event_type` and `source_domain`, causing test coverage to silently narrow rather than fail | High | Med | Critical |
| R-04 | Spec/architecture conflict on FR-06: SPECIFICATION.md retains FR-06 (Admin runtime domain pack override) but ADR-002 explicitly removes Admin runtime re-registration from W1-5 scope. If implemented, it introduces an unresolved MCP tool schema delta; if not implemented, AC-08 will fail | High | High | Critical |
| R-05 | v13→v14 migration breakage: `domain_metrics_json TEXT NULL` column added via `ALTER TABLE ADD COLUMN`; if any existing query against `OBSERVATION_METRICS` uses positional column indexing rather than named columns, it will silently read the wrong field | Med | Med | High |
| R-06 | `parse_observation_rows` passthrough gap: records with unregistered `event_type` are now assigned `source_domain = "unknown"` instead of being dropped; if the detection pipeline does not tolerate mixed-domain slices, a rule without a domain guard can produce phantom findings (SR-07 materialized) | High | Med | High |
| R-07 | Temporal window sort assumption: the two-pointer sliding window in `RuleEvaluator` assumes records are sorted by `ts`; if `detect()` receives an unsorted slice, the window logic silently under-counts and rules fail to fire on real violations | Med | High | High |
| R-08 | `field_path` numeric extraction silent skip: when a `field_path` in a threshold rule resolves to a non-numeric JSON value, the finding is silently suppressed rather than surfaced as a rule evaluation error; misconfigured rules produce no output and no diagnostic | Med | High | High |
| R-09 | DomainPackRegistry startup failure behavior: when `rule_file` is absent or malformed, the server fails to start (ADR-007 decision); a single misconfigured optional domain pack blocks all domains including claude-code | Med | Med | High |
| R-10 | `CategoryAllowlist` poisoning: domain pack categories added via `from_categories()` at startup cannot be removed without restart; a misconfigured pack with invalid category names could pollute the allowlist for all callers for the server lifetime | Med | Low | Medium |
| R-11 | `UNIVERSAL_METRICS_FIELDS` count mismatch: FR-05.5 requires `domain_metrics_json` as the 22nd entry in `UNIVERSAL_METRICS_FIELDS`; if the structural test (R-03/C-06) is updated to expect 22 but the count check is loosened rather than tightened, the test becomes permanently ineffective | Med | Med | Medium |
| R-12 | Schema v14 rollback: `ALTER TABLE ADD COLUMN` cannot be reversed in SQLite; a server downgrade from v14 to v13 will encounter an `OBSERVATION_METRICS` table with an unexpected column, potentially causing deserialization failures on the downgraded binary | Low | Med | Medium |
| R-13 | HookType constants module visibility: ADR-001 retains `HookType` as `pub mod hook_type` in `unimatrix-core`; if it is inadvertently used as a type rather than string constant in any Wave 3 rule rewrite, the compile-time exhaustiveness guarantee is silently restored for that one rule while all others are string-based — creating an inconsistent contract | Med | Low | Medium |
| R-14 | `DomainPackRegistry` `Arc<RwLock<_>>` write contention: the Admin runtime override path (if FR-06 is implemented) acquires a write lock on the registry while detection rule dispatch may hold a read lock; a slow Admin call during active retrospective analysis could delay detection | Low | Low | Low |

---

## Risk-to-Scenario Mapping

### R-01: Cross-Domain False Findings
**Severity**: High
**Likelihood**: High
**Impact**: claude-code retrospective reports contain phantom hotspot findings from unrelated domain events; findings are silent (no test failure, no error) and surface only in production reports.

**Test Scenarios**:
1. For each of the 21 rewritten rules: supply a mixed `ObservationRecord` slice containing `source_domain = "claude-code"` records that should fire the rule AND `source_domain = "unknown"` records that resemble the firing pattern. Assert zero findings for the unknown records.
2. Register a synthetic "sre" domain pack. Ingest a session with `source_domain = "sre"` events whose `event_type` and `tool` fields overlap with claude-code rule trigger patterns (e.g., `tool = "Bash"`). Run `detect_hotspots`. Assert no claude-code rules fire. Assert only the sre-registered rule fires (AC-05).
3. Supply a slice of only `source_domain = "unknown"` records to the full rule set. Assert no findings from any claude-code rule.

**Coverage Requirement**: All 21 rules individually tested with mixed-domain input; AC-05 multi-domain session test. No rule may produce a finding for a record whose `source_domain` does not match the rule's domain.

---

### R-02: Backward Compatibility Regression
**Severity**: High
**Likelihood**: Med
**Impact**: Existing Claude Code operators receive different retrospective findings after upgrading. Silent behavioral change — no compilation error, no test failure unless a regression snapshot exists.

**Test Scenarios**:
1. Snapshot test: capture the `RetrospectiveReport` output from a fixed, representative Claude Code session fixture using the pre-feature codebase (or a known-good baseline). After the refactor, run the same fixture through the new pipeline. Assert byte-for-byte (or field-for-field) equivalence in findings count, finding types, and metric values (AC-04).
2. For each of the 21 rules: supply a rule-triggering `ObservationRecord` slice (with `source_domain = "claude-code"`, `event_type = "PreToolUse"` etc. as strings). Assert the rule fires with the same `HotspotFinding` shape as the pre-refactor version.
3. `compute_universal()`: supply a fixed session fixture and assert all 21 metric fields produce identical values before and after the refactor.

**Coverage Requirement**: End-to-end snapshot test covering a real-world-representative session (AC-04); per-rule regression fixture for each of the 21 rules; `compute_universal()` field-level comparison.

---

### R-03: Wave 4 Test Fixture Gap
**Severity**: High
**Likelihood**: Med
**Impact**: Tests that constructed `ObservationRecord` with `hook: HookType` are updated to compile but silently supply only `event_type`, leaving `source_domain` as an empty string `""`. Rules with `source_domain == "claude-code"` guards then never fire in those tests — false green.

**Test Scenarios**:
1. After Wave 4 completes: `grep -r 'source_domain: ""'` in all test files — assert zero matches for any record intended to test claude-code behavior.
2. `grep -r 'ObservationRecord {' unimatrix-observe/tests/` — manually verify every construction site supplies both `event_type` and a non-empty `source_domain`.
3. Run `cargo test -p unimatrix-observe` and assert the test count for `unimatrix-observe` does not decrease from baseline (AC-02).

**Coverage Requirement**: Static verification (grep-based) of all test fixture construction sites; test count non-regression assertion (AC-02).

---

### R-04: Spec/Architecture Conflict on FR-06 (Admin Runtime Override)
**Severity**: High
**Likelihood**: High
**Impact**: SPECIFICATION.md FR-06 requires Admin runtime domain pack override; ADR-002 removes it from scope. If FR-06 is built, an unresolved MCP tool target (OQ-01 still open) means schema breakage risk on the chosen tool. If FR-06 is not built, AC-08 fails.

**Test Scenarios**:
1. Pre-implementation gate check: confirm FR-06 and AC-08 are explicitly resolved — either removed from the spec or a target tool is named and its schema delta is defined. This test scenario is a gate-entry blocker, not a runtime test.
2. If FR-06 is in scope: integration test with two agent trust levels — Admin caller succeeds in registering a domain pack; non-Admin caller receives `PermissionDenied` (same pattern as `context_enroll` tests in alc-002, AC-08).
3. If FR-06 is out of scope: AC-08 is removed from the acceptance criteria. Assert that `DomainPackRegistry` has no write path accessible via MCP.

**Coverage Requirement**: Resolution of the FR-06/ADR-002 conflict is a hard prerequisite for implementation. The scenario branches on the resolution; both branches are fully testable once the decision is made.

---

### R-05: v13→v14 Migration and Positional Column Access
**Severity**: Med
**Likelihood**: Med
**Impact**: A query using positional column indexing against `OBSERVATION_METRICS` silently reads `domain_metrics_json` instead of the field at the position it previously occupied — wrong values, no error.

**Test Scenarios**:
1. Migration test: start from a schema v13 database, run the v14 migration, insert a row, and read it back. Assert all 21 existing `UniversalMetrics` fields deserialize to their written values with no offset (AC-09).
2. Read-back test with NULL: simulate a schema v13 row (insert without `domain_metrics_json` column) in a v14 database. Assert `domain_metrics_json` reads back as NULL and `MetricVector.domain_metrics` deserializes as `HashMap::new()` (AC-09).
3. Schema version assertion: after migration, `PRAGMA user_version` returns 14. On a fresh database (no prior rows), schema v14 is created directly — no migration path taken.

**Coverage Requirement**: Migration round-trip test; NULL read-back test; schema version assertion. All in `unimatrix-store/src/migration.rs` test suite.

---

### R-06: Passthrough Gap — Mixed-Domain Detection Slice
**Severity**: High
**Likelihood**: Med
**Impact**: Materializes SR-07. Records with `source_domain = "unknown"` reach all 21 rules. Any rule that does not apply the mandatory preamble filter produces false findings silently.

**Test Scenarios**:
1. Integration test: ingest a session containing a mix of `"claude-code"` records and records with an unregistered event type (which get `source_domain = "unknown"`). Run the full `detect_hotspots` pipeline. Assert the finding set is identical to running with only the claude-code records (the unknown records contribute nothing).
2. Unit test for `parse_observation_rows`: assert that a record with an unregistered `event_type` produces an `ObservationRecord` with `source_domain = "unknown"` and is NOT dropped (AC-11).
3. Assert that `source_domain = "unknown"` records are stored in the `observations` table with the raw `event_type` string intact.

**Coverage Requirement**: Mixed-domain detect_hotspots integration test; parse_observation_rows unit test for passthrough behavior (AC-11); storage verification.

---

### R-07: Temporal Window Sort Assumption
**Severity**: Med
**Likelihood**: High
**Impact**: `RuleEvaluator` temporal window rules silently under-count events when the input `ObservationRecord` slice is unsorted by `ts`. No error is raised; the rule simply fails to fire on a genuine violation.

**Test Scenarios**:
1. Unit test: supply an intentionally unsorted `ObservationRecord` slice to a `temporal_window` `RuleEvaluator`. Assert the rule fires (requires either sorting within `detect()` or a documented pre-sort contract enforced by the caller).
2. Unit test: supply a sorted slice to the same rule at threshold + 1 within the window. Assert it fires. Supply the same records in reverse timestamp order. Assert it produces the same result (proving sort-independence or detecting the sorting step).
3. Boundary test: exactly N events within the window (should not fire); N+1 events within the window (should fire). Run both with sorted and unsorted input.

**Coverage Requirement**: All temporal window rule tests must include an unsorted-input case. The ADR-003 decision that `detect()` must sort or verify sort order must be enforced here.

---

### R-08: field_path Numeric Extraction Silent Skip
**Severity**: Med
**Likelihood**: High
**Impact**: A domain pack author specifies a `field_path` pointing to a string field in the payload. The rule silently produces no findings for all sessions, appearing to work (no error) while providing zero value.

**Test Scenarios**:
1. Unit test: construct a `RuleEvaluator` with a `threshold` kind and a `field_path` pointing to a string value in a synthetic payload. Assert that the rule produces no finding AND emits a log-level diagnostic (WARN or DEBUG) identifying the non-numeric extraction.
2. Unit test: `field_path` pointing to a missing key. Assert no finding, no panic.
3. Unit test: `field_path = ""` (empty, count-based threshold). Assert the rule correctly counts event occurrences without any payload extraction.

**Coverage Requirement**: `field_path` non-numeric and missing-key cases tested; count-based (empty `field_path`) case tested.

---

### R-09: DomainPackRegistry Startup Failure Behavior
**Severity**: Med
**Likelihood**: Med
**Impact**: A single misconfigured external domain pack (e.g., a `rule_file` path that does not exist) prevents the server from starting entirely, blocking the claude-code pack and all existing functionality.

**Test Scenarios**:
1. Integration test: start the server with a config containing a `rule_file` path that does not exist. Assert the server fails to start with a clear error message naming the missing file (not a panic, not a generic error).
2. Integration test: start the server with a config containing a valid `rule_file` with a syntactically malformed rule descriptor (e.g., missing required `source_domain` field). Assert startup failure with a message naming the invalid rule.
3. Integration test (baseline): start the server with no `[observation]` section at all. Assert the server starts successfully with the claude-code pack active (AC-03).

**Coverage Requirement**: Startup failure error path tests for both absent file and malformed descriptor; default-config startup test (AC-03).

---

### R-10: CategoryAllowlist Poisoning
**Severity**: Med
**Likelihood**: Low
**Impact**: A domain pack specifying categories that conflict with or shadow existing `INITIAL_CATEGORIES` could alter `context_store` behavior for all callers for the server lifetime. Categories cannot be removed without restart.

**Test Scenarios**:
1. Unit test: register a domain pack whose `categories` include a value already in `INITIAL_CATEGORIES`. Assert the allowlist does not duplicate the entry and existing behavior is unchanged.
2. Unit test: register a domain pack with a category that uses an invalid format (e.g., uppercase, contains spaces). Assert the server rejects the pack at startup with a clear error rather than silently adding an unusable category.

**Coverage Requirement**: Duplicate category idempotency test; invalid category format rejection test.

---

### R-11: UNIVERSAL_METRICS_FIELDS Structural Test Weakening
**Severity**: Med
**Likelihood**: Med
**Impact**: The structural test (R-03/C-06) is updated to expect 22 entries. If the count check is updated correctly but the per-field name alignment check is weakened or removed, the test no longer detects future field regressions in the 21 existing columns.

**Test Scenarios**:
1. Verify the updated structural test: assert it checks `UNIVERSAL_METRICS_FIELDS.len() == 22` AND individually verifies each of the original 21 field names against the SQL column names in declaration order (AC-10).
2. Negative test: temporarily remove one of the 21 field names from `UNIVERSAL_METRICS_FIELDS` — assert the structural test fails. Restore it.
3. Verify `domain_metrics_json` is verified separately by name in the test (not included in the 21-field alignment check per ADR-006).

**Coverage Requirement**: The structural test must enforce column count (22) AND per-field alignment for all 21 original fields AND separate presence check for `domain_metrics_json` (AC-10).

---

### R-12: Schema v14 Rollback Risk
**Severity**: Low
**Likelihood**: Med
**Impact**: Operators downgrading to a pre-v14 binary encounter `OBSERVATION_METRICS` with an unexpected column. Deserialization may succeed silently (extra column ignored) or fail depending on the query pattern.

**Test Scenarios**:
1. Read-back test: use a v14 database schema with a v13-equivalent Rust struct for `MetricVector` (without `domain_metrics`). Assert named-column queries still return correct values for the 21 original fields.
2. Document the rollback risk explicitly in the migration test suite comments — this is a test-infrastructure stewardship item, not a runtime failure.

**Coverage Requirement**: Named-column read-back test using a schema-v14 database with a reduced struct.

---

### R-13: HookType Constants Module Misuse
**Severity**: Med
**Likelihood**: Low
**Impact**: If any Wave 3 implementor imports `hook_type::PRETOOLUSE` as a type constant but the surrounding match expression pattern uses it as if it were a type — or if a test constructs `hook_type::PRETOOLUSE` and then does a type-level comparison — compile-time safety is partially restored for that one file while all other rules use string comparisons. The inconsistency is caught only by careful code review.

**Test Scenarios**:
1. `grep -r "HookType::"` across the workspace after Wave 3 completes. Assert zero matches outside the `hook_type` constants module itself and its documentation.
2. `grep -r "use.*hook_type"` — assert all imports reference the constants module, not a re-exported enum type.

**Coverage Requirement**: Static verification (grep-based) as part of the Wave 3 compilation gate checklist.

---

## Integration Risks

**IR-01: DomainPackRegistry passed into SqlObservationSource**
The `DomainPackRegistry` must be threaded as `Arc` through `SqlObservationSource` to allow `parse_observation_rows` to resolve `source_domain` from known event type strings. If the registry is not injected (e.g., defaults to an empty registry), all records get `source_domain = "unknown"` including claude-code events, and all 21 rules silently produce no findings. Test: assert that with the default built-in claude-code pack, `event_type = "PreToolUse"` resolves to `source_domain = "claude-code"`, not `"unknown"`.

**IR-02: CategoryAllowlist and DomainPack initialization ordering**
`CategoryAllowlist` is populated at startup from domain pack categories. If the server initializes `CategoryAllowlist` before loading `DomainPackConfig` from TOML, non-claude-code domain categories will be absent for the lifetime of the server. Test: assert that after startup with a custom domain pack in config, `context_store` accepts entries with that pack's declared categories.

**IR-03: `compute_universal()` called with non-claude-code records**
`compute_universal()` in `metrics.rs` applies string comparisons against `"PreToolUse"` etc. If it is called on a session containing non-claude-code records without the `source_domain` guard, it will compute non-zero counts for those records when their `event_type` strings happen to match claude-code event names. Test: supply a session with only `source_domain = "sre"` records where one has `event_type = "PostToolUse"`. Assert all 21 `UniversalMetrics` fields are zero.

**IR-04: `context_cycle_review` unchanged MCP shape with new `domain_metrics`**
`RetrospectiveReport` shape is unchanged. `MetricVector` gains `domain_metrics: HashMap<String, f64>`. If `MetricVector` is serialized into `RetrospectiveReport` by reference, the new field must either be excluded from the report or serialize as `{}` for claude-code sessions. Test: invoke `context_cycle_review` for a claude-code session and assert the response shape is identical to the pre-feature schema (AC-04).

---

## Edge Cases

**EC-01: Payload exactly 64 KB (65,536 bytes)** — must pass; 65,537 bytes must reject with `PayloadTooLarge` (AC-06, boundary condition).

**EC-02: JSON nesting exactly 10 levels deep** — must pass; 11 levels must reject with `NestingTooDeep` (AC-06, boundary condition).

**EC-03: `source_domain` exactly 64 characters matching `[a-z0-9_-]`** — must pass; 65 characters must reject (AC-07).

**EC-04: `source_domain = "unknown"` as an explicit registration attempt** — `"unknown"` is a reserved string (used internally for unrecognized events). A domain pack attempting to register `source_domain = "unknown"` must be rejected at startup with a clear error. This case is not explicitly covered by AC-07 but follows from the reserved-string semantics in FR-03.2.

**EC-05: Empty `event_types` list in a domain pack** — a pack with `event_types = []` means all events from that domain are valid. The registry lookup must not treat an empty list as "no match" — it must treat it as "all event types match for this domain".

**EC-06: Session with zero observations** — `detect_hotspots` called with an empty slice. All rules must return empty `Vec` without panicking. `compute_universal()` must return a zero-value `UniversalMetrics`.

**EC-07: Two domain packs with overlapping `event_type` strings (e.g., both declare `event_type = "start"`)** — since `source_domain` is assigned server-side from ingress path (not from `event_type` lookup), this is not a conflict at ingest. However, the registry lookup must handle it consistently. Test: register two packs with a shared event type string; assert `source_domain` assignment is deterministic.

**EC-08: Temporal window rule with `window_secs = 0`** — must be rejected at startup as an invalid rule descriptor (zero window is meaningless and would fire on every event). Test: assert `InvalidRuleDescriptor` error at load time.

**EC-09: `rule_file` specifying a rule whose `source_domain` does not match the domain pack's own `source_domain`** — a rule file loaded for the "sre" pack containing a rule with `source_domain = "claude-code"` is a misconfiguration. Test: assert startup failure or rule rejection with a clear mismatch error.

---

## Security Risks

**SEC-01: Payload size bypass via Unicode**
The size check operates on raw byte length (`input_str.len()`) before JSON parse (ADR-007). A payload constructed with multi-byte UTF-8 characters to stay under 65,536 bytes while carrying more semantic content than a valid ASCII payload is acceptable — the byte bound is enforced correctly. Risk is Low. Test: assert that a payload with exactly 65,536 bytes of valid UTF-8 (multi-byte characters) passes; one that exceeds 65,536 bytes of raw UTF-8 is rejected regardless of character count.

**SEC-02: Recursive depth check stack overflow**
`json_depth()` is recursive. The 64 KB size bound limits total node count. At 10 levels, the worst case is a tree of JSON objects each with one key, which requires O(10) stack frames — far below stack limits. ADR-007 explicitly confirms this is safe. Residual risk: an implementation that applies the depth check after full deserialization rather than during tree walk does not gain the 64 KB protection. Test: assert the depth check is applied to the already-deserialized value and that combined with the byte-size pre-check, no unbounded recursion is possible.

**SEC-03: `field_path` injection in rule descriptors**
JSON Pointer strings accept `~0` (tilde-escape for `~`) and `~1` (escape for `/`). A maliciously crafted `field_path` could attempt to reference unexpected paths in the payload. `serde_json::Value::pointer()` is a read-only path navigator with no side effects — there is no injection risk at the Rust level. Risk is Low. Test: assert that a `field_path` with `~0~1` escape sequences resolves to the correct nested value and produces no side effects.

**SEC-04: Admin override path (FR-06) — write lock during concurrent retrospective analysis**
If FR-06 is implemented: a write lock on `DomainPackRegistry` while `detect_hotspots` holds a read lock (for `rules_for_domain`) can cause a blocking delay. The blast radius is a delayed retrospective response, not data corruption or security breach. Risk is Low. Test: concurrent Admin override + retrospective analysis under load; assert both complete without deadlock within a reasonable timeout.

**SEC-05: `source_domain` set server-side — but future ingress paths may not enforce this**
W1-5 only generalizes the `RecordEvent` hook path. The `source_domain` is always `"claude-code"` on this path. If a future ingress path accepts a client-declared `source_domain`, the regex validation at ingest (ADR-007 Bound 3) is the only defense. This is a future-path risk. The mitigation is that the validation code path exists now. Test: assert the validation function is covered by unit tests even though it is currently a no-op on the hook path (AC-07).

---

## Failure Modes

**FM-01: Server startup failure on invalid domain pack config**
Expected behavior: server fails to start with a structured error message identifying the invalid pack by `source_domain` and the specific invalid field (`rule_file` absent, malformed rule descriptor, invalid `source_domain` regex, `source_domain = "unknown"` reserved). The claude-code pack must not be blamed. No partial state.

**FM-02: Event ingest rejection (PayloadTooLarge, NestingTooDeep)**
Expected behavior: the specific event is skipped; a WARN-level log is emitted with `session_id`, `event_type`, and the measured violation value; all other events in the session are processed normally. The `RetrospectiveReport` for the session reflects only the accepted events.

**FM-03: `detect_hotspots` panic prevention**
Expected behavior: if any rule's `detect()` panics (e.g., due to an unexpected `None` in a field it expected to be `Some`), the panic must not crash the server. The retrospective pipeline runs rules via `spawn_blocking`/rayon — panics within individual rules must be caught via `catch_unwind` or an equivalent boundary. A panicking rule returns an empty finding set and logs an error.

**FM-04: `DomainPackRegistry` write lock poisoning**
If the Admin override write operation panics with the lock held, the `RwLock` is poisoned. The poison recovery pattern (`.unwrap_or_else(|e| e.into_inner())`) used in the `CategoryAllowlist` (production precedent) must be applied here as well. Expected behavior: subsequent reads succeed; poisoned write operation is logged at ERROR.

**FM-05: Schema v14 migration on a corrupted or partially-migrated database**
Expected behavior: if `OBSERVATION_METRICS` already has a `domain_metrics_json` column (from a failed partial migration), `ALTER TABLE ADD COLUMN` returns a SQLite error. The migration system must detect and handle this idempotently (check column existence before applying, or use `IF NOT EXISTS` if available for the SQLite version in use).

---

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (DSL insufficient for temporal rules) | R-07, R-08 | Resolved by ADR-003: `RuleEvaluator` host struct handles temporal aggregation; `json_pointer` is used only for payload field extraction within threshold rules. Operator surface is bounded (threshold + temporal window only). |
| SR-02 (Two live representations of UniversalMetrics) | R-11 | Resolved by ADR-006 (Option A): `UniversalMetrics` typed struct is the single canonical representation. No HashMap at the logical level. `domain_metrics` is a separate extension field. |
| SR-03 (Multi-domain path has no production exercising) | R-03, R-06 | Partially mitigated by AC-05 synthetic integration test. The hook ingress will always produce `source_domain = "claude-code"` in W1-5. W3-1's unblocking gate is explicitly narrowed: pipeline accepts multi-domain events, rules gate on `source_domain` — no production multi-domain ingress required. |
| SR-04 (HookType blast radius across 25+ files) | R-03, R-13 | Resolved by ADR-004: four-wave compilation-gated refactor with `cargo check --workspace` gate between waves. All 25 callsites enumerated and partitioned into waves. |
| SR-05 (Admin runtime re-registration unresolved) | **R-04** | **CONFLICT**: ADR-002 explicitly removes Admin runtime re-registration from W1-5 scope. SPECIFICATION.md FR-06 retains it as a functional requirement with OQ-01 still open. This is an unresolved spec/architecture conflict. The implementor must resolve before implementation begins: either remove FR-06 and AC-08 from the spec, or name the target tool and define the schema delta. This is a gate-entry blocker. |
| SR-06 (BaselineSet deserialization assumption) | R-05 | Accepted: ADR-006 confirms `BaselineSet.universal` is already `HashMap<String, BaselineEntry>` with string keys matching field names. No migration required. OQ-03 in the spec documents the verification obligation for the architect. |
| SR-07 (Cross-domain false findings from unknown passthrough) | R-01, R-06 | Resolved by ADR-005: mandatory `source_domain` guard preamble in every `DetectionRule::detect()` implementation. Enforced at gate-3a review checklist. AC-05 tests the cross-domain isolation. |
| SR-08 (W3-1 "fully functional" gate ambiguity) | — | Resolved in spec AC-05: W3-1 requires only that the pipeline accepts multi-domain events and that detection rules gate correctly on `source_domain`. Pre-built multi-domain production rules are not required. |

---

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 4 (R-01, R-02, R-03, R-04) | 12 scenarios minimum; R-04 is a gate blocker before any runtime tests apply |
| High | 5 (R-05, R-06, R-07, R-08, R-09) | 15 scenarios |
| Medium | 4 (R-10, R-11, R-12, R-13) | 8 scenarios |
| Low | 1 (R-14) | 1 scenario |

Non-negotiable tests (must pass for gate-3c):
- R-01: mixed-domain rule isolation for all 21 rules
- R-02: backward compatibility snapshot test (AC-04)
- R-06: unknown event passthrough test (AC-11)
- R-07: temporal window with unsorted input
- SEC-04 (if FR-06 implemented): concurrent lock test

---

## Knowledge Stewardship
- Queried: `/uni-knowledge-search` for lesson-learned failures, gate rejection, observation pipeline — found #699 (silent data orphaning via hardcoded None in hook pipeline, directly informs R-01/R-06 severity elevation), #2758 (gate-3c non-negotiable test name validation, informs coverage summary), #1203 (cascading rework from incomplete gate validation, informs R-03)
- Queried: `/uni-knowledge-search` for risk pattern — found #261 (AuditSource-driven behavior differentiation pattern, confirms source_domain guard as architectural security pattern per ADR-005), #377 (wave-based refactoring with compilation gates, confirms R-03 test fixture gap risk is well-precedented), #363 (wave-based subtractive refactoring, same)
- Queried: `/uni-knowledge-search` for SQLite migration backward compatibility — found #370/#681 (create-new-then-swap migration pattern), #760 (independent migration versioning), informing R-05 and FM-05
- Stored: nothing novel to store — R-01 (cross-domain false finding contamination pattern) is specific to this feature's architecture; if it recurs across 2+ features it warrants a pattern entry.
