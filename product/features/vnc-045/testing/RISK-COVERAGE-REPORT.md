# Risk Coverage Report: vnc-045 — `context_tag` (mechanism only)

> Stage 3c execution. Scope REDUCED — `protected_tags` DEFERRED (VOIDED-BY-DEFERRAL risks carry no test obligation). Seam split per #5468: the `#[tool]` handler is not unit-constructible, so orchestration + audit proofs land at the `StoreTagService` + store-primitive + `audit_log` read-back seams; route/format proofs land in the infra-001 integration suite.
>
> **Verdict: PASS.** Workspace unit gate 6961 passed / 0 failed. Integration smoke gate PASS (32/32). Every risk R-01..R-08 has passing coverage. No xfails, no GH issues filed (no failures — feature-caused or pre-existing).

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Forbidden-surface mutation / stale read (invariance + read-freshness) | UNIT `test_add_tag_preserves_learning_columns`, `test_add_tag_preserves_hash_chain`, `test_add_tag_preserves_id_and_edges`, `test_remove_tag_invariance`, `test_replace_tag_invariance`, `test_tag_read_freshness` (store); INTEG `test_context_tag_add_then_search_reflects`, `test_context_tag_remove_then_search_absent` (lifecycle) | PASS | Full |
| R-02 | Non-atomic replace loses status; colon-less degrade | UNIT `test_replace_tag_rollback_on_insert_failure`, `test_replace_tag_one_transaction_atomic`, `test_replace_tag_single_value_evicts_prior`, `test_replace_tag_colon_less_degrades_to_add`, `test_replace_tag_no_prior_in_namespace` (store); `test_replace_colon_less_degrades_to_add` (service); INTEG `test_context_tag_replace_single_value`, `test_context_tag_replace_colon_less_degrades_to_add` (tools) | PASS | Full |
| R-03 | Audit record incomplete or lost (PRIMARY, retrofit-hard control) | UNIT `test_audit_prior_value_mandatory_on_remove`, `..._on_replace`, `..._null_on_add`, `..._on_remove_absent_tag`, `test_audit_metadata_never_sentinel`, `test_audit_exactly_one_event_per_mutation`, `test_audit_namespace_derived_recorded_never_validated`, `test_audit_action_is_variant_string_not_integer`, `test_audit_session_id_captured_before_spawn`, `test_audit_field_completeness`, `test_build_tag_metadata_emits_explicit_nulls`, `test_build_tag_metadata_valid_json_never_sentinel` (service/`audit_log` read-back) | PASS | Full (unit seam only — see Gaps) |
| R-04 | Value-opacity violated / accidental validator | UNIT `test_value_opaque_acceptance_table` (service); INTEG `test_context_tag_value_opaque_freeform_accepted` (tools); STATIC grep — no `ProtectedTagsConfig`/validator shipped (see below) | PASS | Full |
| R-05 | Lifecycle guards under/over-applied | UNIT `test_check_tag_lifecycle_quarantined_refused`, `..._deprecated_allowed`, `..._active_allowed` (handler seam); INTEG `test_context_tag_quarantined_entry_refused` (security), `test_context_tag_deprecated_entry_allowed` (lifecycle) | PASS | Full |
| R-06 | Namespace derivation / tag-parse edge cases | UNIT `test_derive_namespace_standard`, `..._colon_terminated`, `..._colon_less`, `..._multi_colon`, `..._mid_string_colon`, `..._leading_colon`, `..._empty` (handler seam); INTEG `test_context_tag_empty_tag_rejected`, `test_context_tag_invalid_action_rejected` (tools) | PASS | Full |
| R-07 | Live-control wiring missed (throttle + op-list) | UNIT `test_check_write_rate_throttles_before_write`, `test_uds_session_exempt_from_throttle` (service); `test_audit_write_count_includes_context_tag`, `test_audit_write_count_context_tag_since_boundary`, `test_audit_write_count_excludes_non_write_ops` (audit op-list) | PASS | Full |
| R-08 | Injection / over-broad DELETE (SQL metachar / LIKE) | UNIT `test_add_tag_sql_metachar_stored_literally`, `test_replace_tag_like_percent_namespace_no_over_match`, `test_replace_tag_like_underscore_namespace_no_over_match` (store); INTEG `test_context_tag_sql_metachar_tag_stored_literally` (security) | PASS | Full |

