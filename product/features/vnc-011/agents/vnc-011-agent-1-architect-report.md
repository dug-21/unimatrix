# Agent Report: vnc-011-agent-1-architect

## Task
Design architecture for vnc-011 (Retrospective ReportFormatter).

## Artifacts Produced

- `/workspaces/unimatrix/product/features/vnc-011/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/vnc-011/architecture/ADR-001-format-dependent-evidence-limit-default.md`
- `/workspaces/unimatrix/product/features/vnc-011/architecture/ADR-002-deterministic-example-selection.md`
- `/workspaces/unimatrix/product/features/vnc-011/architecture/ADR-003-separate-retrospective-module.md`

## Key Decisions

1. **ADR-001**: evidence_limit default changes from 3 to 0 globally (JSON path only). Markdown path ignores evidence_limit entirely — the formatter controls its own k=3 selection. This mitigates SR-03 by making the semantic clear: JSON = full data, markdown = curated view.

2. **ADR-002**: Example selection is deterministic by timestamp (earliest first), not random. Mitigates SR-02. Enables snapshot testing. SCOPE's "random" intent was diversity, not statistical randomness.

3. **ADR-003**: New `retrospective.rs` module in the response layer, separate from `briefing.rs`. Follows existing one-concern-per-module pattern. Existing JSON formatter stays in briefing.rs untouched.

## SR-01 Mitigation (None field combinations)

Exhaustive None-handling table in ARCHITECTURE.md. Each of the 8 Optional fields maps to a section-omission rule. The formatter produces valid markdown for ANY combination of None fields. Minimal report (all None) renders: header + findings + recommendations.

## SR-03 Mitigation (evidence_limit default)

Resolved in ADR-001. The default change from 3 to 0 only affects the JSON path. JSON consumers wanting compact output must pass evidence_limit explicitly. Markdown consumers are unaffected (formatter's k=3 per group is independent).

## Open Questions

1. **Unimatrix storage**: MCP tools were not available in this agent session. ADRs need to be stored via `/store-adr` in a follow-up session.

2. **Duration formatting**: The header summary includes `total_duration`. Should this be formatted as `Xh Ym` (human-readable) or raw seconds? Recommend human-readable, using the existing `format_timestamp` helper in `response/mod.rs` or a similar duration formatter.

3. **Session table time windows**: SCOPE shows `09:52-10:49` format. The `started_at` field is epoch millis. The formatter needs a millis-to-HH:MM converter. The existing `format_timestamp` in `response/mod.rs` converts epoch seconds to UTC datetime strings but not to HH:MM. A small helper is needed.

## Status
Complete.
