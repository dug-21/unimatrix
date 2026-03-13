# Agent Report: col-022-agent-1-architect

## Task
Design architecture for col-022 (Explicit Feature Cycle Lifecycle).

## Status: COMPLETE

## Artifacts Produced

- `/workspaces/unimatrix/product/features/col-022/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/col-022/architecture/ADR-001-reuse-record-event-wire-protocol.md`
- `/workspaces/unimatrix/product/features/col-022/architecture/ADR-002-force-set-for-explicit-attribution.md`
- `/workspaces/unimatrix/product/features/col-022/architecture/ADR-003-json-column-for-keywords.md`
- `/workspaces/unimatrix/product/features/col-022/architecture/ADR-004-shared-validation-function.md`
- `/workspaces/unimatrix/product/features/col-022/architecture/ADR-005-schema-v12-keywords-migration.md`

## Key Design Decisions

1. **ADR-001: Reuse RecordEvent** -- Cycle signals sent as RecordEvent with event_type "cycle_start"/"cycle_stop". Zero wire protocol changes. Existing #198 payload extraction handles attribution automatically.

2. **ADR-002: Force-set for explicit attribution** -- New `set_feature_force()` method that overwrites heuristic attribution when explicit cycle_start arrives. Priority: explicit > eager > majority. Resolves SR-01 race condition.

3. **ADR-003: JSON column for keywords** -- Single nullable TEXT column on sessions table (JSON array). Follows established ADR-007 pattern for non-queried Vec fields. Future injection pipeline reads and deserializes.

4. **ADR-004: Shared validation** -- Single `validate_cycle_params()` function in validation.rs used by both MCP tool and hook handler. Resolves SR-07 split-brain risk.

5. **ADR-005: Schema v12** -- ALTER TABLE sessions ADD COLUMN keywords TEXT. No backfill needed.

## Scope Risk Mitigations

- **SR-01** (race condition): ADR-002 force-set semantic ensures explicit signal always wins.
- **SR-07** (validation split-brain): ADR-004 shared validation function prevents divergence.
- **SR-04** (tool ships inert): Acknowledged in architecture. Follow-up issue for protocol integration is out of scope but documented.

## Open Questions

1. `is_valid_feature_id` is `pub(crate)` in unimatrix-observe. Implementer must decide: re-export as `pub` or duplicate validation logic. Check if unimatrix-server already depends on unimatrix-observe.

2. MCP tool response cannot confirm whether hook-side attribution succeeded (server has no session_id). Tool response should acknowledge parameter acceptance only.

## Unimatrix Storage

All 5 ADRs failed to store in Unimatrix -- agent lacks Write capability. The Design Leader must store these ADRs using a privileged agent or store them manually.
