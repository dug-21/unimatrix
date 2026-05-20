## ADR-001: SQL Recursive CTEs for chain and current Modes

### Context

`chain` and `current` modes must traverse the supersession chain
(`entries.supersedes` / `entries.superseded_by`). Two alternatives exist:

1. **In-memory `TypedRelationGraph`** — `find_terminal_active` in `graph.rs:523`
   traverses outgoing Supersedes edges to the terminal active node. It is called by
   the search hot path today.

2. **SQL recursive CTE** — queries `entries.supersedes` / `entries.superseded_by`
   directly in the database, bypassing the in-memory graph entirely.

ASS-057 Track B confirmed: `context_correct` does NOT write `GRAPH_EDGES` Supersedes
rows. The in-memory graph derives Supersedes topology from `entries.supersedes` in
Pass 2a and explicitly skips `GRAPH_EDGES` Supersedes rows in Pass 2b
(`graph.rs:294-296`). This means the in-memory path and the SQL path are equivalent
in terms of data completeness — both read from the same source (`entries.supersedes`).

The in-memory path has three specific problems for MCP tool use:

- `TypedRelationGraph` is tick-rebuilt. On cold-start (first tick not yet complete),
  it is `TypedRelationGraph::empty()`. `find_terminal_active` returns `None` for any
  ID on a cold-start graph — silently wrong, not an error the handler can detect.

- The in-memory path requires acquiring `Arc<RwLock<TypedGraphState>>::read()`. This
  couples the read path to the tick cycle and introduces contention under concurrent
  queries.

- vnc-017 ADR-001 established the precedent: when a supersession result is needed
  immediately after a write (e.g., `context_correct` returning `new_entry.id`), the
  SQL path is used directly rather than traversing a potentially stale cache.

### Decision

`chain` and `current` modes use SQL recursive CTEs on `entries.supersedes` and
`entries.superseded_by`. The `find_terminal_active` in-memory function is not used by
either mode.

The 50-hop safety cap is enforced **at the CTE level** via `WHERE depth < 50` in the
recursive step, not in Rust iteration. SQLite's recursive CTE depth limit is well
above 50; the guard prevents unbounded traversal of corrupt chains.

For `current` mode, the cap-firing signal is: the CTE terminates without finding a
row where `superseded_by IS NULL`. The handler interprets this as "chain too long"
and returns an error (AC-07), not a silent empty result.

For `chain` mode, each direction branch independently enforces the cap. The `Truncated`
struct (ADR-002) encodes per-direction cap status.

Four indexes are added to support the CTE steps (ADR-007):
- `idx_entries_supersedes ON entries(supersedes)`
- `idx_entries_superseded_by ON entries(superseded_by)`

### Consequences

Easier: handler behavior is deterministic regardless of tick-cycle state. Cold-start
correctness is guaranteed. No read-lock contention with the tick thread. Consistent
with vnc-017 ADR-001 precedent.

Harder: two new SQL query functions must be added to `unimatrix-store`. The query
functions are separate from the existing in-memory `find_terminal_active` —
implementers must not refactor `find_terminal_active` to use SQL (it remains
in-memory for the search hot path). The two implementations of the same traversal
logic coexist intentionally: in-memory for search performance, SQL for MCP tool
correctness.
