## ADR-002: synthesize_from_session Returns current_goal Directly

### Context

`derive_briefing_query` (src/services/index_briefing.rs) uses a three-step
priority function:

1. Explicit `task` param (wins if non-empty).
2. `synthesize_from_session(state)` — currently returns `Some(string)` only when
   both `feature_cycle` AND non-empty `topic_signals` are present, synthesizing
   `"{feature_cycle} {top_3_signals}"`. Returns `None` otherwise.
3. Topic-ID fallback string (always available).

Step 2's current synthesis is weak when signals are sparse or when a session has
just started. The feature goal, by contrast, is a direct declaration of intent
recorded at cycle start — always the most semantically precise query available
before the agent has accumulated topic signals.

Two alternatives were considered:

**Option A**: Replace `synthesize_from_session` body to return `state.current_goal`
directly. When `None` (no goal stored or legacy cycle), fall through to step 3
topic-ID as before.

**Option B**: Keep the topic-signal synthesis and add `current_goal` as a higher-
priority step 1.5 between the explicit task and the synthesis. This would mean
goal wins over explicit task when both are present, which violates the principle
that the most specific caller-provided signal wins.

**Option C**: Blend goal with topic signals (e.g., `"{goal} {top_3_signals}"`).
Not in scope (SCOPE.md §Non-Goals: no scoring pipeline changes). The goal improves
the *query*, not a scoring dimension.

Option A was chosen. When a goal is set, it is always a better step-2 signal than
a topic-signal concatenation. When no goal is set, the fallback behavior is
identical to today. The function signature of `derive_briefing_query` is unchanged,
and `synthesize_from_session` remains a pure sync function that reads already-
resolved `SessionState` — no DB reads, no async.

### Decision

`synthesize_from_session(state: &SessionState) -> Option<String>` returns
`state.current_goal.clone()`.

The existing topic-signal synthesis logic (`extract_top_topic_signals`,
`format!("{feature_cycle} {signals.join(" ")}") `) is removed. Its tests are
superseded by tests that verify step 2 returns `current_goal` when set.

The three-step priority in `derive_briefing_query` is otherwise unchanged:

```
Step 1: explicit task param (wins if non-empty)
Step 2: state.current_goal (wins if Some and non-empty)
Step 3: topic string (always available)
```

Because `derive_briefing_query` is the single shared implementation for both the
MCP `context_briefing` path and the UDS `handle_compact_payload` path, both paths
benefit automatically. No additional wiring is required for the CompactPayload
injection path (AC-07 is satisfied implicitly by this decision).

### Consequences

- Briefing query precision improves for all sessions with an active feature cycle
  that was started with a `goal` param.
- Sessions without a goal (legacy cycles, cycles started without goal) degrade
  gracefully to the topic-ID fallback.
- `synthesize_from_session`'s previous topic-signal synthesis behavior is removed.
  If any future feature wants topic-signal blending, it must be introduced explicitly.
- Existing `derive_briefing_query` tests that verified the topic-signal synthesis
  path (step 2 with signals but no goal) will need updating to reflect the new
  step-2 semantics.
- The briefing and CompactPayload paths remain covered by the same shared function;
  divergence risk (SR-06) is prevented by architecture, not test duplication.
