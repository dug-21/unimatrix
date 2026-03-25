# Gate 3c Report: col-027 — PostToolUseFailure Hook Support

> Gate: 3c (Final Risk-Based Validation)
> Date: 2026-03-25
> Result: PASS

---

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Risk mitigation proof | PASS | 13/14 risks fully covered; R-12 partial (Low/Low, code correct) |
| Test coverage completeness | PASS | All 32+ required scenarios present across listener.rs, friction.rs, metrics.rs, hook.rs, binary integration |
| Specification compliance | PASS | All 12 ACs met; FR-01 through FR-08 verified |
| Architecture compliance | PASS | Component boundaries, ADRs, and integration surface match ARCHITECTURE.md |
| Knowledge stewardship | PASS | Tester agent report includes Queried and Stored entries |

---

## Detailed Findings

### Risk Mitigation Proof

**Status**: PASS (with one low-priority partial gap carried forward)

All 14 risks from the Risk Register are covered. Evidence from RISK-COVERAGE-REPORT.md and direct
code/test inspection:

| Risk ID | Priority | Test(s) | Result |
|---------|----------|---------|--------|
| R-01 | Critical | `test_extract_error_field_present`, `test_extract_error_field_absent`, `test_extract_error_field_truncation_at_501_chars`, `test_extract_observation_fields_posttoolusefailure_full` | PASS |
| R-02 | Critical | `test_two_site_agreement_balanced_failure_and_post`, `test_two_site_agreement_genuine_imbalance`, `test_two_site_agreement_failure_only_no_post` | PASS |
| R-03 | High | `test_extract_observation_fields_posttoolusefailure_full` (obs.tool.is_some()), `test_extract_observation_fields_posttoolusefailure_tool_absent` | PASS |
| R-04 | High | `test_permission_retries_failure_as_terminal_no_finding`, `test_permission_retries_mixed_post_and_failure_balanced` | PASS |
| R-05 | Med | `build_request_posttoolusefailure_explicit_arm`, `build_request_posttoolusefailure_empty_extra`, `build_request_posttoolusefailure_missing_tool_name` | PASS |
| R-06 | Med | `test_tool_failure_rule_at_threshold_no_finding`, `test_tool_failure_rule_above_threshold_fires` | PASS |
| R-07 | Med | `test_tool_failure_rule_non_claude_code_excluded`, `test_tool_failure_rule_mixed_domains` | PASS |
| R-08 | Med | `build_request_posttoolusefailure_null_extra`, `build_request_posttoolusefailure_null_error`, `build_request_posttoolusefailure_missing_tool_name` + AC-12 binary tests | PASS |
| R-09 | Low | `extract_event_topic_signal_posttoolusefailure` | PASS |
| R-10 | Low | `obs.response_size == None` assertion in `test_extract_observation_fields_posttoolusefailure_full` | PASS |
| R-11 | Med | `test_posttoolusefailure_constant_value` (exact string equality) | PASS |
| R-12 | Low | `saturating_sub` present at `metrics.rs:87`; code correct; no test for `failure_count > pre_count` pathological case | PARTIAL (non-blocking) |
| R-13 | Med | `test_default_rules_contains_tool_failure_hotspot`, `test_default_rules_has_22_rules` | PASS |
| R-14 | High | `.claude/settings.json` now contains `"PostToolUseFailure"` entry with `matcher: "*"` and command `unimatrix hook PostToolUseFailure` — pattern identical to `PreToolUse`/`PostToolUse` (AC-01 fix, rework complete) | PASS |

R-12 (Low/Low): `saturating_sub` is verified in source at `metrics.rs:87`. The absence of a
dedicated underflow boundary test is a minor gap. Not a gate blocker per the risk register priority.

---

### Test Coverage Completeness

**Status**: PASS

All risk scenarios from the Risk-Based Test Strategy are exercised. The integration smoke gate
(infra-001, 20/20) passed. Binary integration tests for AC-12 confirm hook exit-code compliance.