R-04 static proof (grep over the vnc-045 diff): no `ProtectedTagsConfig`, `ProtectedTagRule`, `TagDisposition`, `evaluate_protected_tag`, allow-list, or vocabulary type shipped; the pre-write interception point is a marked comment only (RETROFIT SEAM #1, `tools.rs:1609`); `context_tag` does NOT invoke `validate_outcome_tags`. Confirmed against the implemented handler.

## Test Results

### Unit Tests (workspace, hardened `cargo test --workspace`)
- Total: 6961 passed; 0 failed; 31 ignored (pre-existing, unrelated)
- vnc-045-specific unit/seam tests: **47** — 17 store-primitive (`write_tag_tests.rs`, R-01/R-02/R-08 + edge cases incl. cascade-delete) + 17 `StoreTagService`/`audit_log` read-back (`store_tag_tests.rs`, R-03/R-04/R-05/R-07) + 10 handler-seam helpers (`derive_namespace` ×7 + `check_tag_lifecycle` ×3, R-06/R-05) + 3 audit op-list (`audit_count_tests.rs`, R-07). All green.
- Full-workspace LINK smoke (#878 guard, `check-workspace-link-smoke.sh`): PASS (link holds at configured parallelism).

### Integration Tests (infra-001, over the `unimatrix` 0.10.0 release binary)
16 context_tag route/format assertions added (15 new test functions + 1 updated tool-count guard). No new suite file — existing suites extended (test infra is cumulative). All executed runs green:

| Run | Command | Result |
|-----|---------|--------|
| Smoke gate (MANDATORY) | `pytest suites/ -m smoke` | 32 passed / 0 failed (incl. new `test_context_tag_add_roundtrip`, `test_context_tag_requires_write_capability`) |
| Protocol (full) | `pytest suites/test_protocol.py` | 14 passed (incl. `test_list_tools_returns_fifteen`, `test_context_tag_in_tool_list`) |
| Security (full) | `pytest suites/test_security.py` | 23 passed (incl. 3 new context_tag: Write-gate, quarantine-refusal, R-08 metachar) |
| Tools — context_tag | `pytest suites/test_tools.py -k context_tag` | 7 passed |
| Lifecycle — context_tag | `pytest suites/test_lifecycle.py -k context_tag` | 4 passed |

New context_tag integration tests by suite: protocol 2 (1 new + 1 updated count-guard), tools 7, lifecycle 4, security 3.

- xfail markers added: **0** (no failures encountered).
- GH issues filed: **0** (no feature-caused or pre-existing failure surfaced).
- Integration tests deleted / commented out: **0**.

## Gaps

- **R-03 audit-record completeness is unit-seam-only (KNOWN, by design).** `audit_log` is not exposed through any MCP tool, so `prior_value` mandatoriness, non-`{}` metadata, single-event-per-mutation, variant-string serde, and `session_id`-before-`spawn` are proven exclusively at the `StoreTagService` + `audit_log` raw-`SELECT` read-back seam (`store_tag_tests.rs`, 12 tests). The integration suite confirms only that the route accepts the call and the mutation is read-back-visible; it does NOT and cannot assert audit shape. This matches the Stage-3a OVERVIEW integration plan ("Gap the harness cannot cover") — no integration assertion was fabricated for it.
- No other uncovered risks. VOIDED-BY-DEFERRAL scope risks (SR-03/04/06/08/09/10) carry no test obligation and correctly have no tests.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | INTEG `test_context_tag_in_tool_list` + `test_list_tools_returns_fifteen` (registered, Write-gated route); UNIT `test_add_tag_preserves_hash_chain`, `test_add_tag_preserves_id_and_edges` (content_hash/previous_hash/edges byte-identical, no supersession id) |
| AC-02 | PASS | UNIT `test_add_tag_preserves_learning_columns`, `test_tag_read_freshness`; INTEG `test_context_tag_add_then_search_reflects` / `test_context_tag_remove_then_search_absent` (five learning columns unchanged; live-SQL read-freshness, no invalidation) |
| AC-03 | PASS | UNIT `test_replace_tag_single_value_evicts_prior`, `test_replace_tag_rollback_on_insert_failure`, `test_replace_tag_colon_less_degrades_to_add`; INTEG `test_context_tag_replace_single_value` (one-tx evict-prior; rollback leaves prior; colon-less degrades to add) |
| AC-04 | PASS | UNIT R-03 read-back suite (12 tests) — full field set, `namespace` derived-never-validated, `prior_value` mandatory on remove/replace + null on add, never `"{}"`, exactly one event, variant-string serde, `session_id` before spawn |
| AC-05 | PASS | UNIT `test_value_opaque_acceptance_table`; INTEG `test_context_tag_value_opaque_freeform_accepted`; STATIC grep no validator/config type shipped, single marked seam, `validate_outcome_tags` not invoked. No rejection-path test written (none ships) |
| AC-06 | PASS | UNIT `test_check_write_rate_throttles_before_write`, `test_uds_session_exempt_from_throttle`; `test_audit_write_count_includes_context_tag` (+ boundary + excludes-non-write) — throttle enforced (UdsSession-exempt); `'context_tag'` counted by `audit_write_count_since` |
| AC-07 | PASS | UNIT `test_check_tag_lifecycle_quarantined_refused` / `..._deprecated_allowed` / `..._active_allowed`; INTEG `test_context_tag_quarantined_entry_refused` (all 3 actions), `test_context_tag_deprecated_entry_allowed` |

## Knowledge Stewardship
- Queried: `context_briefing` (task: Stage 3c context_tag test execution) — surfaced #5389 (extract `#[tool]` decision logic into `pub(crate)` seam fns — the non-constructibility workaround this feature's helper seams use), #317/#296 (MCP handler context-building + transport-agnostic service extraction), #4357 (RequestContext/session-id capture). Applied as the seam-split rationale confirmation; no divergence from the Stage-3a plan.
- Stored: nothing novel. The reusable patterns exercised (non-constructible-handler → seam-fn + `audit_log` read-back with settle; new-MCP-tool integration = client method + protocol count-guard + per-suite extension) are already captured (#5389, crt-058 precedent). No new cross-feature test pattern surfaced. Per stewardship rules, feature-specific assertions were not stored.
