# Gate 3a Report: col-027

> Gate: 3a (Design Review)
> Date: 2026-03-25
> Result: PASS

## Summary

| Check | Status | Notes |
|-------|--------|-------|
| Architecture alignment | PASS | All 6 components match ARCHITECTURE.md decomposition; wave structure, file targets, ADR references all consistent |
| Specification coverage | PASS | All 12 FRs and 6 NFRs map to explicit pseudocode; no scope additions |
| Risk coverage | PASS | All 14 risks map to test scenarios; critical correctness items verified |
| Interface consistency | PASS | Shared types and function signatures consistent across all pseudocode files |
| Knowledge stewardship compliance | WARN | synthesizer report missing `## Knowledge Stewardship` section |

---

## Detailed Findings

### Architecture Alignment

**Status**: PASS

**Evidence**:

- Component 1 (Hook Registration / `.claude/settings.json`): `hook-registration.md` exactly mirrors architecture — `matcher: "*"`, command `unimatrix hook PostToolUseFailure`, same path pattern as `PreToolUse`/`PostToolUse`. FR-01 covered.

- Component 2 (Hook Dispatcher / `hook.rs`): `hook-dispatcher.md` adds explicit `"PostToolUseFailure"` arm in BOTH `build_request()` and `extract_event_topic_signal()` as required by architecture §Component 2 and ADR-001. No wildcard fallthrough. No call to `extract_response_fields()`. Routes to `HookRequest::RecordEvent`. Does not enter rework logic. Matches integration surface contract: `ImplantEvent { event_type: "PostToolUseFailure", payload: input.extra, topic_signal }`.

- Component 3 (Core Constants / `observation.rs`): `core-constants.md` adds `pub const POSTTOOLUSEFAILURE: &str = "PostToolUseFailure"` exactly. Updates doc comment on `response_snippet`. No enum changes. Matches integration surface: `pub const &str = "PostToolUseFailure"`.

- Component 4 (Observation Storage / `listener.rs`): `observation-storage.md` adds explicit `"PostToolUseFailure"` arm before wildcard. Calls `extract_error_field()` (not `extract_response_fields()`). New `extract_error_field()` sibling reads `payload["error"]` as plain string, returns `(None, Some(snippet))`. Normalization block explicitly does NOT touch `"PostToolUseFailure"`. Stored `hook` = `"PostToolUseFailure"` verbatim. Matches all integration surface contracts.

- Component 5a (PermissionRetriesRule / `friction.rs`): Internal rename `post_counts` → `terminal_counts`. Both `PostToolUse` and `PostToolUseFailure` accumulated in `terminal_counts`. `retries = pre.saturating_sub(terminal)`. Rule name, category, severity, claim, threshold all unchanged. Matches ADR-004.

- Component 5b (compute_universal / `metrics.rs`): Widens post-bucket to include `POSTTOOLUSEFAILURE`. `saturating_sub` retained. Variable renamed `terminal_counts`. All other computations unchanged. Matches FR-06.

- Component 6 (ToolFailureRule / `friction.rs`): New struct, `impl DetectionRule`. `name() -> "tool_failure_hotspot"`. `category() -> HotspotCategory::Friction`. `severity -> Severity::Warning`. `threshold = 3`. Fires at `count > 3`. Claim: `"Tool '{tool}' failed {count} times"`. `source_domain == "claude-code"` pre-filter. One finding per tool. Evidence records per failure event. Registered in `default_rules()` in `mod.rs`. Count updated to 22. Matches architecture §Component 6 and ADR-005 exactly.

**ADR compliance check**:

| ADR | Requirement | Pseudocode compliance |
|-----|-------------|----------------------|
| ADR-001 | Explicit `"PostToolUseFailure"` arm in both `build_request()` and `extract_event_topic_signal()` | PASS — both explicit arms present in `hook-dispatcher.md` |
| ADR-002 | `extract_error_field()` separate sibling; never call `extract_response_fields()` on failure payload | PASS — `observation-storage.md` creates new sibling, calls `extract_error_field()` in the arm |
| ADR-003 | No normalization; `"PostToolUseFailure"` stored verbatim | PASS — normalization block explicitly skips `PostToolUseFailure` |
| ADR-004 | Atomic two-site commit: `friction.rs` + `metrics.rs` + `mod.rs` in same commit | PASS — `friction-metrics.md` explicitly marks all three files as atomic Wave 2 component |
| ADR-005 | `rule_name = "tool_failure_hotspot"`; threshold 3, fires strictly > 3 | PASS — `"tool_failure_hotspot"` used throughout; comment explicitly says "do not use 'tool_failures'"; `count > TOOL_FAILURE_THRESHOLD` is strict |

---

### Specification Coverage

**Status**: PASS

**Evidence**:

All 12 functional requirements covered:

