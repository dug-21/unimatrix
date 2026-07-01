# vnc-042-researcher — Agent Report

## Deliverable
- `product/features/vnc-042/SCOPE.md` — written.

## Task
Validate GH #843's LOCKED behavior against the codebase; resolve the open parameter-naming
question. Behavior treated as settled (AC-1..AC-7); not relitigated.

## Key Findings
- **AC-5 reuse claim is SOUND.** `follow_to_current(store,id)->Option<u64>`
  (`graph_read_neighbors.rs:36-55`) is the exact primitive: 50-hop cap, `Some(terminal)` on
  Active terminal, `None` on orphaned/quarantined/>50-hops/store-err. `query_current_terminal`
  (`graph_queries.rs:161-201`) returns the full `EntryRecord` via SQL CTE with a mandatory
  `AND e.status=0` terminal guard. No new chain-walk required.
- **Use `follow_to_current`, NOT `handle_current`.** `handle_current`
  (`graph_read_supersession.rs:86-103`) returns `Err` on orphaned/non-existent, which
  violates AC-4's "return what's found, never empty." `follow_to_current` fails soft to the
  fallback path AC-4 needs.
- **Surface to change is tight:** `context_get` handler `tools.rs:950-1052` (raw fetch at
  :978), `GetParams` struct `tools.rs:246-274` (add `#[serde(default)]` field like
  `include_edges`), notice via existing `format_store_success_with_note` precedent (:936).
  Tool-description strings :947-948 must be updated (#4303).
- **Consistency tension confirmed:** `context_graph` uses `resolve_supersessions: Option<bool>`
  default **false** (`graph_read.rs:84`). vnc-042 defaults to follow — same concept, opposite
  default. Central to naming.
- **New spec-level edge case (OQ-2):** on the AC-4 path, `follow_to_current` returns `None`
  (discards the stop-id), so the cheap path returns the *originally requested* entry flagged,
  not the non-active *terminal*. AC-4 wording is ambiguous — flagged for spec.

## Open Questions for human (naming is primary)
- **OQ-1 (DECIDE):** recommend **`follow_supersessions: Option<bool>` default `true`**.
  Boolean (concept is binary; enum's only future value `chain` is Out-of-Scope), shares the
  `supersessions` noun with graph for vocabulary consistency, `follow_*` verb deliberately
  signals the differing default to avoid the same-name/opposite-default trap of exact-matching
  `resolve_supersessions`. Runner-up: exact `resolve_supersessions=true` if exact name-match
  is prioritized over default-safety.
- **OQ-2:** AC-4 — return originally-requested entry (recommended, AC-5-clean) vs the
  non-active terminal (needs helper tweak).
- **OQ-3:** notice/flag rendering under `format="json"` — string vs structured field.

## Requires ADR
Yes — default-behavior change to the most-used read tool; ADR must also rule on graph-vs-get
naming/consistency.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing — surfaced #4468 (CTE-only chain traversal),
  #4538 (status=0 guard makes deprecated-only params moot), #4494 (node-substitution care),
  #4303 (tool descriptions must not lie), #3728 (context_get id coercion history). All applied.
- Stored: nothing novel to store — the naming/default-divergence pattern is unresolved
  (pending human decision) and the rest is feature-specific scope captured in SCOPE.md.
  Storing a pre-decision recommendation would risk poisoning recall.
