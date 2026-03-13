## ADR-001: Reuse RecordEvent for Cycle Lifecycle Wire Protocol

### Context

The hook handler needs to communicate cycle_start and cycle_stop signals to the UDS listener. Two options exist:

- **Option A (new variants):** Add `HookRequest::CycleBegin` and `HookRequest::CycleEnd` variants to the wire protocol enum.
- **Option B (reuse RecordEvent):** Send cycle signals as `HookRequest::RecordEvent` with special `event_type` values ("cycle_start", "cycle_stop") and feature_cycle/keywords in the payload.

The existing `RecordEvent` handler (listener.rs:598-618) already extracts `feature_cycle` from event payloads and calls `set_feature_if_absent()`. This was added in bugfix-198 precisely for payload-based attribution.

### Decision

Reuse `RecordEvent` with `event_type: "cycle_start"` / `"cycle_stop"`. The feature_cycle and keywords are carried in the `ImplantEvent.payload` JSON object.

For `cycle_start`, the existing #198 extraction path (listener.rs:598-618) handles attribution automatically. The only new listener code needed is keywords extraction and a `cycle_start`-specific match arm that calls `set_feature_force` instead of `set_feature_if_absent` (see ADR-002).

For `cycle_stop`, the generic observation persistence path records the event without session state changes.

The `topic_signal` field on `ImplantEvent` is set to the `topic` value, providing a strong signal for eager attribution as a secondary path.

Wire protocol backward compatibility is maintained: old hook binaries that do not produce `cycle_start` events continue to work. Old listeners that receive unknown `event_type` values process them through the generic `RecordEvent` handler (which already extracts `feature_cycle` from payload via #198), so attribution still works -- just without the force-set semantic.

### Consequences

**Easier:**
- Zero wire protocol changes. No new `HookRequest` variants. No serde compatibility concerns.
- Cycle events flow through the established observation persistence path, so they appear in the observations table for retrospective analysis.
- The #198 payload extraction already handles the base case.

**Harder:**
- The listener must match on `event_type` string values ("cycle_start") before the generic `RecordEvent` handler, adding implicit coupling between hook and listener via string constants.
- Future developers must know that "cycle_start" is a magic event_type with special handling.

Mitigated by: defining event_type constants in a shared location (e.g., `unimatrix-engine::wire` or a constants module).
