## ADR-002: Force-Set Semantic for Explicit cycle_start Attribution

### Context

The existing `set_feature_if_absent()` implements first-writer-wins: once a session's feature_cycle is set, it cannot be changed. This is the foundational invariant for one-session-one-feature (#1067).

However, SR-01 identifies a race condition: if heuristic signals (file path patterns from early tool calls) trigger eager attribution before the SM agent calls `context_cycle(start)`, the explicit declaration is silently rejected by `set_feature_if_absent`. The explicit signal -- from the SM who knows which feature this session is for -- would lose to a heuristic guess.

Three options:
1. **Preserve first-writer-wins** -- rely on protocol ordering (SM calls context_cycle before any file-touching tools). Fragile: depends on agent behavior.
2. **Force-set for explicit signals** -- a new `set_feature_force()` that overwrites heuristic attribution when an explicit cycle_start arrives. Safe: explicit > heuristic is a sound priority ordering.
3. **Two-phase commit** -- defer all attribution until explicit signal or SessionClose. Complex, breaks eager attribution for sessions that never call context_cycle.

### Decision

Introduce `SessionRegistry::set_feature_force(session_id, feature) -> SetFeatureResult` that unconditionally sets the session's feature_cycle, overwriting any existing value.

`SetFeatureResult` has three variants:
- `Set` -- feature was NULL, now set (same as `set_feature_if_absent` returning true)
- `AlreadyMatches` -- feature was already set to this exact value (no-op)
- `Overridden { previous: String }` -- feature was set to a different value, now overwritten

The `cycle_start` handler in the listener calls `set_feature_force` instead of `set_feature_if_absent`. All other attribution paths (SessionStart, eager voting, majority vote) continue using `set_feature_if_absent`.

The force-set is scoped exclusively to `cycle_start` events. The `set_feature_if_absent` invariant is preserved for all heuristic paths. The priority ordering becomes: explicit > eager > majority.

The `Overridden` variant is logged at `warn` level so that attribution overwrites are visible in server logs for debugging.

### Consequences

**Easier:**
- SR-01 resolved: explicit context_cycle(start) always wins regardless of timing.
- No protocol ordering dependency: SM can call context_cycle at any point in the session.
- Backward compatible: sessions without context_cycle continue using heuristic attribution unmodified.
- The `SetFeatureResult` gives the MCP tool response (if it had session identity) and log output precise information about what happened.

**Harder:**
- Breaks the pure first-writer-wins invariant for one code path. Must be carefully scoped to prevent heuristic paths from accidentally using force-set.
- If an agent erroneously calls `context_cycle(start)` with the wrong topic, it overwrites correct heuristic attribution. Mitigated by: only SM/coordinator agents should call this tool, and they receive the feature ID from the spawn prompt.
- The `update_session_feature_cycle` SQLite persistence must also run on force-set to keep the database consistent.
