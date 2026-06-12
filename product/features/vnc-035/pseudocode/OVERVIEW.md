# vnc-035 Pseudocode — OVERVIEW

> `context_correct(A → B)` carries A's eligible **outgoing** graph edges forward to the new
> corrected entry B by default (currently silently dropped). New step **8b′** between the
> existing `params.edges` write (8b) and the incoming-redirect loop (8c). Warn-and-continue;
> never rolls back the committed correction. Adds an `edges_carried` ack (omitted when zero).

This OVERVIEW is the contract between the four component files. Read it first; each component
file is self-contained but references the shared types and ordering defined here.

## Components

| Component | File | Crate / Location | New / Changed |
|-----------|------|------------------|---------------|
| `query_outgoing_edges` + `OutgoingEdgeRow` | `query_outgoing_edges.md` | `unimatrix-store/src/read.rs` (or `read_outgoing.rs` — O-2, dev decides on live line count) | New |
| `run_carry_forward_loop` + `CarrySummary` | `run_carry_forward_loop.md` | `unimatrix-server/src/mcp/tools.rs` (sibling of `run_redirect_loop`, ~:4660) | New |
| `context_correct` handler step 8b′ + ack | `context_correct_handler.md` | `unimatrix-server/src/mcp/tools.rs::context_correct` (~:1015), `format_correct_success` caller (~:1162) | Changed |
| docs cleanup (uni-zero SKILL + agent docs) | `docs_cleanup.md` | `.claude/skills/uni-zero/SKILL.md` + agent docs | Changed (doc-edit plan, not code) |

## Data flow

```
context_correct handler  (A = original_id, B = correct_result.corrected_entry.id)
  │
  │  8.  store_ops.correct()                → commits (B Active, A Deprecated)   [EXISTING]
  │  8b. validate_and_write_edges(B, params.edges, now)                          [EXISTING vnc-015]
  │
  ├─ 8b′ run_carry_forward_loop(store, A, B)  ──────────► returns CarrySummary    [NEW]
  │        │
  │        ├─ store.query_outgoing_edges(A)  ──► Result<Vec<OutgoingEdgeRow>>     [NEW store query]
  │        │      SQL excludes 'Supersedes','CoAccess','Informs' (agent-declared only)
  │        │      Err → warn!, return CarrySummary{0,0,0}; correction NOT rolled back
  │        │
  │        └─ for row in rows:
  │             write each onto B via write_graph_edge (source="agent", created_at=now,
  │                                                     weight=1.0; created_at NOT preserved)
  │             count `carried` ONLY on a `true` return  (pattern #4041)
  │             Contradicts → forward (counted) + reverse (not counted) = one logical edge
  │             write SQL-error (false) → failed++ , warn already fired internally; continue
  │
  │  8c. run_redirect_loop(store, A, B)                                           [EXISTING vnc-017]
  │  9.  confidence.recompute(...)                                               [EXISTING]
  │
  └─ 10. ack: append edges_carried = CarrySummary.carried  (omit when 0)         [NEW]
```

## Pipeline placement (load-bearing — ADR-001)

```
 8.  store_ops.correct()
 8b. validate_and_write_edges(B, params.edges, now)   [EXISTING]
 8b′ run_carry_forward_loop(store, A, B)              ◄── NEW
 8c. run_redirect_loop(store, A, B)                   [EXISTING, UNCHANGED]
 9.  confidence.recompute
 10. format response + edges_carried ack (omit when 0)
```

- **8b′ AFTER 8b**: both writes target the same id B with `INSERT OR IGNORE` on
  `UNIQUE(source_id,target_id,relation_type)`. The final edge set is order-independent
  (idempotent insert is commutative), but ordering is **material to counting**: an edge
  `params.edges` already wrote in 8b is a UNIQUE conflict in 8b′ (`false`), so it is NOT
  double-counted. `edges_carried` reports only edges the caller did NOT re-supply (ADR-001/003).
- **8b′ BEFORE 8c**: carry reads A's **outgoing** rows (`source_id = A`); redirect reads A's
  **incoming** rows (`target_id = A`). Disjoint row sets (a self-loop `A→A` is rejected at
  write time), so no `Contradicts` pair is touched by both loops in one correction (ADR-005, SR-06).

## Shared types

