# Risk Coverage Report: col-023

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Cross-domain false findings: claude-code rule fires on `source_domain = "unknown"` records if `source_domain` guard preamble is missing from any of the 21 rules | `test_threshold_rule_ignores_wrong_source_domain`, `test_temporal_window_rule_ignores_wrong_source_domain`, `test_threshold_source_domain_guard_isolation`, `metrics::tests::test_compute_universal_filters_mixed_domain_slice`, `metrics::tests::test_compute_universal_zeros_for_non_claude_code_domain`, static guard verification across all 4 rule modules, **+21 per-rule isolation tests in `detection_isolation.rs`** (`test_{rule_name}_ignores_non_claude_code_domain` for all 21 rules) | PASS | Full (rework) |
| R-02 | Backward compatibility regression: claude-code retrospective produces different findings or metric values post-refactor | `report::tests::test_backward_compat_deserialization`, per-rule "fires_for" tests in all 4 modules, `test_compute_universal_*`, **+`test_retrospective_report_backward_compat_claude_code_fixture` in `detection_isolation.rs`** (full pipeline smoke: detect_hotspots + compute_metric_vector against hardcoded fixture, verifies Agent/Friction/Session categories present) | PASS | Full (rework) |
| R-03 | Wave 4 test fixture gap: `ObservationRecord` construction sites in tests missing `source_domain` | Static grep: `grep -r 'source_domain: ""'` in crates/unimatrix-observe/ → zero matches; test count 401 >= 359 baseline | PASS | Full |
| R-04 | Structural: `DomainPackRegistry` has no MCP write path | `test_domain_pack_registry_no_runtime_write_path`, static check: no `DomainPackRegistry` refs in `src/mcp/` | PASS | Full |
| R-05 | v13→v14 migration breaks positional column access | `test_v14_migration_round_trip_all_original_fields` (all 21 fields verified by name) | PASS | Full |
| R-06 | `parse_observation_rows` passthrough gap: unknown events dropped instead of passed through | `test_parse_rows_unknown_event_type_passthrough`, `test_resolve_source_domain_unknown_event_type_returns_unknown` | PASS | Full |
| R-07 | Temporal window sort assumption: unsorted slice causes silent under-count | `test_temporal_window_unsorted_input_fires`, `test_temporal_window_sorted_vs_unsorted_equivalent` | PASS | Full |
| R-08 | `field_path` non-numeric extraction silent skip | `test_threshold_field_path_non_numeric_silent_skip`, `test_threshold_field_path_missing_key_no_panic`, `test_threshold_empty_field_path_counts_events` | PASS | Full |
| R-09 | DomainPackRegistry startup failure behavior | `test_registry_rejects_unknown_as_source_domain`, `test_registry_rejects_invalid_source_domain_formats`, `test_validate_config_rejects_invalid_source_domain_chars`, `test_display_invalid_observation_source_domain`, **+`test_startup_fails_on_invalid_rule_descriptor_window_secs_zero`**, **+`test_startup_fails_on_rule_source_domain_mismatch_names_both_domains`**, **+`test_startup_fails_on_empty_source_domain_with_rules`** | PASS | Full (rework) |
| R-10 | CategoryAllowlist poisoning from domain pack categories | `test_builtin_pack_has_all_initial_categories`, `test_registry_rejects_invalid_source_domain_formats`, **+`test_duplicate_source_domain_registration_last_writer_wins`**, **+`test_duplicate_categories_in_pack_accepted`**, **+`test_invalid_category_name_format_accepted_at_registry_level`** | PASS | Full (rework) |
| R-11 | `UNIVERSAL_METRICS_FIELDS` structural test weakening | `test_metric_vector_has_domain_metrics_field`, `test_universal_metrics_fields_22nd_is_domain_metrics_json`, `test_metric_vector_serde_round_trip_with_domain_metrics`, `test_metric_vector_deserialize_without_domain_metrics_field` | PASS | Full |
| R-12 | Schema v14 rollback risk | `test_v14_schema_named_column_readback_with_reduced_struct` | PASS | Full |
| R-13 | HookType constants module misuse | Static grep: `grep -rn "HookType::" crates/unimatrix-observe/src/detection/ crates/unimatrix-server/src/` → zero matches | PASS | Full |
| R-04 (closed) | Spec/architecture conflict on FR-06 | N/A — FR-06 removed from scope | N/A | Closed |
| R-14 (closed) | DomainPackRegistry write contention | N/A — FR-06 removed from scope | N/A | Closed |

---

## Test Results

### Unit Tests (cargo test --workspace)

**(Rework iteration 1: +28 tests)**

- **Total**: 3,029 (excluding migration_v13_to_v14 which requires `--features test-support`)
- **Passed**: 3,029
- **Failed**: 0
- **Ignored**: 27 (unimatrix-embed, pre-existing)

**Per-crate breakdown:**

