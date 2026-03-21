# Gate 3a Report: col-023

> Gate: 3a (Component Design Review)
> Date: 2026-03-21
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 8 components map to architecture sections; ADRs reflected |
| Specification coverage | PASS | All FR-01 through FR-05, all NFRs addressed in pseudocode |
| Risk coverage in test plans | PASS | All 13 active risks mapped to named tests |
| Interface consistency | WARN | `iter_packs()` absent from OVERVIEW shared types table |
| ADR-005 source_domain guard as first filter | PASS | All 21 rules + RuleEvaluator use guard as Step 1 |
| Constraint 12: ts-sort before temporal window scan | PASS | Explicit STEP 3 sort in detect_temporal_window |
| Constraint 10: "unknown" reserved domain rejected at startup | PASS | DomainPackRegistry::new() explicitly checks and rejects |
| IR-01: DomainPackRegistry Arc into server (non-optional) | PASS | SqlObservationSource gains registry field; startup wiring confirmed |
| 21 rules with per-rule mixed-domain isolation tests | PASS | detection-rules.md test plan names 21 tests + unknown-only slice test |
| Backward-compat snapshot test for RetrospectiveReport | PASS | T-DET-COMPAT-02 covers AC-04 end-to-end with field-level assertions |
| T-SEC-12 test expectation vs hook-path behavior | WARN | Test expects source_domain="unknown" but hook path always assigns "claude-code" |
| Knowledge Stewardship — pseudocode agent | PASS | Queried entries present; read-only agent |
| Knowledge Stewardship — test-plan agent | PASS | Queried and Stored entries present |

---

## Detailed Findings

### Architecture Alignment
**Status**: PASS
**Evidence**: Every architecture component has a corresponding pseudocode file:
- `unimatrix-core/src/observation.rs` → `pseudocode/observation-record.md` (Wave 1)
- `unimatrix-observe/src/domain/mod.rs` → `pseudocode/domain-pack-registry.md` + `pseudocode/rule-dsl-evaluator.md` (Wave 2)
- `unimatrix-server/src/infra/config.rs` → `pseudocode/config-extension.md` (Wave 2)
- `unimatrix-observe/src/detection/*.rs` + `extraction/*.rs` → `pseudocode/detection-rules.md` (Wave 3)
- `unimatrix-store/src/metrics.rs` + `unimatrix-observe/src/metrics.rs` → `pseudocode/metrics-extension.md` (Wave 3)
- `unimatrix-store/src/migration.rs` → `pseudocode/schema-migration.md` (Wave 3)
- `unimatrix-server/src/services/observation.rs` → `pseudocode/ingest-security.md` (Wave 4)

All 7 ADRs have corresponding pseudocode coverage. The four-wave compilation-gated sequence is documented in OVERVIEW.md with explicit `cargo check --workspace` gates. Technology choices (rusqlite, serde, no new crates) are consistent with ADR-005, ADR-006, ADR-007.

---

### Specification Coverage
**Status**: PASS
**Evidence**:

FR-01 (ObservationRecord): `observation-record.md` replaces `hook: HookType` with `event_type: String` + `source_domain: String`, retains all 7 other fields, and converts `HookType` to a `pub mod hook_type` constants module. Covers FR-01.1 through FR-01.4.

FR-02 (DomainPackRegistry): `domain-pack-registry.md` defines `DomainPack`, `DomainPackRegistry` with `Arc<RwLock<HashMap<String, DomainPack>>>`, built-in "claude-code" pack always loaded, `#[serde(default)]` config section, category registration. Covers FR-02.1 through FR-02.7.

FR-03 (Event Ingest): `ingest-security.md` removes `HookType` match arm, assigns `source_domain = "claude-code"` for all hook-path records, enforces 64 KB size limit, 10-level depth limit, and `source_domain` regex validation. Covers FR-03.1 through FR-03.7.

FR-04 (Detection Rule Generalization): `detection-rules.md` rewrites all 21 rules to use string comparisons with mandatory `source_domain` guard as first operation. `rule-dsl-evaluator.md` defines threshold and temporal_window operators. `default_rules()` returns 21 rules; `domain_rules()` appends DSL rules. Covers FR-04.1 through FR-04.7.

FR-05 (UniversalMetrics Extension): `metrics-extension.md` adds `domain_metrics: HashMap<String, f64>` to `MetricVector`, adds `domain_metrics_json` as 22nd entry in `UNIVERSAL_METRICS_FIELDS`, writes NULL for claude-code sessions, handles v13 NULL read-back as empty map. `schema-migration.md` implements v13 → v14 `ALTER TABLE ADD COLUMN` with idempotency check. Covers FR-05.1 through FR-05.6.

