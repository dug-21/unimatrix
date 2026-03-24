# Agent Report: col-024-agent-1-architect

## Status
Complete.

## Outputs Produced

- `/workspaces/unimatrix/product/features/col-024/architecture/ARCHITECTURE.md`
- `/workspaces/unimatrix/product/features/col-024/architecture/ADR-001-single-block-sync-entry.md` (Unimatrix #3371)
- `/workspaces/unimatrix/product/features/col-024/architecture/ADR-002-named-timestamp-conversion-helper.md` (Unimatrix #3372)
- `/workspaces/unimatrix/product/features/col-024/architecture/ADR-003-structured-log-on-primary-path-fallback.md` (Unimatrix #3373)
- `/workspaces/unimatrix/product/features/col-024/architecture/ADR-004-shared-enrich-topic-signal-helper.md` (Unimatrix #3374)
- `/workspaces/unimatrix/product/features/col-024/architecture/ADR-005-open-ended-window-cap.md` (Unimatrix #3375)

## Key Decisions

1. **ADR-001**: All three SQL steps in `load_cycle_observations` execute inside one `block_sync` closure — same pattern as all existing `ObservationSource` methods.
2. **ADR-002**: `cycle_ts_to_obs_millis(ts_secs: i64) -> i64` named helper (with `saturating_mul`) is the only permitted path for the seconds→millis conversion. No raw `* 1000` literals in window-boundary code.
3. **ADR-003**: `tracing::debug!` with structured fields (`cycle_id`, `path`) fires at each fallback transition in `context_cycle_review`. Addresses SR-06 without altering error semantics.
4. **ADR-004**: `enrich_topic_signal(extracted, session_id, registry) -> Option<String>` is a single private free function in `listener.rs`. All three write sites (RecordEvent, RecordEvents batch, ContextSearch) call it. `ObservationRow.topic_signal` is overridden post-`extract_observation_fields` rather than mutating `ImplantEvent`.
5. **ADR-005**: Open-ended windows use `unix_now_secs()` as implicit stop; no max-age cap. Abandoned-cycle over-inclusion is mitigated by the topic_signal filter in Step 2 (no enriched sessions → empty primary result → legacy fallback).

## Integration Surface (for implementer)

| Symbol | Signature | Location |
|--------|-----------|----------|
| `ObservationSource::load_cycle_observations` | `fn(&self, cycle_id: &str) -> Result<Vec<ObservationRecord>>` | `unimatrix-observe/src/source.rs` |
| `cycle_ts_to_obs_millis` | `fn(i64) -> i64` — `saturating_mul(1000)` | `services/observation.rs` (private) |
| `enrich_topic_signal` | `fn(Option<String>, &str, &SessionRegistry) -> Option<String>` | `uds/listener.rs` (private) |

## Open Questions

1. Whether to add a `tracing::debug!` on the AC-08 signal-mismatch case (extracted signal differs from registry feature). Low cost, aids post-deploy investigation.
2. Whether to distinguish "no cycle_events rows" from "rows exist but no matching topic_signal" in the fallback log. Requires an extra COUNT query; defer unless operational evidence warrants it.

## Knowledge Stewardship

- Stored: ADR-001 (single block_sync entry) → Unimatrix entry #3371, topic: col-024, category: decision
- Stored: ADR-002 (named timestamp conversion helper cycle_ts_to_obs_millis) → Unimatrix entry #3372, topic: col-024, category: decision
- Stored: ADR-003 (structured debug log on primary-path fallback) → Unimatrix entry #3373, topic: col-024, category: decision
- Stored: ADR-004 (shared enrich_topic_signal helper for all write sites) → Unimatrix entry #3374, topic: col-024, category: decision
- Stored: ADR-005 (open-ended window cap at unix_now_secs()) → Unimatrix entry #3375, topic: col-024, category: decision