| FR | Coverage |
|----|---------|
| FR-01 (Hook Registration) | `hook-registration.md` |
| FR-02 (hook_type Constant) | `core-constants.md` |
| FR-03 (Hook Dispatcher build_request) | `hook-dispatcher.md` FR-03.1–FR-03.7 all addressed |
| FR-04 (Storage Layer listener.rs) | `observation-storage.md` FR-04.1–FR-04.6 all addressed |
| FR-05 (PermissionRetriesRule Fix) | `friction-metrics.md` FR-05.1–FR-05.4 all addressed |
| FR-06 (permission_friction_events Fix) | `friction-metrics.md` FR-06.1–FR-06.4 all addressed |
| FR-07 (ToolFailureRule) | `friction-metrics.md` FR-07.1–FR-07.7 all addressed |
| FR-08 (Detection Rule Audit) | Architecture §Detection Rule Audit documents all 21 rules; only PermissionRetriesRule and compute_universal require fixes |

NFR coverage:

| NFR | Coverage |
|-----|---------|
| NFR-01 (40ms latency) | Fire-and-forget `RecordEvent` path; no sync DB writes — confirmed in hook-dispatcher, OVERVIEW |
| NFR-02 (Defensive Parsing) | `unwrap_or` / `Option` chaining throughout hook-dispatcher and observation-storage pseudocode |
| NFR-03 (No Schema Migration) | Stated explicitly in OVERVIEW and component pseudocode; hook TEXT column accepts new value |
| NFR-04 (Test Additivity / make_failure helper) | `make_failure` defined in friction-metrics pseudocode and both test-plan files |
| NFR-05 (Test Count Baseline) | All tests are additive; no deletions |
| NFR-06 (String Constant Discipline) | `pub const POSTTOOLUSEFAILURE: &str` only; no enum variant |

**No scope additions found.** Pseudocode does not implement anything outside the SPECIFICATION or ARCHITECTURE. The `is_interrupt` field is noted as captured but unused, consistent with the NOT-in-scope statement.

---

### Risk Coverage

**Status**: PASS

**Evidence**:

All 14 risks from the Risk-Based Test Strategy have mapped test scenarios:

| Risk | Priority | Pseudocode mitigates | Test plan covers |
|------|----------|---------------------|-----------------|
| R-01 (wrong extractor) | Critical | `observation-storage.md` calls `extract_error_field()` explicitly; ADR-002; negative test T-OS-06 | T-OS-01 to T-OS-07; compound assertion in T-OS-08 |
| R-02 (partial two-site fix) | Critical | `friction-metrics.md` declares atomic three-file Wave 2 component | T-FM-08 asserts BOTH `compute_universal()` AND `PermissionRetriesRule::detect()` in same function |
| R-03 (wildcard stores tool=None) | High | Explicit arm before wildcard in `observation-storage.md`; `obs.tool.is_some()` asserted | T-OS-08 compound assertion; T-OS-10 |
| R-04 (PermissionRetriesRule false fire) | High | `terminal_counts` widened in `friction-metrics.md` | T-FM-01, T-FM-02, T-FM-03 |
| R-05 (build_request wildcard) | Med | Explicit arm in `hook-dispatcher.md` `build_request()` | T-HD-01, T-HD-02, T-HD-03 |
| R-06 (threshold boundary) | Med | `count > TOOL_FAILURE_THRESHOLD` (strictly greater) in `friction-metrics.md` | T-FM-11 (at-threshold no finding), T-FM-12 (above fires) |
| R-07 (source_domain guard) | Med | `source_domain == "claude-code"` pre-filter in `ToolFailureRule::detect()` | T-FM-16, T-FM-17 |
| R-08 (hook non-zero exit) | Med | Defensive `unwrap_or`/`Option` chaining throughout `hook-dispatcher.md` | T-HD-02, T-HD-03, T-HD-04 + binary integration tests |
| R-09 (topic_signal stringify fallthrough) | Low | Explicit arm in `extract_event_topic_signal()` in `hook-dispatcher.md` | T-HD-06 / T-HD-05 in test plan |
| R-10 (response_size non-None) | Low | `response_size = None` always in `observation-storage.md`; ADR-002 | Asserted in T-OS-08 compound block |
| R-11 (constant misspelled) | Med | `POSTTOOLUSEFAILURE: &str = "PostToolUseFailure"` with exact casing | T-CC-01; constant used in assertions not inline literals |
| R-12 (negative metric underflow) | Low | `saturating_sub` in `friction-metrics.md` `compute_universal()` | T-FM-07 (saturating sub test), T-FM-10 |
| R-13 (ToolFailureRule not registered) | Med | Registration in `mod.rs` explicit in `friction-metrics.md` with count 22 | T-FM-19, T-FM-20 |
| R-14 (settings.json wrong pattern) | High | Command pattern and casing requirements explicit in `hook-registration.md` | T-HR-01, T-HR-02, T-HR-03, T-HR-04 |

**Critical correctness items from spawn prompt (verified):**