NFRs: NFR-01 (backward compat) — addressed throughout pseudocode. NFR-02 (payload limits) — `ingest-security.md` Step 1/2. NFR-03 (domain validation) — `domain-pack-registry.md` `validate_source_domain_format()`. NFR-04 (rule sandboxing) — `rule-dsl-evaluator.md` explicitly states no eval, no filesystem access. NFR-05 (no new deps) — manual char check used instead of regex crate. NFR-08 (schema v14) — `schema-migration.md`. NFR-09 (compilation gates) — OVERVIEW.md.

---

### Risk Coverage in Test Plans
**Status**: PASS
**Evidence**: All 13 active risks (R-01 through R-13, minus closed R-04 and R-14) have named test scenarios:

| Risk | Test Location | Named Tests |
|------|--------------|-------------|
| R-01 (Critical) | `test-plan/detection-rules.md` | 21 × `test_{rule_name}_no_findings_for_unknown_domain` + `test_all_21_rules_produce_no_findings_for_unknown_only_slice` |
| R-02 (Critical) | `test-plan/detection-rules.md` | `test_{rule_name}_backward_compat_fires_for_claude_code_fixture` × 21 + T-DET-COMPAT-02 snapshot |
| R-03 (Critical) | Static grep + count | `grep -r 'source_domain: ""'` zero-match gate; test count non-regression assertion |
| R-05 (High) | `test-plan/schema-migration.md` | T-MIG-03 named-column round-trip |
| R-06 (High) | `test-plan/ingest-security.md` | T-SEC-12 unknown passthrough; T-SEC-14 partial batch |
| R-07 (High) | `test-plan/rule-dsl-evaluator.md` | T-DSL-12 unsorted input fires; T-DSL-13 sorted/unsorted equivalent |
| R-08 (High) | `test-plan/rule-dsl-evaluator.md` | T-DSL-05 non-numeric skip; T-DSL-06 missing key |
| R-09 (High) | `test-plan/config-extension.md` | T-CFG-07 invalid domain; T-CFG-08 missing rule_file; T-CFG-09 malformed descriptor |
| R-10 (Med) | `test-plan/domain-pack-registry.md` | T-DPR-12 duplicate idempotent; T-DPR-13 invalid format rejected |
| R-11 (Med) | `test-plan/metrics-extension.md` | T-MET-01 count 22; T-MET-02 21 original unchanged; T-MET-03 22nd is domain_metrics_json |
| R-12 (Med) | `test-plan/schema-migration.md` | T-MIG-07 v14 schema named-column readback with reduced struct |
| R-13 (Med) | Static grep | `grep -r "HookType::"` zero-match gate post-Wave-3 |
| IR-01 | `test-plan/domain-pack-registry.md` | T-DPR-01 + resolve_source_domain test |
| IR-02 | `test-plan/domain-pack-registry.md` + `config-extension.md` | T-DPR-12; T-CFG-06 |
| IR-03 | `test-plan/metrics-extension.md` | T-MET-10; T-MET-11 |

Integration risks (EC-01 through EC-09) and Security risks (SEC-01 through SEC-03) are also covered by named tests in `test-plan/ingest-security.md`.

All non-negotiable Gate 3c tests are named and placed in specific files:
1. R-01 per-rule isolation: `detection_isolation.rs` 21 tests
2. R-02 snapshot: `detection_isolation.rs` T-DET-COMPAT-02
3. R-04 structural: `domain_pack_tests.rs` T-DPR-11
4. R-06 unknown passthrough: `services/observation.rs` T-SEC-12
5. R-07 temporal unsorted: `domain/mod.rs` T-DSL-12

---

### Interface Consistency
**Status**: WARN
**Evidence**: Shared types defined in `pseudocode/OVERVIEW.md` are internally consistent and match per-component pseudocode for `ObservationRecord`, `DomainPack`, `DomainPackRegistry`, `MetricVector`, `ObserveError` variants, and `ObservationConfig`/`DomainPackConfig`.

**Issue (WARN)**: `DomainPackRegistry::iter_packs()` is used in both `ingest-security.md` and `config-extension.md` startup wiring sequences, but is not listed in the `DomainPackRegistry` methods table in `pseudocode/OVERVIEW.md`. The method is defined in `ingest-security.md`:
```
pub fn iter_packs(&self) -> Vec<DomainPack>:
    let guard = self.inner.read().unwrap_or_else(|e| e.into_inner())
    guard.values().cloned().collect()
```
This is not a blocking gap — the method is fully specified — but the OVERVIEW omission could cause a Wave 4 implementor to miss it when reviewing the shared type surface. The implementation agent should ensure `iter_packs()` is implemented as part of the `domain-pack-registry` Wave 2 work and re-exported alongside the other public methods.

