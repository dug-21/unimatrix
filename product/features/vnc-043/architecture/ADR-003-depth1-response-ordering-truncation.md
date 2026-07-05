## ADR-003 vnc-043: Depth-1 response contract — stable ordering (uniform across paths) + truncation semantics

### Context
Two response-contract questions must be settled at design time, not left to the coder:

- **SR-03 / AC-14 (ordering).** Neither subgraph path sorts today: `collected_node_ids` is in BFS
  discovery order and `fetch_nodes_batch` returns store-natural (arbitrary) order; `collected_edges` is
  in discovery order. The live-DB path may therefore order nodes/edges differently from the cache-BFS
  path, so the DoD one-shot would be non-deterministic and depth-1 vs depth>1 outputs could diverge
  confusingly. SCOPE resolves that a stable, documented order is required but leaves the key to the
  architect.
- **SR-05 / Open Q5 (truncation).** `subgraph_via_db` honors `max_nodes` (default 200, hard cap 200)
  and sets `truncated`. AC-15 requires `truncated == false` at "realistic goal fan-in", but "realistic"
  is undefined and a very-high-fan-in goal could still cap seed + one hop.

### Decision
**Ordering.** Return depth-1 (and all subgraph) results in a stable, documented order:
- `nodes` sorted by **ascending `id`** (`EntryRecord.id`).
- `edges` sorted by the canonical triple **`(source_id, target_id, relation_type)`** ascending.

Apply this sort as a final assembly step in **both** `subgraph_via_db` (serves depth-1 always + depth>1
cold-start fallback) and `handle_subgraph`'s warm-BFS assembly, so callers see **one ordering contract
regardless of path or depth** (SR-03). The sort is presentation-only and set-preserving: it runs after
the R-05 dangling-edge filter and does not change which nodes/edges are returned, so AC-02's behavioral
equivalence for depth>1 (same set, same `use_fallback` branch, same tick staleness) holds — only the
serialized order becomes deterministic, and a pre-existing warm-vs-cold order mismatch is removed.
`depth_reached` (computed via `.max()`) and `truncated` are order-independent and unchanged. Each
`EdgeRecord` still carries its own `depth`, so sorting does not lose per-edge hop information.

**Truncation.** The board caller does **not** raise `max_nodes`; the default (200) covers realistic
fan-in. Concretely: at depth-1 the result is `seed(s) + one hop`, so a single-goal read tolerates up to
**199** incoming capabilities before truncating. "Realistic fan-in" for the AC-15 test = **≥ 30 incoming
`Advances` capabilities on the seed goal**, asserting `truncated == false` — above a real goal's
capability count, well under the 199 headroom. The existing `truncated` flag stays surfaced and
asserted so a pathological >199-neighbor read signals partial rather than silently truncating
(no-silent-truncation, AC-15). No new field; `SubgraphResponse` shape unchanged (ADR-004 vnc-019).

### Consequences
Easier: the DoD one-shot is deterministic and testable; callers get one ordering across depth-1/depth>1
and warm/cold paths; the board read is never silently partial (explicit `truncated`); no schema change.
Harder: the sort adds an O(n log n) step over ≤200 nodes / ≤1000 edges (negligible); an existing depth>1
test that asserts a *fixed* output order (rather than a set) must be updated as presentation-only —
`fetch_nodes_batch` already documents "arbitrary order," so well-written tests compare sets and are
unaffected. Applying the order to depth>1 slightly widens the change beyond the depth-1 mandate, chosen
deliberately to avoid shipping two ordering contracts (SR-03).