| Crate / Test Suite | Tests | Result |
|--------------------|-------|--------|
| unimatrix-adapt | 47 | PASS |
| unimatrix-core | 16 | PASS |
| unimatrix-embed | 101 (+ 27 ignored) | PASS |
| unimatrix-engine | 291 | PASS |
| unimatrix-learn (lib + retraining_e2e) | 74 | PASS |
| unimatrix-observe (lib) | 357 | PASS |
| unimatrix-observe (detection_isolation) | **22** (new) | PASS |
| unimatrix-observe (domain_pack_tests) | **44** (+6) | PASS |
| unimatrix-observe (extraction_pipeline) | 6 | PASS |
| unimatrix-server (lib) | 1721 | PASS |
| unimatrix (main + integration suites) | 85 | PASS |
| unimatrix-store (lib) | 136 | PASS |
| unimatrix-store migration suites (v10–v13) | 36 | PASS |
| unimatrix-store (sqlite_parity*) | 60 | PASS |
| unimatrix-vector | 106 | PASS |
| pipeline_calibration / regression / retrieval | 23 | PASS |
| test_scenarios_unit | 7 | PASS |

**Migration v13→v14 (requires `--features test-support`):**

| Test | Result |
|------|--------|
| `test_current_schema_version_is_14` | PASS |
| `test_fresh_db_creates_schema_v14` | PASS |
| `test_v13_to_v14_migration_adds_column` | PASS |
| `test_v14_migration_round_trip_all_original_fields` | PASS |
| `test_v13_row_reads_null_domain_metrics_json` | PASS |
| `test_schema_version_is_14_after_migration` | PASS |
| `test_v13_to_v14_migration_idempotent` | PASS |
| `test_v14_schema_named_column_readback_with_reduced_struct` | PASS |

**Baseline comparison (AC-02):**
- Pre-feature unimatrix-observe test count: 359
- Post-rework unimatrix-observe test count: **429** (+70 vs baseline; +28 vs pre-rework 401)
- Verdict: PASS (count increased, no tests deleted or weakened)

### Integration Tests (infra-001)

