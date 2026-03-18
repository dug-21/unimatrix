# Risk Coverage Report: dsn-001 — Config Externalization (W0-3)

Generated: 2026-03-18
Tester agent: dsn-001-agent-11-tester

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | ConfidenceParams call site migration incomplete — compiled constants still used | `test_compute_confidence_uses_params_w_fresh`, `test_freshness_score_uses_params_half_life`, `test_freshness_score_configurable_half_life`; static audit: `W_BASE/W_FRESH/etc.` absent from function bodies | PASS | Full |
| R-02 | SR-10 regression — collaborative preset diverges from ConfidenceParams::default() | `collaborative_preset_equals_default_confidence_params` (exact comment present), `test_confidence_params_default_values` | PASS | Full |
| R-03 | Preset weight sum invariant violated | `test_custom_weights_sum_0_92_passes`, `test_custom_weights_sum_0_95_aborts`, `test_custom_weights_sum_0_91_aborts`, `test_custom_weights_sum_0_920000001_aborts`, `test_custom_weights_sum_0_919999999_aborts` | PASS | Full |
| R-04 | SR-05 rename partial — context_retrospective survives in non-Rust files | Grep gate (zero matches outside excluded dirs); `test_protocol.py` line 55 asserts `context_cycle_review`; tools suite 68/68 pass with renamed method; `client.py` updated | PASS | Full |
| R-05 | Custom preset missing-field — server does not abort | `test_custom_preset_both_fields_present_succeeds`, `test_custom_preset_missing_weights_aborts`, `test_custom_preset_missing_half_life_aborts`, `test_custom_preset_both_absent_returns_missing_weights` | PASS | Full |
| R-06 | freshness_half_life_hours precedence chain wrong | `test_freshness_precedence_named_preset_no_override`, `test_freshness_precedence_named_preset_with_override`, `test_freshness_precedence_custom_no_half_life_aborts`, `test_freshness_precedence_custom_with_half_life_succeeds`, `test_freshness_precedence_collaborative_override_applies` | PASS | Full |
| R-07 | [server] instructions injection bypass | `test_instructions_injection_aborts`, `test_instructions_8192_bytes_passes`, `test_instructions_8193_bytes_aborts_before_scan`, `test_instructions_valid_multiline_passes`; security suite 17/17 | PASS | Full |
| R-08 | [confidence] weights silently active for named presets | `test_named_preset_ignores_confidence_weights` | PASS | Full |
| R-09 | Weight sum validation uses wrong invariant (sum <= 1.0) | `test_custom_weights_sum_0_95_aborts` (critical regression detector); `sum <= 1.0` grep in config.rs returns comments only — not code | PASS | Full |
| R-10 | Cross-level custom preset weight inheritance | `test_merge_cross_level_custom_weights_prohibited`, `test_merge_cross_level_no_global_weights_still_aborts`, `test_merge_cross_level_both_custom_per_project_wins` | PASS | Full |
| R-11 | [agents] session_capabilities Admin privilege escalation | `test_session_capabilities_admin_aborts`, `test_session_capabilities_admin_mixed_aborts`, `test_session_capabilities_admin_lowercase_behavior`, `test_session_capabilities_valid_permissive_set_passes` | PASS | Full |
| R-12 | freshness_half_life_hours validation gap (NaN, Inf, 0.0) | `test_half_life_zero_aborts`, `test_half_life_negative_aborts`, `test_half_life_nan_aborts`, `test_half_life_infinity_aborts`, `test_half_life_negative_zero_aborts`, `test_half_life_87600_0_passes`, `test_half_life_87600_001_aborts`, `test_half_life_min_positive_passes` | PASS | Full |
| R-13 | ContentScanner::global() not warmed before validate_config() | Code audit: `ContentScanner::global()` warm call at top of `load_config` with ordering-invariant comment; security suite 17/17 with injection detection exercised | PASS | Full |
| R-14 | AgentRegistry session_caps not propagated | `test_merge_configs_per_project_wins_for_specified_fields`; lifecycle suite `test_agent_auto_enrollment` 23/23 | PASS | Partial |
| R-15 | dirs::home_dir() returning None panics | `test_load_config_file_too_large_aborts` (degrades path tested); `load_config` code audit: None branch emits warn and returns `UnimatrixConfig::default()` | PASS | Partial |
| R-16 | File size cap bypassed via toml::from_reader | `test_load_config_file_too_large_aborts`, `test_load_config_file_exactly_64kb_passes`; code audit: uses `Vec<u8>` buffer + len check before `from_str` | PASS | Full |
| R-17 | CategoryAllowlist::new() behavior changes | `test_empty_categories_documented_behavior`; existing tests use `new()` and pass (1438 unit tests pass) | PASS | Full |
| R-18 | from_preset(Custom) called directly — panic by design | Code audit: `confidence_params_from_preset` panics on `Preset::Custom` with explicit panic message; no direct call with `Custom` outside `resolve_confidence_params` | PASS | Full |
| R-19 | boosted_categories subset validation absent | `test_boosted_category_not_in_allowlist_aborts` | PASS | Full |
| R-20 | Hook/bridge path accidentally loads config | Code audit: `Command::Hook` and `tokio_main_bridge` paths do not call `load_config` | PASS | Full |
| R-21 | World-writable abort message does not identify file | `test_display_world_writable`; `test_check_permissions_world_writable_aborts` includes path; all `Display` impls include path | PASS | Full |
| R-22 | Merge false-negative: per-project field = compiled default treated as absent | `test_merge_configs_per_project_wins_for_specified_fields`, `test_merge_configs_list_replace_not_append`; `Option<f64>` type for `freshness_half_life_hours` prevents false-negative (None = absent, Some(v) = present regardless of value) | PASS | Full |
| IR-01 | unimatrix-engine API change — full call site sweep | `cargo test --workspace`: 1438+868 unit tests pass with zero new failures; 10 pre-existing pool timeout failures (GH#303) unchanged | PASS | Full |
| IR-02 | agent_resolve_or_enroll third parameter — behavior risk | Code audit: all existing call sites pass `None`; server-infra wrapper passes configured caps when present; lifecycle `test_agent_auto_enrollment` passes | PASS | Partial |
| IR-03 | SearchService — all four hardcoded comparisons replaced | `grep '"lesson-learned"' search.rs` → doc comment only (line 112); search suite via smoke + lifecycle pass | PASS | Full |
| IR-04 | Background tick receives stale ConfidenceParams | Code audit: `Arc<ConfidenceParams>` resolved after config load, before tick spawn; lifecycle `test_empirical_prior_flows_to_stored_confidence` validates empirical preset flows through | PASS | Full |
| IR-05 | CategoryAllowlist::from_categories vs new() delegation | `test_empty_categories_documented_behavior`; `new()` calls `from_categories(INITIAL_CATEGORIES.to_vec())` confirmed by code audit | PASS | Full |
| EC-01 | Empty categories list | `test_empty_categories_documented_behavior` — behavior documented: empty list accepted (no minimum enforced), all future store calls fail category check | PASS | Full |
| EC-02 | Empty boosted_categories HashSet no panic | Implicitly covered: `boosted_categories = []` produces empty `HashSet`, search re-ranking skips boost iteration; smoke + lifecycle pass | PASS | Partial |
| EC-03 | Zero weight in custom (valid) | `test_custom_weights_sum_0_92_passes` exercises full valid custom path; zero individual weight not prohibited by spec | PASS | Partial |
| EC-04 | IEEE boundary values for half_life | `test_half_life_87600_0_passes` (inclusive upper bound), `test_half_life_87600_001_aborts`, `test_half_life_negative_zero_aborts`, `test_half_life_nan_aborts`, `test_half_life_infinity_aborts`, `test_half_life_min_positive_passes` | PASS | Full |
| EC-05 | Empty per-project config file = defaults | `test_empty_per_project_file_produces_defaults` | PASS | Full |
| EC-06 | 65536-byte boundary inclusive | `test_load_config_file_too_large_aborts` (65537 fails), `test_load_config_file_exactly_64kb_passes` (65536 passes) | PASS | Full |
| EC-07 | Symlink to world-writable target aborts startup | `test_check_permissions_symlink_to_world_writable_aborts` (unix only) | PASS | Full |
| EC-08 | Duplicate session_capabilities behavior | `test_session_capabilities_valid_permissive_set_passes`; code stores as `Vec<String>`, duplicates pass validation | PASS | Partial |
| SR-SEC-01 | [server] instructions universal prompt injection | `test_instructions_injection_aborts`, `test_instructions_8193_bytes_aborts_before_scan` (length-before-scan ordering verified) | PASS | Full |
| SR-SEC-02 | [agents] session_capabilities Admin exclusion | `test_session_capabilities_admin_aborts`, `test_session_capabilities_admin_mixed_aborts` | PASS | Full |
| SR-SEC-03 | [knowledge] categories schema gate | `test_category_invalid_char_aborts`, `test_category_too_long_aborts`, `test_category_count_exceeds_64_aborts` | PASS | Full |
| SR-SEC-04 | Config file symlink / TOCTOU | `test_check_permissions_symlink_to_world_writable_aborts`; code audit: uses `metadata()` not `symlink_metadata()` | PASS | Full |
| SR-SEC-05 | Config file size cap and TOML parser memory DoS | `test_load_config_file_too_large_aborts`, `test_load_config_file_exactly_64kb_passes`; code audit: `Vec<u8>` buffer checked before `from_str` | PASS | Full |

---

## Mandatory Pre-PR Gates

| Gate | Command | Result |
|------|---------|--------|
| SR-10 test present with exact comment | `grep "fix the weight table, not the test" crates/` | PASS — line 1019 of config.rs |
| context_retrospective eradication (Rust/py/skill/config) | `grep -rn "context_retrospective" . --include="*.rs" --include="*.py" --include="*.toml" --include="*.md"` (excl. git, features/, research/) | PASS — zero matches |
| lesson-learned literal removed from search.rs boost logic | `grep '"lesson-learned"' crates/unimatrix-server/src/services/search.rs` | PASS — doc comment only (line 112) |
| Weight sum invariant correct (no sum <= 1.0 in code) | `grep 'sum <= 1.0' crates/unimatrix-server/src/infra/config.rs` | PASS — appears in comments only |
| All four AC-25 freshness precedence cases present | Named test functions in config.rs | PASS — five tests including collaborative-override |

---

## Test Results

### Unit Tests

| Crate | Passed | Failed | Notes |
|-------|--------|--------|-------|
| unimatrix-store | 47 | 0 | |
| unimatrix-vector | 12 | 0 | |
| unimatrix-embed | 76 | 0 | 18 ignored (feature-gated) |
| unimatrix-engine | 270 | 0 | |
| unimatrix-observe | ~40 | 0 | (multiple result lines) |
| unimatrix-server | 1438 | 10 | 10 pre-existing pool timeout failures (GH#303) |
| **Total** | **~1906** | **10** | **All failures pre-existing (GH#303)** |

Pre-existing failures (GH#303, not caused by dsn-001):
- `import::tests::test_hash_validation_empty_both`
- `import::tests::test_hash_validation_empty_previous_hash`
- `import::tests::test_hash_validation_empty_title_edge_case`
- `import::tests::test_hash_validation_valid_chain`
- `import::tests::test_header_only_file`
- `import::tests::test_sql_injection_in_content`
- `import::tests::test_sql_injection_in_title`
- `mcp::identity::tests::test_resolve_anonymous`
- `mcp::identity::tests::test_resolve_known_agent`
- `mcp::identity::tests::test_resolve_unknown_agent`

All 10 are SQLite connection pool timeouts under concurrent test load. No dsn-001 regression.

### Integration Tests (infra-001)

| Suite | Total | Passed | Failed | XFail | Notes |
|-------|-------|--------|--------|-------|-------|
| `smoke` | 20 | 19 | 0 | 1 | xfail: GH#111 volume test (pre-existing) |
| `protocol` | 13 | 13 | 0 | 0 | All pass including tool rename AC-13 |
| `tools` | 73 | 68 | 0 | 5 | xfail: GH#305 (baseline, pre-existing), GH#238 (multi-agent), others pre-existing |
| `security` | 17 | 17 | 0 | 0 | ContentScanner, capability enforcement |
| `lifecycle` | 25 | 23 | 0 | 2 | xfail: GH#238, auto_quarantine env-driven |
| **Total** | **148** | **140** | **0** | **8** | **All xfails pre-existing; zero new failures** |

No integration tests were deleted, commented out, or newly marked xfail.

---

## Gaps

### Partial Coverage Items (documented, not blocking)

**R-14 (session_caps propagation) — Partial**: The end-to-end path from config-loaded `session_caps` through `AgentRegistry::resolve_or_enroll()` to `store.agent_resolve_or_enroll()` with `Some(caps)` is covered by code audit and the `test_agent_auto_enrollment` lifecycle test. A dedicated integration test requiring a config-injection harness fixture (AC-06) was not added. The test-plan OVERVIEW.md identifies this as a known harness gap requiring a `config_server` fixture. Unit-level coverage is full; MCP-level end-to-end is partial. GH Issue for harness enhancement: not yet filed — recommended as follow-up for W3-1 or harness sprint.

**R-15 (dirs::home_dir() None) — Partial**: Code audit confirms the None branch degrades gracefully with `tracing::warn!` and returns `UnimatrixConfig::default()`. No dedicated unit test exercises this path directly (requires mocking `dirs::home_dir()`). Acceptable given code audit evidence.

**EC-02 (empty boosted_categories) — Partial**: Empty HashSet in SearchService produces correct behavior (no boost applied) and does not panic. Covered implicitly by smoke + lifecycle passing but no dedicated test for the empty-set case.

**EC-03 (zero individual weight in custom) — Partial**: Zero weight in a single field is not explicitly tested. The sum invariant tests verify the overall constraint; individual zero weights satisfy the `[0.0, 1.0]` spec range check.

**EC-08 (duplicate session_capabilities) — Partial**: Duplicates are stored as-is in `Vec<String>`; validation checks each element against the allowlist. Behavior is documented but no dedicated test for the duplicate case.

**AC-05 / AC-06 / AC-07 (config-injection integration tests) — Partial**: These ACs require the harness to start the server with a custom config file. The current `infra-001` harness has no `config_server` fixture. Unit-level coverage via `validate_config()` and `resolve_confidence_params()` is full. The MCP-level verification (server instructions in `initialize` response, strict session_caps in enrollment response) is a documented gap.

No risks from RISK-TEST-STRATEGY.md are uncovered. All partial items are documented limitations of test depth, not missing risk coverage.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `cargo test --workspace` zero new failures with no config present; all existing tests use `Default::default()` |
| AC-02 | PASS | `CategoryAllowlist::from_categories()` unit tests in categories.rs; `new()` delegates to `from_categories(INITIAL_CATEGORIES)` |
| AC-03 | PASS | `grep '"lesson-learned"' search.rs` → doc comment only; `boosted_categories: HashSet<String>` field used in all four comparison sites |
| AC-04 | PASS | `test_freshness_score_configurable_half_life`; `test_freshness_score_uses_params_half_life` (ratio matches expected decay) |
| AC-05 | PARTIAL | Unit: `ServerConfig { instructions: Some(...) }` struct verified. MCP-level: harness config-injection fixture gap (see Gaps section) |
| AC-06 | PARTIAL | Unit: `AgentsConfig { default_trust, session_capabilities }` verified. MCP-level: harness config-injection fixture gap |
| AC-07 | PARTIAL | `test_merge_configs_per_project_wins_for_specified_fields`, `test_merge_configs_list_replace_not_append`. MCP-level: harness config-injection fixture gap |
| AC-08 | PASS | `test_check_permissions_world_writable_aborts` (unix, `#[cfg(unix)]`) |
| AC-09 | PASS | `test_check_permissions_group_writable_returns_ok` (unix, `#[cfg(unix)]`) |
| AC-10 | PASS | `test_category_invalid_char_aborts`, `test_category_too_long_aborts`, `test_category_count_exceeds_64_aborts` |
| AC-11 | PASS | `test_boosted_category_not_in_allowlist_aborts`; error message contains invalid value |
| AC-12 | PASS | `test_instructions_injection_aborts` with known ContentScanner injection pattern |
| AC-13 | PASS | grep gate: zero `context_retrospective` matches; `test_protocol.py` line 55: `"context_cycle_review"`; tools suite 68/68 pass with renamed method; `client.py` line 629: `context_cycle_review` |
| AC-14 | PASS | `CycleParams.topic` doc in `mcp/tools.rs` references domain-agnostic examples (feature, incident, campaign, case, experiment) per implementation |
| AC-15 | PASS | `test_load_config_file_too_large_aborts` (65537 bytes rejects before parse); `test_load_config_file_exactly_64kb_passes` (65536 passes) |
| AC-16 | PASS | `test_half_life_zero_aborts`, `test_half_life_negative_aborts`, `test_half_life_nan_aborts`, `test_half_life_infinity_aborts`, `test_half_life_negative_zero_aborts` |
| AC-17 | PASS | `test_half_life_87600_001_aborts`; `test_half_life_87600_0_passes` (inclusive boundary) |
| AC-18 | PASS | `test_invalid_default_trust_aborts`; error message lists both `"permissive"` and `"strict"` |
| AC-19 | PASS | `test_session_capabilities_admin_aborts`, `test_session_capabilities_admin_mixed_aborts`, `test_session_capabilities_admin_lowercase_behavior`, `test_session_capabilities_valid_permissive_set_passes` |
| AC-20 | PASS | `test_instructions_8193_bytes_aborts_before_scan` (9000-byte injection string returns `InstructionsTooLong`, not `InstructionsInjection`); `test_instructions_8192_bytes_passes` |
| AC-21 | PASS | `collaborative_preset_equals_default_confidence_params` present with exact comment "SR-10: If this test fails, fix the weight table, not the test." at line 1019 |
| AC-22 | PASS | `resolve_confidence_params(&UnimatrixConfig::default())` returns `ConfidenceParams::default()` — covered by SR-10 + `test_freshness_precedence_named_preset_no_override` using default config |
| AC-23 | PASS | `test_named_preset_ignores_confidence_weights`; `test_confidence_params_default_values` checks exact field values; all four named presets verified in weight tests |
| AC-24 | PASS | `test_custom_preset_missing_weights_aborts`, `test_custom_preset_both_absent_returns_missing_weights` |
| AC-25 | PASS | All four named precedence tests present: `test_freshness_precedence_named_preset_no_override`, `test_freshness_precedence_named_preset_with_override`, `test_freshness_precedence_custom_no_half_life_aborts`, `test_freshness_precedence_custom_with_half_life_succeeds` + `test_freshness_precedence_collaborative_override_applies` |
| AC-26 | PASS | `test_unrecognised_preset_serde_error` — serde rejects unknown string before `validate_config` |
| AC-27 | PASS | `test_confidence_params_has_nine_fields` (exactly 9 public fields); `test_confidence_params_default_values` (all nine non-zero for collaborative); all four named presets yield non-zero fields per weight table |

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` (category: "procedure") for gate verification steps and integration test triage — returned #487 "How to run workspace tests without hanging" and #553 worktree isolation; neither directly applicable to dsn-001 test execution. No novel procedure gaps discovered.
- Stored: nothing novel to store — test patterns for config validation (validate_config standalone testability, SR-10 mandatory invariant comment) are feature-specific to dsn-001. The pattern "test the wrong invariant explicitly (sum=0.95 to detect sum<=1.0 implementations)" is worth storing if this class of spec/ADR discrepancy recurs.
