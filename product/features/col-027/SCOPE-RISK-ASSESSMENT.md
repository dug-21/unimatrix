# Scope Risk Assessment: col-027

## Technology Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-01 | `PostToolUseFailure` payload uses `error` (string) not `tool_response` (object) — the `extract_response_fields()` function was written for the `PostToolUse` object shape; passing a plain string will produce incorrect extraction or a silent None | High | High | Architect must add an explicit `error` field extraction path in `extract_response_fields()` separate from the `tool_response` object path |
| SR-02 | Claude Code hook documentation describes `is_interrupt` boolean on failure payloads; the field may be absent — if `extract_response_fields()` panics or silently skips on missing fields, AC-03 will not be satisfied | Med | Med | Design should validate all extracted fields with defensive `Option` access; absence of `is_interrupt` must not affect `response_snippet` population |
| SR-03 | The 40ms hook timeout (FR-03.7) applies to `PostToolUseFailure`; fire-and-forget `RecordEvent` dispatch must complete channel send within this window even under server load | Med | Low | Architect should confirm the existing `RecordEvent` channel send path meets this budget; no synchronous DB writes may be added |

## Scope Boundary Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-04 | "Blast radius: all 21 detection rules" (SCOPE §Constraints) — audit scope is stated but not validated in this feature. Any rule using `"PostToolUse"` string comparison that is NOT on the Pre-Post differential path may inadvertently match or miss `PostToolUseFailure` records in future rule extensions | Med | Med | Spec writer should require an explicit audit finding per rule (pass/no-action or fix required), not just "affected rules updated" |
| SR-05 | `ToolFailureRule` threshold of 3 is hardcoded with no configuration path; if future features need per-tool or per-phase thresholds, the constant will need extraction | Low | Low | Accept as-is for col-027; note as follow-on if threshold tuning emerges |
| SR-06 | Non-goal: "retroactive correction of past retrospectives" — retrospective findings stored for features prior to nan-002 will remain corrupted. The fix is forward-only, but nothing in scope prevents a consumer from computing trend metrics across old+new data and getting a distorted picture | Med | Low | Architect should consider whether `context_retrospective` response should include a `data_quality` caveat for features predating col-027 |

## Integration Risks

| Risk ID | Risk | Severity | Likelihood | Recommendation |
|---------|------|----------|------------|----------------|
| SR-07 | col-023 ADR-001 (entry #2903) replaced a `HookType` enum with string constants to avoid blast-radius refactors; adding `POSTTOOLUSEFAILURE` as a constant is safe, but `extract_observation_fields()` match arms in listener.rs must not fall through to the wildcard for this new type — the wildcard currently returns `(None, None, None, None)` producing a stored record with no `tool_name` | High | Med | Architect must add an explicit arm in `extract_observation_fields()` before the wildcard; test must assert `tool_name` is populated (not None) |
| SR-08 | `metrics.rs compute_universal()` and `friction.rs PermissionRetriesRule` both independently implement the Pre-Post differential — they must both be updated atomically; a partial fix (one without the other) will cause the metric and the detection rule to diverge | Med | Med | Spec writer should couple AC-05/AC-06 and AC-07 in the same AC group and require both pass before the fix is considered complete |

## Assumptions

- **SCOPE §Background / Hook Dispatcher**: Assumes `PostToolUseFailure` falls through to the `_` wildcard today because the hook is unregistered. If there is any other early-exit path in `build_request()` that would reject an unknown event name before reaching the wildcard, the "add an explicit arm" approach is correct but the current no-data claim needs verification.
- **SCOPE §ObservationRecord Schema**: Assumes `response_snippet` is safely reused for error content. This assumes the error string from `input.extra["error"]` is always a non-null JSON string. If Claude Code emits a null or omits the field, `response_snippet` would be None and AC-03 would fail silently.
- **SCOPE §No Schema Migration Required**: Correct per the string-typed `hook TEXT` column, but relies on no downstream consumer having added an `IN (...)` constraint or a view that enumerates hook types since col-023.

## Design Recommendations

- **SR-01 / SR-07**: The highest-combined-risk items. The architect should implement `extract_response_fields()` to accept a named-field hint (`"error"` vs `"tool_response"`) rather than probing field names at runtime — this makes the distinction explicit and testable. Entry #1268 (lesson-learned) warns that test payloads must match real producer serialization; test fixtures must use the real `error` string shape, not a `tool_response` object.
- **SR-04**: Request the spec writer add an explicit "detection rule audit table" AC that lists all 21 rules with pass/no-action disposition — not just "affected rules updated". Entry #2843 (pattern) documents the blast-radius risk from col-023 hooktype coupling.
- **SR-06**: Consider a lightweight scope addition: a `data_quality_note` field in retrospective output (or a log-only warning) for features observed before `PostToolUseFailure` was registered. This is low-cost and prevents future metric confusion.