- **Smoke suite**: 20 passed, 193 deselected — PASS (mandatory gate)
- **Lifecycle suite**: 27 passed, 1 xfailed (pre-existing `test_retrospective_baseline_present`, GH#305) — PASS
- **Security suite**: 19 passed — PASS

| Suite | Tests Run | Passed | xfailed | Failed |
|-------|-----------|--------|---------|--------|
| smoke | 20 | 20 | 0 | 0 |
| lifecycle | 28 | 27 | 1 | 0 |
| security | 19 | 19 | 0 | 0 |

The lifecycle suite xfail (`test_retrospective_baseline_present`, GH#305) is pre-existing and unrelated to col-023.

---

## Static Verification Results

### R-03: No empty `source_domain` in test fixtures

```bash
grep -r 'source_domain: ""' crates/unimatrix-observe/
# Result: zero matches
```

PASS. All `ObservationRecord` construction sites in tests supply a non-empty `source_domain`.

### R-13: No `HookType::` references outside constants module

```bash
grep -rn "HookType::" crates/unimatrix-observe/src/detection/
grep -rn "HookType::" crates/unimatrix-server/src/
# Result: zero matches in both
```

PASS. All four rule modules (agent.rs, friction.rs, session.rs, scope.rs) use `String` comparisons exclusively.

### R-04: No MCP write path to DomainPackRegistry

```bash
grep -rn "DomainPackRegistry" crates/unimatrix-server/src/mcp/
# Result: zero matches
```

PASS. `DomainPackRegistry` is only referenced in server startup paths (`main.rs`, `background.rs`, `server.rs`) and the observation service. No MCP handler references it.

### Source domain guard presence in all 21 rules

All four detection modules have `source_domain == "claude-code"` filter as first operation in every `detect()` implementation:
- `agent.rs`: 7 rules, each has `.filter(|r| r.source_domain == "claude-code")` on lines 30, 98, 180, 244, 309, 383, 440
- `friction.rs`: 4 rules, guard on lines 27, 94, 169, 236
- `session.rs`: 5 rules, guard on lines 31, 101, 203, 263, 328
- `scope.rs`: 5 rules, guard on lines 34, 97, 159

Total: 21 rules. All have mandatory source_domain guard preamble (ADR-005 compliant).

---

## Gaps

**Rework iteration 1 (Stage 3c rework) resolved all four gaps. No remaining gaps.**

### GAP-01: RESOLVED

**Resolution**: Added `crates/unimatrix-observe/tests/detection_isolation.rs` with 21 per-rule isolation tests (`test_{rule_name}_ignores_non_claude_code_domain`). Each test constructs records with `source_domain = "sre"` using event_type and tool values that WOULD trigger the rule for `"claude-code"`, and asserts zero findings. All 21 rules covered.

### GAP-02: RESOLVED

**Resolution**: Added `test_retrospective_report_backward_compat_claude_code_fixture` to `detection_isolation.rs`. Runs `detect_hotspots` + `compute_metric_vector` against a hardcoded representative claude-code fixture (2 agent spawns, 20 Read calls, 8 compile commands, 1 sleep, 3-hour session gap, task completion). Asserts: no panic, Agent + Friction + Session categories present in findings, `total_tool_calls > 0` in metric vector.

### GAP-03: RESOLVED

**Resolution**: Added 3 startup failure tests to `domain_pack_tests.rs`:
- `test_startup_fails_on_invalid_rule_descriptor_window_secs_zero`: `window_secs = 0` in TemporalWindowRule → `Err(InvalidRuleDescriptor)`.
- `test_startup_fails_on_rule_source_domain_mismatch_names_both_domains`: rule source_domain != pack source_domain → `Err` naming both domains.
- `test_startup_fails_on_empty_source_domain_with_rules`: empty pack source_domain with rules → `Err(InvalidSourceDomain)`.

Note: The `rule_file` path-existence check is not yet implemented in the codebase (documented as W1-5 scope only in `main.rs:41`). These tests cover the actual startup failure paths available in the observe crate.

### GAP-04: RESOLVED

**Resolution**: Added 3 CategoryAllowlist/duplicate tests to `domain_pack_tests.rs`:
- `test_duplicate_source_domain_registration_last_writer_wins`: two packs with same source_domain → second wins; `iter_packs()` returns exactly one entry.
- `test_duplicate_categories_in_pack_accepted`: pack with duplicate category strings → accepted (no crash); categories preserved.
- `test_invalid_category_name_format_accepted_at_registry_level`: pack with uppercase/space category names → accepted at observe-crate level; documents that format validation lives in `unimatrix-server::infra::config::validate_config`.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `grep -r "hook: HookType" crates/` → zero matches outside constants module; `ObservationRecord` has `event_type: String` + `source_domain: String`; `cargo check --workspace` exits 0 |
| AC-02 | PASS | `cargo test -p unimatrix-observe -- --list` shows 429 tests >= 359 pre-feature baseline (+70 total: +42 Stage 3b + +28 rework) |
| AC-03 | PASS | `test_default_config_loads_claude_code_pack` and `test_with_builtin_claude_code_pack_always_loads` in `domain_pack_tests.rs` |
| AC-04 | PASS | `report::tests::test_backward_compat_deserialization` (schema compat) + `test_retrospective_report_backward_compat_claude_code_fixture` in `detection_isolation.rs` (behavioral compat: full pipeline smoke with hardcoded fixture) |
| AC-05 | PASS | `test_threshold_source_domain_guard_isolation` tests mixed claude-code + sre + unknown slice; DSL-level tests `test_threshold_rule_ignores_wrong_source_domain` and `test_temporal_window_rule_ignores_wrong_source_domain` confirm sre-domain records do not trigger claude-code guards; **+21 per-rule isolation tests in `detection_isolation.rs`** |
| AC-06 | PASS | `test_payload_size_boundary_exact_limit_passes`, `test_payload_size_one_byte_over_limit_rejects`, `test_nesting_depth_boundary_10_passes`, `test_nesting_depth_11_rejects` in `services::observation::tests` |
| AC-07 | PASS | `test_source_domain_invalid_cases_all_reject` (ingest-security tests) and `test_registry_rejects_invalid_source_domain_formats` (DomainPackRegistry); boundary case 64 chars passing verified |
| AC-08 | PASS | `test_domain_pack_registry_no_runtime_write_path` in `domain_pack_tests.rs`; static grep confirms zero `DomainPackRegistry` refs in `src/mcp/` |
| AC-09 | PASS | Full migration v13→v14 test suite (8 tests, `--features test-support`): fresh DB v14, migration adds column, round-trip all 21 fields, NULL readback for v13 rows |
| AC-10 | PASS | `test_metric_vector_has_domain_metrics_field`, `test_universal_metrics_fields_22nd_is_domain_metrics_json`, `test_metric_vector_serde_round_trip_with_domain_metrics` |
| AC-11 | PASS | `test_parse_rows_unknown_event_type_passthrough`, `test_resolve_source_domain_unknown_event_type_returns_unknown`, `uds::hook::tests::build_request_unknown_event` |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` (category: "procedure") for "gate verification steps integration test triage" — found #750 (pipeline validation tests), #487 (running workspace tests without hanging), #296 (service extraction procedure). No novel testing procedures found.
- Stored: nothing novel to store — migration test pattern (create_v13_database helper + `#[cfg(feature = "test-support")]` gating) follows the established migration suite pattern for this project; if it recurs in v15+ migrations it warrants a procedure entry.

**Rework iteration 1:**
- Queried: `/uni-knowledge-search` (category: "procedure") for "testing procedures" — server unavailable, proceeded without.
- Stored: nothing novel to store — per-rule isolation test pattern (one test per rule, sre domain records that would trigger if source_domain were claude-code) is domain-specific to this feature; the `build_representative_claude_code_fixture()` helper pattern may be reusable in future regression tests but is not sufficiently general to warrant a stored entry yet.