1. `observation-storage.md` `"PostToolUseFailure"` arm calls `extract_error_field()` — line 132 is `let (response_size, response_snippet) = extract_error_field(&event.payload);`. NOT `extract_response_fields()`. PASS.

2. `hook-dispatcher.md` has explicit `"PostToolUseFailure"` arm in BOTH `build_request()` and `extract_event_topic_signal()` — both confirmed at lines 46 and 103 respectively. Neither is wildcard fallthrough. PASS.

3. No normalization: `observation-storage.md` normalization block explicitly checks `if hook == "post_tool_use_rework_candidate"` only; `"PostToolUseFailure"` falls to the else branch returning `hook` unchanged. PASS.

4. `friction-metrics.md` declares all three files (`friction.rs`, `mod.rs`, `metrics.rs`) as a single atomic Wave 2 component with explicit ADR-004 rationale. PASS.

5. `ToolFailureRule::name()` returns `"tool_failure_hotspot"` — present at line 142 with comment "ADR-005: canonical name; do not use 'tool_failures'". PASS.

6. T-FM-08 (test plan `friction-metrics.md`) asserts BOTH `compute_universal()` AND `PermissionRetriesRule::detect()` on the same `records` slice within a single test function (`test_two_site_agreement_balanced_failure_and_post`). PASS.

---

### Interface Consistency

**Status**: PASS

**Evidence**:

The OVERVIEW.md Integration Surface table defines five integration points. Verified consistency across all pseudocode files:

| Integration Point | Defined In | Used Consistently In |
|------------------|-----------|---------------------|
| `hook_type::POSTTOOLUSEFAILURE` = `"PostToolUseFailure"` | `core-constants.md` | `friction-metrics.md` (both PermissionRetriesRule and ToolFailureRule comparisons), `observation-storage.md` assertions, test helpers |
| `extract_error_field(payload: &Value) -> (Option<i64>, Option<String>)` | `observation-storage.md` | Called only from `"PostToolUseFailure"` arm of `extract_observation_fields()` |
| `ToolFailureRule` / `impl DetectionRule` / `name() -> "tool_failure_hotspot"` | `friction-metrics.md` | Registered in `default_rules()` in same component; consistent name throughout test scenarios |
| `RecordEvent` for PostToolUseFailure: `ImplantEvent { event_type: "PostToolUseFailure", payload: input.extra, topic_signal }` | `hook-dispatcher.md` | `observation-storage.md` reads from `event.payload["tool_name"]` / `payload["error"]` / `payload["tool_input"]` — consistent with payload = `input.extra` |
| Stored `hook` column: `"PostToolUseFailure"` verbatim | `observation-storage.md` | No normalization; consistent with `friction-metrics.md` string comparisons using `hook_type::POSTTOOLUSEFAILURE` |

Payload chain integrity confirmed: `hook-dispatcher.md` passes `input.extra.clone()` as payload, and `observation-storage.md` reads `payload["tool_name"]`, `payload["error"]`, `payload["tool_input"]` — no intermediate field remapping, matching the architecture §Component Interactions diagram.

---

### Knowledge Stewardship Compliance

**Status**: WARN

**Evidence**:

Agents with active-storage roles (architect, risk-strategist):
- `col-027-researcher-report.md` — has `## Knowledge Stewardship` with `Stored: entry #3471` and `Stored: entry #3472`. PASS.
- `col-027-agent-0-scope-risk-report.md` — has `## Knowledge Stewardship` with `Stored: entry #3472`. PASS.
- `col-027-agent-3-risk-report.md` — has `## Knowledge Stewardship` with `Stored: nothing novel to store — {reason}`. PASS.

Read-only agents (spec, pseudocode, test-plan):
- `col-027-agent-2-spec-report.md` — has `## Knowledge Stewardship` with multiple `Queried:` entries. PASS.
- `col-027-agent-1-pseudocode-report.md` — has `## Knowledge Stewardship` with `Queried:` entries (MCP errors noted but attempts made). PASS.
- `col-027-agent-2-testplan-report.md` — has `## Knowledge Stewardship` with `Queried:` and `Stored:` entries. PASS.

**Issue**: `col-027-synthesizer-report.md` has NO `## Knowledge Stewardship` section. The synthesizer is a design-phase agent (it synthesizes design artifacts and is not a read-only agent). Its report contains only a status, outputs list, and self-check checklist. The stewardship block is entirely absent.

**Severity**: WARN (not FAIL). The synthesizer's primary role is artifact assembly, not knowledge generation. The relevant design knowledge from this session (ADR entries, lessons, patterns) was already stored by the upstream agents (researcher, scope-risk, spec agents). No novel knowledge appears to have been generated during synthesis that is absent from other reports. However, the missing block means the self-check requirement is unverifiable.

---

## Rework Required

None. Gate result is PASS.

---

## Knowledge Stewardship

- Stored: nothing novel to store — gate 3a validation patterns for this feature are standard and already covered by existing stewardship entries. No cross-feature pattern emerged from this review beyond what is already in the knowledge base.
