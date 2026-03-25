# Agent Report: col-027-agent-2-spec

## Output

- Wrote: `product/features/col-027/specification/SPECIFICATION.md`

## Key Decisions

1. **FR-04.3: Direct error string extraction, not extract_response_fields().** SR-01 (High/High) required this. The `extract_response_fields()` function reads `payload["tool_response"]` (an object field). `PostToolUseFailure` payloads carry `payload["error"]` (a plain string). Reusing the helper would silently produce `None` for `response_snippet`, violating AC-03. The spec requires direct `payload["error"]` access in the `PostToolUseFailure` arm.

2. **FR-06.4 + AC-05/AC-06/AC-07 coupling.** SR-08 required that metrics.rs and friction.rs be updated atomically. The spec makes this explicit: AC-05/AC-06 (PermissionRetriesRule fix) and AC-07 (permission_friction_events fix) are marked coupled, and FR-06.4 states a partial fix is not acceptable.

3. **FR-08: Explicit detection rule audit table as a delivery requirement.** SR-04 flagged that "affected rules updated" is insufficient. The spec requires an explicit disposition per rule (pass/no-action or fix required) documented in the implementation brief or PR description. This is a process requirement, not a code requirement.

4. **ToolFailureRule naming.** `rule_name = "tool_failure_hotspot"` chosen to distinguish from the `"permission_retries"` family and to be self-describing for retrospective consumers.

5. **NOT in scope: data_quality caveat (SR-06).** SR-06 recommended considering a `data_quality_note` in retrospective output. Accepted as a follow-on; cost-benefit ratio for col-027 scope is unfavorable. Documented in NOT in scope section.

6. **FR-07.6: Rule count increases to 22.** Adding `ToolFailureRule` to `default_rules()` makes the Friction group 5 rules and the total 22. The module-level doc comment and `default_rules()` comment require updating; this is called out explicitly to prevent stale counts.

## Open Questions

None. All SCOPE.md questions are resolved. All SR items are either addressed in spec requirements or accepted as follows:
- SR-02, SR-03: mitigated by defensive parsing requirements (FR-03.5, NFR-02) and fire-and-forget constraint (C-03, NFR-01)
- SR-05, SR-06: accepted as follow-on, documented in NOT in scope

## Knowledge Stewardship

- Queried: /uni-query-patterns for PostToolUse observation hook extraction detection rules -- found entries #763, #2903, #2843, #3471, #3472 (relevant patterns and ADRs confirmed)
- Queried: /uni-query-patterns for PermissionRetriesRule friction detection pre post differential -- found entries #3446, #3419, #1279, #3472 (lesson-learned entries and atomic-update pattern confirmed)
