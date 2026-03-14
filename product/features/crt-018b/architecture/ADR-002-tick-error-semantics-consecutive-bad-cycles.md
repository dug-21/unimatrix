## ADR-002: Hold (Not Increment) consecutive_bad_cycles on Background Tick Error

### Context

SR-07 identifies a critical failure mode: if `compute_report()` returns an error (e.g., a
transient SQLite lock), the `EffectivenessState` is not updated. The previous
classifications remain stale. Three behaviors are possible for `consecutive_bad_cycles` when
a tick is skipped:

1. **Increment** — treat a missed tick as if the entry was still classified bad. This
   maximises the speed at which auto-quarantine fires after repeated bad classification, but
   it means a single transient error can count as a "bad cycle" even though no classification
   data was computed.

2. **Hold** — leave `consecutive_bad_cycles` at its current value. The tick is simply skipped;
   no change in either direction. Auto-quarantine takes longer to fire if ticks fail
   repeatedly, but it never fires on stale data alone.

3. **Reset** — set all counters back to 0 on tick error. This is the most conservative option
   but would effectively disable auto-quarantine under intermittent failure conditions (e.g.,
   a database under load with occasional lock contention).

The root risk identified in SR-07 is: "if bad classifications linger, `consecutive_bad_cycles`
keeps incrementing and auto-quarantine may trigger on stale data." This is the increment
behavior causing the problem. The concern is that a cycle of failures could promote a
false-positive quarantine without any fresh classification evidence.

There is also an interaction with the N-cycle guard semantics in the SCOPE. The SCOPE specifies
"N consecutive background tick passes" where each pass evaluates the entry as Ineffective or
Noisy. A failed tick does not evaluate the entry — it is not a bad pass, it is an absent pass.
Incrementing on error violates the semantics of "consecutive classification passes."

### Decision

On `compute_report()` error: hold `consecutive_bad_cycles` at its current value. Do not
increment, do not reset.

In addition, emit a structured audit event with:
```
operation: "tick_skipped"
agent_id:  "system"
outcome:   Failure
detail:    "background tick compute_report failed: {error_reason}"
```

This audit event serves two purposes:
1. It provides operators visibility into how often ticks fail.
2. It documents the duration during which classification state was stale, making it easier to
   diagnose apparent auto-quarantine delays.

The `EffectivenessState` is not modified in any way on a failed tick. The write lock is never
acquired on the error path.

### Consequences

Easier:
- Auto-quarantine only fires on fresh, consecutive, successfully-computed classifications.
  No false positives from tick errors.
- The semantics of "N consecutive bad cycles" are honored exactly — only successfully computed
  cycles count.
- Operators can detect and investigate tick failures via the audit log.

Harder:
- Under sustained tick failure conditions, auto-quarantine is effectively paused. An entry that
  should be quarantined after 3 cycles may take longer if ticks are intermittently failing.
  This is acceptable given that the failure mode being avoided (false-positive quarantine) is
  more operationally severe than delayed quarantine.
- Operators must monitor the `tick_skipped` audit events to understand when the consecutive
  counter is "frozen." No automated alerting is added in this feature.
