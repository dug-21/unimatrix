# Risk Coverage Report: vnc-012

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | `#[serde(default)]` missing — absent fields error instead of `None` | AC-03-ABSENT-ID, AC-03-ABSENT-LIMIT, AC-04-ABSENT, AC-05-ABSENT, AC-06-ABSENT (5 tests in tools.rs vnc012_coercion_tests) | PASS | Full |
| R-02 | rmcp dispatch path not tested — unit tests miss `Parameters<T>` delegation | `test_get_params_string_id_coercion` (AC-13 via `from_value`), IT-01 `test_get_with_string_id`, IT-02 `test_deprecate_with_string_id` | PASS | Full |
| R-03 | JSON null → `Some(0)` or error instead of `None` | AC-03-NULL-ID, AC-03-NULL-LIMIT, AC-04-NULL, AC-05-NULL, AC-06-NULL + `test_deserialize_opt_i64_null_input`, `test_deserialize_opt_usize_null_input` | PASS | Full |
| R-04 | `usize` truncation from `as usize` on 32-bit targets | `test_retro_params_negative_evidence_limit_is_err` (AC-09), `test_retro_params_zero_evidence_limit` (AC-06-ZERO), `test_deserialize_opt_usize_u64_overflow_string`, `test_deserialize_opt_usize_negative_string` | PASS | Full |
| R-05 | `#[schemars(with)]` typo emits empty schema `{}` | `test_schema_integer_type_preserved_for_all_nine_fields` (AC-10) | PASS | Full |
| R-06 | Float JSON Numbers not handled by `visit_f64` — panic or wrong error | `test_get_params_float_number_is_err`, `test_search_params_float_number_k_is_err`, `test_deserialize_i64_float_number`, `test_deserialize_opt_i64_float_number`, `test_deserialize_opt_usize_float_number` (AC-09-FLOAT-NUMBER) | PASS | Full |
| R-07 | `deserialize_with` path string literal not compiler-validated | `cargo build --release` success — all 9 attribute path strings resolve at macro-expansion time | PASS | Full (build-time) |
| R-08 | Non-numeric string silently coerces to zero | `test_get_params_nonnumeric_id_is_err`, `test_deprecate_params_nonnumeric_id_is_err`, `test_quarantine_params_nonnumeric_id_is_err`, `test_correct_params_nonnumeric_original_id_is_err` (AC-08); `test_lookup_params_nonnumeric_id_is_err`, `test_lookup_params_nonnumeric_limit_is_err`, `test_search_params_nonnumeric_k_is_err`, `test_briefing_params_nonnumeric_max_tokens_is_err`, `test_retro_params_nonnumeric_evidence_limit_is_err` (AC-08-OPT) | PASS | Full |
| R-09 | `make_server()` not accessible in test context for schema snapshot | `test_schema_integer_type_preserved_for_all_nine_fields` uses `make_server()` via `#[cfg(test)]` in `server.rs` — confirmed accessible | PASS | Full |
| R-10 | Existing `test_retrospective_params_evidence_limit` regression | `cargo test --workspace` — all 2455 server lib tests pass; no regression | PASS | Full |

---

## Test Results

### Unit Tests

All tests run via `cargo test --workspace`.

