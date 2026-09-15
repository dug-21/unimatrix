## ADR-009: Degraded/experimental legs absorbed into the parity-gap posture — fail-safe, documented, non-cap-failing

### Context
Two legs are reachable but not full-fidelity (ass-106 §B, SR-03, SR-04):
- **PreCompact** rests on `experimental.session.compacting`, which OpenCode flags unstable; the API may
  change or break silently.
- **Stop** derives from `session.idle`, whose semantics are undocumented — it may fire on every idle
  transition, risking SessionClose over-count; `duration`/`outcome` must be computed plugin-side.
  SessionStart is similarly bus-derived (no `transcript_path` file).

SR-03/SR-04 direct: absorb these into the parity-gap posture — document honestly as degraded/
experimental, do not let their instability **fail** the capability, and do not let them **silently
corrupt** data (over-count / mis-derived fields).

### Decision
1. **Record as measured parity gap, not defects** (C18 `done_when`): SubagentStart injection
   unreachable, no stdin contract, Stop/SessionStart bus-derived-degraded, PreCompact experimental.
   These are inherent to OpenCode's architecture (ARCHITECTURE "Measured parity gap").
2. **PreCompact**: C1 subscribes to `experimental.session.compacting` behind a plugin capability
   flag/`try-catch`; if the API is absent or throws, the leg no-ops **loud in plugin logs** and the
   other six events are unaffected. Its instability must not fail the cap. Flag default decided at
   delivery (open question).
3. **`session.idle`→Stop over-count guard**: C1 emits Stop **once per active→idle transition**, guarded
   against repeat idle events within the same session-active window; it does **not** silently
   over-count. Requires a delivery-time PoC to confirm idle semantics (SR-04 open question); until
   measured, the guard is conservative (dedupe) and Stop `duration`/`outcome` are marked
   plugin-derived, not full-fidelity.
4. **Attribution correctness is orthogonal and not degraded**: even on degraded legs, provider/
   `source_domain`/`model_id` (ADR-001/002) are stamped correctly — a degraded Stop still lands as
   `source_domain=opencode` with its model.

### Consequences
Easier: the cap is not held hostage to an upstream experimental API or undocumented bus semantics;
data quality risks are contained (dedupe + honest labeling) rather than silently corrupting records.
Harder: Stop/SessionStart metrics are lower-fidelity than claude-code's; the idle PoC is a delivery
prerequisite for the over-count guard; PreCompact may regress if OpenCode changes the experimental API
(accepted, flagged). Cross-references ADR-004 (plugin), ADR-001/002 (attribution stays correct).
