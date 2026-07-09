## ADR-006 vnc-047: The future tag-mutation home is reserved on the existing `context_tag` tool (seam only — not built)

### Context

Tags are set-once (OQ-2). But "one tag model" (issue north star, SR-06) requires that IF cycle-tag
mutation is ever added, it does not fork a bespoke cycle dialect or spawn a parallel
`context_cycle_tag` tool. OQ-3 RESOLVED: the future mutation path is an additive, entry-defaulting
option on the **existing** `context_tag` tool (tools.rs:1542-1633, `TagParams` :364-376), not a new
tool. Two junctions (`entry_tags`, `cycle_tags`) plus a reserved mutation home invite semantic drift
over time (SR-06) unless the seam is pinned now.

The precedent for a comment-only reservation without a stub already exists on `context_tag` itself
(the two retrofit seams at tools.rs:1558-1561 and :1609-1614). Anti-stub rules forbid `todo!()` /
placeholder handlers, so the reservation is documentation, not code.

### Decision

Reserve, do not build:

- **No mutation surface ships.** No `add`/`remove`/`replace` verb for cycle tags; no new MCP tool; no
  change to `TagParams` or the `context_tag` handler beyond a comment.
- **Documented seam:** add a comment on the `context_tag` handler (near the existing retrofit seams)
  recording the decided future shape — *if* cycle-tag mutation is needed, it becomes an additive
  target selector on `TagParams` (e.g. an optional `target` defaulting to entry), reusing the ported
  `cycle_tags` `ON CONFLICT DO NOTHING` primitive (ADR-002) and the vnc-045 `remove_tag`/`replace_tag`
  primitives re-keyed to `feature_cycle`. Entry-targeting stays the default so the current interface is
  unchanged (AC-06).
- **`context_correct` and the entry `context_tag` path are untouched** (Non-Goal #7).

This is how "one tag model" parity is honored without building mutation now: the write primitive is
already shared (ADR-002 ports `add_tag`), and the mutation door is pinned to the existing tool.

### Consequences

- Easier later: a future mutation feature has a decided home and a shared primitive — no re-litigation,
  no divergent-dialect risk if the seam is followed (SR-06).
- Harder / accepted: the seam is a comment, so it is only as durable as future readers' attention; the
  edge from ADR-002 to the vnc-045 primitive (#5599) is the traversal anchor that keeps the port
  canonical.
- No runtime behavior change; interface stability (AC-06) is preserved — the only external change in
  vnc-047 remains the additive `CycleParams.tags` param.
- Cross-ref ADR-002 (the shared write primitive), vnc-045 #5599 (the primitive being reserved for reuse).