---

### ADR-005: source_domain Guard as First Filter
**Status**: PASS
**Evidence**: `pseudocode/rule-dsl-evaluator.md` `detect_threshold()`:
```
-- STEP 1: source_domain guard (MANDATORY FIRST FILTER — ADR-005)
let domain_records: Vec<&ObservationRecord> = records
    .iter()
    .filter(|r| r.source_domain == rule.source_domain)
    .collect()
```
Same pattern in `detect_temporal_window()`. `pseudocode/detection-rules.md` defines the "Mandatory source_domain Guard Pattern (ADR-005)" block and shows it applied to every rule across all four rule modules (agent.rs 7 rules, friction.rs 4 rules, session.rs 5 rules, scope.rs 5 rules) and all 5 extraction rules. `pseudocode/metrics-extension.md` shows `compute_universal()` pre-filters `source_domain == "claude-code"`.

No domain-specific rule in the pseudocode lacks the guard preamble.

---

### Constraint 12: ts-Sort Before Temporal Window Scan
**Status**: PASS
**Evidence**: `pseudocode/rule-dsl-evaluator.md` `detect_temporal_window()`:
```
-- STEP 3: Sort by ts (MANDATORY — Constraint 12, R-07)
let mut sorted: Vec<&ObservationRecord> = filtered
sorted.sort_by_key(|r| r.ts)
-- STEP 4: Two-pointer sliding window max-count
```
The sort is performed on the domain-filtered, event_type-filtered slice before the two-pointer sliding window scan begins. Test T-DSL-12 (`test_temporal_window_unsorted_input_fires`) verifies this at Gate 3c.

---

### Constraint 10: "unknown" Reserved Source Domain Rejected at Startup
**Status**: PASS
**Evidence**: `pseudocode/domain-pack-registry.md` `DomainPackRegistry::new()`:
```
-- Validate source_domain is not reserved "unknown"
if pack.source_domain == "unknown":
    return Err(ObserveError::InvalidSourceDomain {
        domain: "unknown".to_string()
    })
```
This check appears before the regex validation check, ensuring `"unknown"` fails with a specific reserved-domain error rather than a generic format error. Test T-DPR-07 (`test_registry_rejects_unknown_as_source_domain`) covers EC-04. The failure mode is correctly documented in FM-01.

---

### IR-01: DomainPackRegistry Threaded as Arc into Server (Non-Optional)
**Status**: PASS
**Evidence**: `pseudocode/ingest-security.md` defines the `SqlObservationSource` extension:
```
pub struct SqlObservationSource:
    store: Arc<SqlxStore>
    registry: Arc<DomainPackRegistry>    -- NEW
```
Startup wiring in `lib.rs` (pseudocode/ingest-security.md and pseudocode/config-extension.md):
```
let registry = DomainPackRegistry::new(packs)?
let registry_arc = Arc::new(registry)
let obs_source = SqlObservationSource::new(store.clone(), registry_arc)
```
The `registry` field is not optional (`Option<Arc<_>>`). `parse_observation_rows()` signature includes `registry: &DomainPackRegistry` as a required parameter. Both callsites (`load_feature_observations()` and `load_unattributed_sessions()`) must pass `&self.registry`. Test T-CFG-06 validates the wiring end-to-end.

---

### 21 Rules with Per-Rule Mixed-Domain Isolation Tests (R-01, Critical)
**Status**: PASS
**Evidence**: `test-plan/detection-rules.md` explicitly specifies:
> "Mixed-Domain Isolation Tests (R-01, AC-05) — One per rule, 21 total"

The naming convention is `test_{rule_name}_no_findings_for_unknown_domain` for each of the 21 rules. Additionally:
- `test_all_21_rules_produce_no_findings_for_unknown_only_slice`: 100 records with `source_domain = "unknown"` and `event_type = "PostToolUse"` supplied to all 21 rules; asserts zero findings total.
- `test_sre_domain_events_trigger_sre_rule_not_claude_code_rules`: AC-05 full multi-domain test.

The test plan names the four modules (`agent.rs`: ~5 rules, `friction.rs`: ~6 rules, `session.rs`: ~5 rules, `scope.rs`: ~5 rules) and confirms all tests use `source_domain` string values, not `HookType` enum variants.

---