**Test totals (cargo test --workspace, verified this run)**:

| Crate | Passed | Failed | Ignored |
|-------|--------|--------|---------|
| unimatrix-core | 17 | 0 | 0 |
| unimatrix-server | 421 | 0 | 0 |
| unimatrix-observe | 297 | 0 | 0 |
| unimatrix-store | 101 | 0 | 27 |
| All other crates | ~2,758 | 0 | 0 |
| **Total workspace** | **3,594** | **0** | **27** |

NFR-05 baseline (2,185) exceeded. All 27 ignored tests are pre-existing, unrelated to col-027.

Integration smoke gate: 20/20 PASS (confirmed from tester session output).

Binary integration (AC-12):
- `echo '{}' | unimatrix hook PostToolUseFailure` → exit 0
- `echo 'not-json' | unimatrix hook PostToolUseFailure` → exit 0
- `echo '' | unimatrix hook PostToolUseFailure` → exit 0

---

### Specification Compliance

**Status**: PASS

All 12 acceptance criteria verified:

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `PostToolUseFailure` present in `.claude/settings.json` with `matcher: "*"` and command `/workspaces/unimatrix/target/release/unimatrix hook PostToolUseFailure`. Structure identical to `PreToolUse` and `PostToolUse` entries. (Rework from prior gate-3c — now resolved.) |
| AC-02 | PASS | `test_posttoolusefailure_constant_value` passes; `hook_type::POSTTOOLUSEFAILURE == "PostToolUseFailure"` |
| AC-03 | PASS | `test_extract_observation_fields_posttoolusefailure_full` passes; compound assertions: `obs.hook == "PostToolUseFailure"`, `obs.tool.is_some()`, `obs.response_snippet == Some(...)`, `obs.response_size == None` |
| AC-04 | PASS | Same test as AC-03; `obs.hook == "PostToolUseFailure"` explicitly asserted, not normalized |
| AC-05 | PASS | `test_permission_retries_failure_as_terminal_no_finding`: 5 Pre + 0 Post + 5 Failure → findings empty |
| AC-06 | PASS | All pre-existing `PermissionRetriesRule` tests pass; `test_permission_retries_genuine_imbalance_with_failures` passes |
| AC-07 | PASS | `test_two_site_agreement_balanced_failure_and_post` asserts both `compute_universal()` (permission_friction_events == 0) and `PermissionRetriesRule::detect()` (findings empty) on the same observation set |
| AC-08 | PASS | `test_tool_failure_rule_above_threshold_fires`: 4 failures for "Bash" → 1 finding, `rule_name == "tool_failure_hotspot"`, `measured == 4.0`, `threshold == 3.0` |
| AC-09 | PASS | `test_tool_failure_rule_at_threshold_no_finding`: 3 failures for "Read" → findings empty |
| AC-10 | PASS | `make_failure` helper present at `friction.rs:450`; used in AC-05, AC-06, AC-08, AC-09 test bodies |
| AC-11 | PASS | Explicit `hook_type::POSTTOOLUSEFAILURE` arm at `hook.rs:493`; `build_request_posttoolusefailure_explicit_arm` verifies `event.event_type == "PostToolUseFailure"` |
| AC-12 | PASS | Binary exits 0 for empty JSON, malformed JSON, and empty stdin |

All functional requirements (FR-01 through FR-08) and non-functional requirements (NFR-01 through
NFR-06) are satisfied:
- NFR-01 (40ms budget): `PostToolUseFailure` routes to fire-and-forget `RecordEvent`, identical path to existing events.
- NFR-02 (defensive parsing): All field accesses use `Option`-chaining; R-08 tests verify no panic on malformed input.
- NFR-03 (no migration): Verified — no schema changes; `observations.hook` is TEXT, no constraint.
- NFR-04 (test additivity): `make_failure` helper added; existing tests unmodified.
- NFR-05 (test count): 3,594 total > 2,185 baseline.
- NFR-06 (string constant discipline): `pub const POSTTOOLUSEFAILURE` added; no enum created.

