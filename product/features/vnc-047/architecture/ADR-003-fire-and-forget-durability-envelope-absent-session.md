## ADR-003 vnc-047: Fire-and-forget durability envelope — tags persist iff payload `feature_cycle` is non-empty; absent-session parity via the #519 pre-register

### Context

The hook persistence path has a documented silent-failure history: an absent/evicted session or a
NULL/empty `feature_cycle` drops cycle data with no error (#4140, #981; SR-07). A `cycle_tags` insert
on that same path inherits the failure mode. SR-07 requires the architect to confirm tags persist even
when the SM session is absent/evicted (parity with the #4136/#519 pre-register fix), and forbid a
divergent route (SR-03).

Two independent things could make a session "absent" at cycle_start time:
1. The **in-memory registry** entry was evicted (session drained before cycle_start arrived). This is
   what `handle_cycle_event` step-1b (listener.rs:2894-2908, GH #519) repairs by re-registering the
   session from `payload.feature_cycle` so attribution works.
2. The **payload `feature_cycle`** itself is empty/missing — sanitized to `String::new()` at
   listener.rs:2865-2883, which then skips the whole step-5 persistence block.

Crucially, the existing `goal`/`cycle_event` persistence (step-5, listener.rs:3038) is gated **only**
on `!feature_cycle.is_empty()` — it does **not** require the session to be present in the registry.
The registry (in-memory) and the DB write (cycle_events) are decoupled.

### Decision

Cycle tags inherit `goal`'s exact durability envelope — no more, no less:

- **Persistence gate = `!feature_cycle.is_empty()`** (the same step-5 gate). Tag persistence does
  **not** depend on the session being present in the in-memory registry. An evicted-session cycle_start
  that still carries a `feature_cycle` in its payload persists both the cycle_start row and its tags.
  This is the SR-07 answer: the #519 pre-register (step-1b) restores registry attribution, but tag
  persistence never needed the registry entry in the first place — it reads `feature_cycle` straight
  from the payload.
- **Empty/missing `feature_cycle` ⇒ tags dropped, silently.** This is pre-existing behavior for the
  entire cycle_start event (not just tags); tags do not introduce a new failure mode and do not get a
  special rescue. Documented, not mitigated (there is no `feature_cycle`-less cycle to attribute tags
  to).
- **DB error inside the spawn ⇒ `tracing::warn`, no caller signal.** Parity with the
  `insert_cycle_event` failure arm (listener.rs:3077). No persistence-confirmation API is added (SR-03,
  set-and-forget). The write is one transaction (ADR-002), so a mid-write DB error rolls back both the
  cycle_start row and its tags together — no half-written state.

### Consequences

- Tags survive an evicted/re-registered SM session as long as the payload carries `feature_cycle`
  (SR-07 satisfied; parity with #519).
- No new silent-failure surface beyond the one `goal` already lives with; the envelope is identical, so
  the feature adds no net durability risk.
- Accepted gap: a genuinely `feature_cycle`-less start loses its tags with only a `tracing::warn` — but
  such a start also loses its cycle_event row today, so tags are not a regression.
- Testing must cover the assembled path with an absent registry entry (tags still land) — a
  structural-only direct-insert test would miss this (SR-08).
- Cross-ref ADR-002 (write path), col-025 ADR-004 (#3399, the goal degrade-to-None read contract this
  mirrors on the read side).