- Total workspace tests passed: **4,056**
- Failed: **0** (feature-caused)
- Pre-existing failures: **3** (col018 listener tests — see GH #452; flaky due to Tokio runtime shutdown timing, not caused by vnc-012)

#### vnc-012 Feature Tests Breakdown

| Module | Test Count | Result |
|--------|-----------|--------|
| `mcp::serde_util::tests` (serde_util.rs) | 33 | 33 passed |
| `mcp::tools::vnc012_coercion_tests` (tools.rs) | 42 | 42 passed |
| `server::tests::test_schema_integer_type_preserved_for_all_nine_fields` | 1 | 1 passed |
| **vnc-012 total** | **76** | **76 passed** |

unimatrix-server lib total: 2455 passed (includes all prior tests, no regression).

#### Clippy Status

- `cargo clippy -p unimatrix-server --no-deps -- -D warnings`: multiple errors present in files NOT touched by this feature (`server.rs` dead fields, `eval/` unused imports, `background.rs`, `bridge.rs` collapsible-if). These are pre-existing. The unimatrix-engine clippy failure (`collapsible_if` in `auth.rs`) is also pre-existing.
- No new clippy warnings introduced by this feature's changes to `serde_util.rs`, `tools.rs` (new tests), `mod.rs`, or `server.rs` (AC-10 test).

### Integration Tests

#### Smoke Suite (`-m smoke`) — Mandatory Gate

- Total smoke tests: **22**
- Passed: **22**
- Failed: **0**
- IT-01 (`test_get_with_string_id`): PASS (after fixing test assertion — see below)
- IT-02 (`test_deprecate_with_string_id`): PASS

**Test fix applied**: IT-01 had an incorrect content assertion. The test called `server.call_tool("context_get", {"id": string_id, "agent_id": "human"})` without `"format": "json"`, which caused `context_get` to return a summary/index-table format. The assertion `"IT-01 string id coercion test content" in text` failed because the summary format does not include the entry content body. Fix: added `"format": "json"` to the call_tool arguments and changed assertion from `get_result_text(get_resp)` to `parse_entry(get_resp)["content"]`. This is a test assertion bug (not a feature code bug) — the coercion itself was working (`assert_tool_success` passed before the content check).

#### Protocol Suite

- Total: **13**
- Passed: **13**
- Failed: **0**

#### Security Suite

- Total: **19**
- Passed: **19**
- Failed: **0**
- No coercion of security-relevant non-numeric fields (agent_id, format, category, status) was introduced.

#### Tools Suite

- Total: **98**
- Passed: **96**
- xfailed: **2** (pre-existing, unrelated to vnc-012)
- Failed: **0**

#### Integration Test Total

| Suite | Run | Passed | xfailed | Failed |
|-------|-----|--------|---------|--------|
| smoke | 22 | 22 | 0 | 0 |
| protocol | 13 | 13 | 0 | 0 |
| security | 19 | 19 | 0 | 0 |
| tools | 98 | 96 | 2 | 0 |
| **Total** | **152** | **150** | **2** | **0** |

---

## Gaps

None. All risks from RISK-TEST-STRATEGY.md have test coverage.

**Pre-existing issues documented (not gaps in vnc-012 coverage):**

1. **GH #452** — `col018_long_prompt_truncated`, `col018_prompt_at_limit_not_truncated`, `col018_topic_signal_null_for_generic_prompt` in `uds/listener.rs` fail intermittently with `assertion left == right failed (0 vs 1)` and Tokio runtime shutdown timing errors. Pre-existing on `main`, not caused by vnc-012. Marked as pre-existing in GH #452.

2. **Clippy errors** (pre-existing in unimatrix-server): Dead fields/methods across multiple files (`server.rs`, `eval/`, `background.rs`, `bridge.rs`, `services/`). Not caused by vnc-012. No clippy errors in the four files added/modified by this feature.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_get_params_string_id`, `test_deprecate_params_string_id_coercion`, `test_quarantine_params_string_id` — all assert `id == 3770i64` from `"3770"` |
| AC-02 | PASS | `test_correct_params_string_original_id` — asserts `original_id == 3770i64` from `{"original_id": "3770", "content": "c"}` |
| AC-03 | PASS | `test_lookup_params_string_id` (id), `test_lookup_params_string_limit` (limit) |
| AC-03-ABSENT-ID | PASS | `test_lookup_params_absent_id` — `params.id.is_none()` on `{}` |
| AC-03-ABSENT-LIMIT | PASS | `test_lookup_params_absent_limit` — `params.limit.is_none()` on `{}` |
| AC-03-NULL-ID | PASS | `test_lookup_params_null_id` — `params.id.is_none()` on `{"id": null}` |
| AC-03-NULL-LIMIT | PASS | `test_lookup_params_null_limit` — `params.limit.is_none()` on `{"limit": null}` |
| AC-04 | PASS | `test_search_params_string_k` — `k == Some(5i64)` |
| AC-04-ABSENT | PASS | `test_search_params_absent_k` — `k.is_none()` on `{"query": "q"}` |
| AC-04-NULL | PASS | `test_search_params_null_k` — `k.is_none()` on `{"query": "q", "k": null}` |
| AC-05 | PASS | `test_briefing_params_string_max_tokens` — `max_tokens == Some(3000i64)` |
| AC-05-ABSENT | PASS | `test_briefing_params_absent_max_tokens` — `max_tokens.is_none()` on `{"task": "t"}` |
| AC-05-NULL | PASS | `test_briefing_params_null_max_tokens` — `max_tokens.is_none()` on `{"task": "t", "max_tokens": null}` |
| AC-06 | PASS | `test_retro_params_string_evidence_limit` — `evidence_limit == Some(5usize)` |
| AC-06-ZERO | PASS | `test_retro_params_zero_evidence_limit` — `evidence_limit == Some(0usize)` |
| AC-06-ABSENT | PASS | `test_retro_params_absent_evidence_limit` — `evidence_limit.is_none()` on `{"feature_cycle": "col-001"}` |
| AC-06-NULL | PASS | `test_retro_params_null_evidence_limit` — `evidence_limit.is_none()` on explicit null |
| AC-07 | PASS | `test_get_params_string_and_integer_equal`, `test_get_params_integer_id`, `test_deprecate_params_integer_id`, `test_quarantine_params_integer_id`, `test_correct_params_integer_original_id` |
| AC-08 | PASS | `test_get_params_nonnumeric_id_is_err`, `test_deprecate_params_nonnumeric_id_is_err`, `test_quarantine_params_nonnumeric_id_is_err`, `test_correct_params_nonnumeric_original_id_is_err` — all `is_err()` |
| AC-08-OPT | PASS | `test_lookup_params_nonnumeric_id_is_err`, `test_lookup_params_nonnumeric_limit_is_err`, `test_search_params_nonnumeric_k_is_err`, `test_briefing_params_nonnumeric_max_tokens_is_err`, `test_retro_params_nonnumeric_evidence_limit_is_err` |
| AC-09 | PASS | `test_retro_params_negative_evidence_limit_is_err` — `"-1"` for evidence_limit returns `Err` |
| AC-09-FLOAT | PASS | `test_get_params_float_string_is_err` (required), `test_search_params_float_string_k_is_err` (optional), `test_deserialize_i64_float_string`, `test_deserialize_opt_i64_float_string`, `test_deserialize_opt_usize_float_string` |
| AC-09-FLOAT-NUMBER | PASS | `test_get_params_float_number_is_err`, `test_search_params_float_number_k_is_err`, `test_lookup_params_float_number_id_is_err`, `test_deserialize_i64_float_number`, `test_deserialize_opt_i64_float_number`, `test_deserialize_opt_usize_float_number` — all `is_err()`, none truncate |
| AC-10 | PASS | `test_schema_integer_type_preserved_for_all_nine_fields` — constructs `UnimatrixServer` via `make_server()`, extracts `input_schema` for all 9 affected properties, asserts `"type": "integer"` for each |
| AC-11 | PASS | `cargo test --workspace` — 2455 server lib tests pass; no regression on existing tests including `test_retrospective_params_evidence_limit` |
| AC-12 | PASS | 33 tests in `mcp::serde_util::tests` covering integer input, string input, non-numeric rejection, null for optional, absent for optional — all three helpers covered |
| AC-13 | PASS | `test_get_params_string_id_coercion` and `test_deprecate_params_string_id_coercion` — use `serde_json::from_value::<T>(args)` (the exact rmcp `Parameters<T>: FromContextPart` dispatch path); assert `Ok` and correct field value |
| IT-01 | PASS | `test_get_with_string_id` in infra-001 smoke suite — stores entry, retrieves with string id via stdio transport, asserts success and content match |
| IT-02 | PASS | `test_deprecate_with_string_id` in infra-001 smoke suite — stores entry, deprecates with string id via stdio transport, asserts success |

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — entries #238 (test infrastructure cumulative), #840 (infra-001 quick reference), #1685 (skip stub resolution procedure) returned. Entry #840 confirmed the USAGE-PROTOCOL.md as the definitive reference.
- Stored: entry via `/uni-store-pattern` — "IT test assertion format mismatch: call_tool without format=json returns summary not content" — this is a novel pattern worth capturing for future IT authors.