```rust
// unimatrix-store — defined in query_outgoing_edges.md
pub struct OutgoingEdgeRow {
    pub target_id: u64,
    pub relation_type: String,
    pub created_at: u64,   // read for ordering/observability ONLY; NOT written onto B (ADR-004)
}

// unimatrix-server/src/mcp/tools.rs — defined in run_carry_forward_loop.md
pub(super) struct CarrySummary {
    found: usize,    // eligible outgoing rows returned by query_outgoing_edges
    carried: usize,  // write_graph_edge `true` returns → the edges_carried ack value
    failed: usize,   // distinguished SQL-error writes → the SR-01 observable signal
}
```

`run_carry_forward_loop` returns `CarrySummary` **by value** (not `Option` like
`run_redirect_loop`). The handler reads `summary.carried` for the ack and omits the ack when
`carried == 0`. Returning a value (not `Option`) lets the handler always observe `found`/`failed`
for logging without `if let` nesting; the query-Err path returns `CarrySummary{0,0,0}` (ADR-002).

## Reused primitives (no signature change)

| Primitive | Location | Contract |
|-----------|----------|----------|
| `write_graph_edge` | `services/nli_detection.rs:78` | returns `bool`: `true`=insert, `false`=UNIQUE conflict (no warn), `false`=SQL error (warns internally, no `Err`). Pattern #4041. |
| `validate_and_write_edges` | `mcp/edge_write.rs:152` | discards the per-edge bool (R-08) — carry loop CANNOT delegate to it for counting; it owns its own write loop (ADR-003). |
| `EDGE_SOURCE_AGENT` | `mcp/edge_write.rs:28` | `"agent"` — bound to both `source` and `created_by`. |
| `RelationType` | `unimatrix_engine::graph` | `Contradicts` special-cased in carry loop (ADR-005). |
| `run_redirect_loop` | `mcp/tools.rs:4660` | model to mirror (posture, pub(super) test visibility); UNCHANGED. |
| `format_correct_success` | `mcp/response/entries.rs:301` | ack appended to its `CallToolResult` by the handler, mirroring the `format_redirect_summary` append (~:1167). |

## Sequencing constraints (build order)

1. `query_outgoing_edges` + `OutgoingEdgeRow` (store) — no dependency; build first.
2. `run_carry_forward_loop` + `CarrySummary` (tools.rs) — depends on (1) and on the
   fault-injection seam (AC-07). Build second.
3. `context_correct` handler step 8b′ + ack — depends on (1) and (2). Build third.
4. docs cleanup — independent; can land any time but is a hard AC-10/AC-11 coupling
   (neither ships without the other).

## Cross-cutting constraints (apply to every code component)

- **No rollback (NFR-01):** 8b′ runs after the correction commits. Nothing in carry can
  return `Err` to the handler or abort the correction.
- **Single SQL eligibility predicate (NFR-02 / SR-03):** the `NOT IN (...)` clause lives in
  exactly one place — the `query_outgoing_edges` SQL. No parallel Rust-side filter.
- **Count = actual inserts (NFR-03 / SR-02 / #4041):** `carried` keys off `true` only.
- **created_at = now, source = agent, no provenance marker (ADR-004 / FR-11 / R-11):** never
  preserve the source row's `created_at`/`created_by`.
- **Workspace rules (NFR-05):** no `unsafe`; no `.unwrap()`/`.expect()` in non-test code;
  ≤500 lines/file.
- **AC-07 fault-injection seam:** the carry write loop MUST expose a seam so a test can drive
  exactly one mid-loop edge write to a SQL-error (`false`). Design is in `run_carry_forward_loop.md`.

## Open questions (flagged, non-blocking)

- **O-1 (index):** confirm whether `idx_graph_edges_source_id` exists. `query_incoming_edges`
  relies on `idx_graph_edges_target_id`; the carry query filters on `source_id`. Latency-only
  (R-09) — developer verifies and notes; no pseudocode gap.
- **O-2 (module split):** `query_outgoing_edges` lands in `read.rs` or a new `read_outgoing.rs`
  depending on `read.rs`'s live line count vs the 500-line rule. `read.rs` is already >1570
  lines (noted at `read.rs:1800`) — a new module is the likely choice; developer decides.
- **AC-07 seam shape:** ADR-003 leaves the seam *mechanism* to the developer. This pseudocode
  specifies a `#[cfg(test)]` injectable write-fn indirection as the recommended seam (see
  `run_carry_forward_loop.md` §Fault-injection seam); the developer may substitute an
  equivalent that lets one mid-loop write return `false`-as-SQL-error. The CONTRACT (one
  mid-loop edge driven to SQL-error; `failed++`; warn; correction + prior carries persist) is fixed.
