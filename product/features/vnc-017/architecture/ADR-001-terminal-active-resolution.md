## ADR-001: Terminal-Active Resolution — Always Use new_entry.id Directly

### Context

When `context_correct` supersedes entry A with entry B, the redirect loop must point
stale incoming edges at the terminal-active entry in the correction chain. If A was
previously corrected to B and now B is corrected to C, edges pointing at B should be
redirected to C — not to an intermediate. Two resolution strategies were considered:

1. **Cache traversal** — call `find_terminal_active(new_entry.id, graph, entries)` using
   a read lock on `TypedGraphState` to follow Supersedes edges to the terminal node.
2. **Direct use of new_entry.id** — use the freshly created correction entry's ID as the
   target without any traversal.

`find_terminal_active` requires a pre-built `TypedRelationGraph` and entries snapshot
from the `TypedGraphState` tick cache. The cache is rebuilt on a background tick and may
not yet include the entry just inserted. A cold-start or pre-tick-rebuild window would
require a fallback to `new_entry.id` anyway.

`context_correct` can only be called on an `Active` entry (enforced by the write
transaction in `store.correct_entry`). The new correction entry is inserted with
`superseded_by = None` and `status = Active` — it is terminal-active by definition at
creation time with no traversal needed.

The Supersedes graph in `TypedGraphState` derives from `entries.supersedes`, not from
`graph_edges` rows. The tick will incorporate the new `supersedes = original_id` field on
the next rebuild. Waiting for the tick is not necessary; the new entry IS the terminal
active.

### Decision

Always use `correct_result.corrected_entry.id` directly as the redirect target. Do not
call `find_terminal_active`. Do not acquire a read lock on `TypedGraphState`.

Rationale:
1. `context_correct` can only be called on an Active entry, making `new_entry.id` always
   terminal-active at the moment of creation.
2. Cache traversal introduces a read-lock dependency and a cold-cache edge case requiring
   a fallback — complexity with no benefit for forward-going usage.
3. Deep chain resolution (A→B→C) is the responsibility of subsequent corrections: when
   the agent corrects B→C, that call's auto-redirect handles any edges still pointing at
   B. This is the correct incremental model.

If the invariant that `context_correct` may only be called on an Active entry is ever
relaxed (e.g., admin correction of a Deprecated entry), this decision must be revisited.

### Consequences

Easier: no read-lock on TypedGraphState, no cold-cache edge case, no fallback path, no
dependency on the graph tick cycle.

Harder: edges pointing at an intermediate node in a multi-hop correction chain (e.g.,
edges to B when A→B→C exists) are not redirected to C in a single call — they are
redirected to B (the next active node), and only redirected to C when B is later
corrected. Agents making multiple sequential corrections may see intermediate states.
This is accepted: each correction is responsible only for redirecting edges to its own
deprecated-to-active transition.
