# Component 3 — `follow_to_current` visibility widen + re-export

**Files:**
- `crates/unimatrix-server/src/mcp/graph_read_neighbors.rs` (`:36` canonical copy) — widen visibility
- `crates/unimatrix-server/src/mcp/graph_read.rs` — re-export

**Change class:** visibility ONLY. No body change, no signature change, no behavior change.

## Purpose

Make the canonical `follow_to_current` callable from the `tools.rs` handler via the
already-established fully-qualified path `crate::mcp::graph_read::follow_to_current`
(Pattern #4436 — the handler already calls `crate::mcp::graph_read::handle_graph`).

## Current state (verified)

```
// graph_read_neighbors.rs:36
pub(super) async fn follow_to_current(store: &Store, id: u64) -> Option<u64> {
    let mut current = id;
    for _ in 0..50 {                        // 50-hop cap (C-3, #4538) — DO NOT weaken
        let entry = match store.get(current).await { Ok(e) => e, Err(_) => return None };
        match entry.superseded_by {
            None => if entry.status == Status::Active { return Some(current) } else { return None },
            Some(next_id) => current = next_id,
        }
    }
    None
}
```

`Ok`-terminal-Active → `Some(id)`; orphaned/quarantined terminal, store error, or >50 hops
→ `None`. This exact `None`-vs-`Some` contract is what ADR-002 depends on.

## 3a. Widen visibility (`graph_read_neighbors.rs:36`)

```
pub(super) async fn follow_to_current(...)   →   pub(crate) async fn follow_to_current(...)
```

Body UNCHANGED. This is the only edit in this file.

## 3b. Re-export from `graph_read.rs`

Add a re-export so the canonical symbol is reachable at `crate::mcp::graph_read::follow_to_current`,
matching the module's existing re-export style (mirror whatever pattern `handle_graph` /
sibling symbols use — likely `pub(crate) use graph_read_neighbors::follow_to_current;` or a
module re-export). Verify against the actual re-export idiom already present in `graph_read.rs`
rather than inventing one.

## Do NOT (hazard guards)

- Do NOT call or widen the **duplicate** copy at `graph_read_supersession.rs:122` — the handler
  must bind the canonical `graph_read_neighbors.rs:36` copy (R-05). Consolidating the two copies
  is explicitly out of scope (flagged for future cleanup).
- Do NOT use `handle_current` (`graph_read_supersession.rs:86-103`) — it errors on orphaned
  terminals, violating AC-04.
- Do NOT change existing callers of `follow_to_current` (neighbors/subgraph) — widening
  `pub(super)`→`pub(crate)` is a strict superset; their behavior and tests stay green (R-05 sc.3).

## Data Flow

- **Input:** `store: &Store`, `id: u64` (the requested id, passed by the handler with no cast).
- **Output:** `Option<u64>` — `Some(active_terminal_id)` or `None` (dead-end).
- Consumed by Component 1 step 3 resolution branch.

## Error Handling

Store errors inside the walk are already swallowed to `None` (`:41`) by design — the handler
treats `None` as the fail-loud dead-end (ADR-002). No change; do not add `?`/propagation here
(that would alter the shared primitive's contract and break other callers).

## Key Test Scenarios (hints)

- Build + clippy `-D warnings` green after the widen + re-export (R-05 sc.1) — watch for
  `unused`/`dead_code` if the re-export path is wrong.
- Call-site assertion: handler resolves the canonical `crate::mcp::graph_read::follow_to_current`,
  not the `graph_read_supersession.rs:122` duplicate (R-05 sc.2).
- Existing `follow_to_current` callers (neighbors/subgraph) + `graph_queries_tests.rs`
  hop-cap/orphan-guard suite stay green — no behavioral change (NFR-04, R-05 sc.3).