---

### Architecture Compliance

**Status**: PASS

All six components from ARCHITECTURE.md are implemented as specified:

- **Component 1** (Hook Registration): `.claude/settings.json` entry matches exact constraint (same command pattern as `PreToolUse`/`PostToolUse`, `matcher: "*"`).
- **Component 2** (Hook Dispatcher `hook.rs`): Explicit `hook_type::POSTTOOLUSEFAILURE` arm at line 493 in `build_request()`; explicit arm in `extract_event_topic_signal()` at line 291. Does not enter rework logic. Payload is `input.extra.clone()`.
- **Component 3** (Core Constants `observation.rs`): `pub const POSTTOOLUSEFAILURE: &str = "PostToolUseFailure"` added. `response_snippet` doc comment updated to include `PostToolUseFailure`.
- **Component 4** (Observation Storage `listener.rs`): Explicit `"PostToolUseFailure"` arm in `extract_observation_fields()`; `extract_error_field()` sibling function added; doc comment uses "500-byte" (corrected from rework). `hook` column stored verbatim as `"PostToolUseFailure"` per ADR-003.
- **Component 5** (Two-Site Differential Fix): `friction.rs` uses `terminal_counts` (widened to include failures); `metrics.rs` widens post-bucket. Both in same delivery — ADR-004 atomicity satisfied.
- **Component 6** (`ToolFailureRule`): Implements `DetectionRule`; name `"tool_failure_hotspot"`; category `Friction`; severity `Warning`; threshold 3 (strictly greater than); `source_domain == "claude-code"` guard; registered in `default_rules()` — rule count now 22 per FR-07.6.

ADR decisions followed:
- ADR-001 (string constants, no enum): Confirmed — `POSTTOOLUSEFAILURE` is `pub const &str`.
- ADR-002 (separate error extractor): Confirmed — `extract_error_field()` is a separate function; `extract_response_fields()` not called for failure events.
- ADR-003 (no normalization): Confirmed — stored value is `"PostToolUseFailure"`, not `"PostToolUse"`.
- ADR-004 (atomic two-site fix): Confirmed — `friction.rs` and `metrics.rs` updated together; integration test asserts both sites.

Detection rule audit (FR-08): All 21 prior rules audited per ARCHITECTURE.md §Detection Rule Audit.
Only `PermissionRetriesRule` and `compute_universal()` used the Pre-Post differential — both fixed.

---

### Knowledge Stewardship Compliance

**Status**: PASS

The tester agent report (tester-report from prior gate-3c run, same agent lineage) contains a
`## Knowledge Stewardship` section with:
- `Queried:` entries present (evidence of procedure/pattern queries before gate execution)
- `Stored: nothing novel to store` with explicit reasons provided

---

## Rework Required

None. The single blocker from the prior gate-3c run (AC-01 / R-14: `PostToolUseFailure` absent
from `.claude/settings.json`) has been resolved. All other checks pass.

---

## Non-Blocking Gap (Carried Forward)

**R-12**: No dedicated unit test for `permission_friction_events` when `failure_count > pre_count`.
The `saturating_sub` at `metrics.rs:87` is correct. A test asserting `permission_friction_events >= 0`
when given 1 Pre + 5 Failure would close this gap. Risk register rating: Low/Low. Not a merge blocker.

---

## Knowledge Stewardship

- Queried: `/uni-knowledge-search` for "gate-3c validation post-rework checklist hook settings.json" — entries consulted included #3479 (two-site atomicity pattern), #247 (hook exit-code contract), #3474 (ADR-002 extract_error_field mitigation).
- Stored: nothing novel to store -- the AC-01 settings.json omission is an implementation error already documented in the prior gate-3c report (not a new pattern). The hook registration pattern is entry #3471. Recurring pattern of settings.json omission after hook code implementation may warrant a lesson — but this is the first instance in this codebase; no cross-feature pattern established yet.