### Backward-Compat Snapshot Test for RetrospectiveReport (R-02, Critical)
**Status**: PASS
**Evidence**: `test-plan/detection-rules.md` T-DET-COMPAT-02:
```
// test_retrospective_report_backward_compat_claude_code_fixture
// Arrange: fixed hardcoded ObservationRecord slice (2-4 agent spawns, ~50 tool calls)
// All records: source_domain = "claude-code", event_type = "PostToolUse" etc.
// Act: detect_hotspots + compute_metric_vector
// Assert (field-by-field):
//   (a) findings count == EXPECTED_FINDINGS_COUNT (hardcoded baseline)
//   (b) finding types == EXPECTED_FINDING_TYPES
//   (c) All 21 UniversalMetrics fields == EXPECTED_METRIC_VALUES
```
The test plan explicitly instructs the Stage 3b implementor to capture the baseline values on pre-feature `main` before making changes. This provides the snapshot anchor that R-02 requires.

---

### T-SEC-12 Test Expectation vs Hook-Path Behavior
**Status**: WARN
**Evidence**: `test-plan/ingest-security.md` T-SEC-12 asserts:
```
// Assert: record.source_domain == "unknown"
```
for a record with an unregistered `event_type` arriving via `parse_observation_rows`.

However, `pseudocode/ingest-security.md` hard-codes:
```
-- Assign source_domain = "claude-code" for all hook-path records (FR-03.3)
let source_domain: String = "claude-code".to_string()
```
The hook-path implementation always assigns `"claude-code"`, regardless of whether the `event_type` is registered. This is correct per FR-03.3 ("Events arriving via the unimatrix hook CLI ingress path shall always be assigned `source_domain = "claude-code"` server-side").

The conflict: T-SEC-12 tests `parse_observation_rows` with a registry that has only the claude-code pack. On the hook path, an unregistered event type would get `source_domain = "claude-code"`, not `"unknown"`. FR-03.2 (unregistered events get `"unknown"`) applies to non-hook ingress paths that do not exist in W1-5.

**Impact**: T-SEC-12 as written would fail if the pseudocode is implemented faithfully. The test-plan agent intended to test FR-03.2/AC-11 passthrough behavior, but the test's expected `source_domain` value contradicts FR-03.3.

**Fix required at Stage 3b**: Update T-SEC-12 to either:
(a) Assert `record.source_domain == "claude-code"` (matching the hook-path pseudocode and FR-03.3), and separately document that AC-11 ("records with unregistered event_type pass through") means they are stored with `source_domain = "claude-code"` on the hook path; or
(b) Add a separate unit test for a hypothetical non-hook ingress path where `resolve_source_domain` is called, asserting it returns `"unknown"` for unregistered event types.

This is a WARN rather than a FAIL because:
- The pseudocode itself is correct per the spec (FR-03.3 takes precedence on the hook path)
- The test is in the test plan, not yet implemented
- The fix is localized to one test expectation, not a design error

---

### Knowledge Stewardship — Pseudocode Agent
**Status**: PASS
**Evidence**: `agents/col-023-agent-1-pseudocode-report.md` contains `## Knowledge Stewardship` section with two `Queried:` entries documenting searches for detection rule patterns and architectural decisions. Read-only agents require `Queried:` entries only — this requirement is satisfied.

---

### Knowledge Stewardship — Test-Plan Agent
**Status**: PASS
**Evidence**: `agents/col-023-agent-2-testplan-report.md` contains `## Knowledge Stewardship` section with `Queried:` entries and `Stored: entry #2928 "String-Refactor Test Plan Patterns: Domain Isolation, Backward-Compat Snapshot, Static Grep Gates"` via `/uni-store-pattern`. Both read-query and write-store obligations are satisfied.

---

## Rework Required

No FAIL items. Two WARN items do not block gate passage:

| Issue | Which Agent | What to Fix |
|-------|-------------|-------------|
| `iter_packs()` absent from OVERVIEW shared types | uni-rust-dev (Wave 2 domain-pack-registry implementor) | Implement `iter_packs()` as part of `DomainPackRegistry` in Wave 2; ensure it is exported at the `pub mod domain` level alongside other public methods |
| T-SEC-12 expected `source_domain` contradicts FR-03.3 | uni-rust-dev (Wave 4 ingest-security implementor) | When writing T-SEC-12, assert `source_domain == "claude-code"` for hook-path unknown event types; document that FR-03.2 passthrough applies only to non-hook ingress paths |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "gate validation cross-domain rule isolation pattern" — not executed (gate validator does not query Unimatrix per its role definition; knowledge base was accessed via prior agent stewardship entries). No recurring gate failure patterns observed across features in this gate run — T-SEC-12 expectation conflict is feature-specific, not a systemic pattern across features yet.
- Stored: nothing novel to store — the T-SEC-12 ingest-path/spec-expectation tension is feature-specific. If this pattern (test plan expecting behavior that contradicts the spec on edge-path records) recurs in two or more future gate reviews, it warrants a lesson-learned entry.
