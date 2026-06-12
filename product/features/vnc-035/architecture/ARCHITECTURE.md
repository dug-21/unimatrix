# vnc-035 Architecture — `context_correct` Outgoing-Edge Carry-Forward

> Feature: `context_correct` carries the original entry's eligible **outgoing** graph
> edges forward to the new corrected entry **by default** (currently silently dropped).
> All product decisions are settled (SCOPE Resolved Decisions OQ-01..05). This document
> designs the implementation against them and addresses SR-01..SR-08.

## System Overview

`context_correct(A → B)` deprecates entry A and creates a new active entry B. Today the
handler (`unimatrix-server/src/mcp/tools.rs::context_correct`, ~:1015) repairs the typed
knowledge graph **asymmetrically**:

- **Incoming** edges `E → A` are auto-redirected to `E → B` by `run_redirect_loop`
  (step 8c, vnc-017), bounded by `REDIRECT_CEILING = 50`, warn-and-continue.
- **Outgoing** edges `A → X` are **never carried**. Only `params.edges` are written, and
  they attach to B (step 8b, Phase B, vnc-015).

This feature adds a symmetric **outgoing carry-forward** step that reads A's eligible
outgoing edges and re-writes them as B's outgoing edges, then composes the passed
`params.edges` additively on top. The design mirrors vnc-017's incoming path in posture
(warn-and-continue, `Supersedes` exclusion, never abort the correction) and reuses the
vnc-015 write primitives.

The carry-forward is **purely additive** to the existing pipeline: it inserts a new step
**8b′** between Phase B edge writes (8b) and the incoming redirect loop (8c). No existing
step changes behavior; the only externally visible change is a new `edges_carried` integer
in the response ack (AC-11).

### Pipeline order (the load-bearing sequencing decision — ADR-001)

```
 8.  store_ops.correct()            → commits the correction (B created, A deprecated)
 8b. Phase B: write params.edges    → outgoing edges on B  (EXISTING, vnc-015)
 8b′ Carry-forward loop  ◄── NEW    → copy A's eligible outgoing edges onto B
 8c. run_redirect_loop              → redirect incoming E→A to E→B (EXISTING, vnc-017)
 9.  confidence.recompute
 10. format response + edges_carried ack
```

The carry-forward runs **after** Phase B (`params.edges`) so that the carry baseline and
the passed edges meet on the same target id (B) and compose by the natural `INSERT OR
IGNORE` UNIQUE-conflict dedupe (ADR-004). It runs **before** 8c so outgoing-carry and
incoming-redirect operate on disjoint edge sets and cannot interfere on `Contradicts`
(ADR-005, SR-06).

## Component Breakdown

| Component | Crate / File | Responsibility | New / Changed |
|-----------|--------------|----------------|---------------|
| `query_outgoing_edges` | `unimatrix-store/src/read.rs` (or new `read_outgoing.rs` if it breaches the 500-line rule) | Read A's eligible outgoing edges with the agent-declared-only eligibility predicate at the SQL level | **New** |
| `OutgoingEdgeRow` | same module as the query | Row DTO: `target_id`, `relation_type`, `created_at` | **New** |
| `run_carry_forward_loop` | `unimatrix-server/src/mcp/tools.rs` (sibling of `run_redirect_loop`) | Orchestrate the carry: query eligible edges, write each onto B via the shared write helper, accumulate `edges_carried` | **New** |
| `CarrySummary` | `unimatrix-server/src/mcp/tools.rs` | Accumulator: `found`, `carried`, `failed` | **New** |
| `context_correct` handler | `unimatrix-server/src/mcp/tools.rs:1015` | Insert step 8b′; thread `edges_carried` into the ack | **Changed** |
| Response ack (`format_correct_success` caller path) | `unimatrix-server/src/mcp/tools.rs` (~:1162) | Surface `edges_carried` count (omitted when zero) | **Changed** |
| `validate_and_write_edges` | `unimatrix-server/src/mcp/edge_write.rs:152` | Reused by carry loop for `Contradicts` bidirectional handling — see ADR-005 | Reused (possibly + count variant, ADR-003) |
| `uni-zero` SKILL + agent docs | `.claude/skills/uni-zero/…` | Remove manual re-declaration guidance; document carry-forward + shed path | **Changed** (AC-10) |

## Component Interactions / Data Flow

```
context_correct handler (B = corrected_entry.id, A = original_id)
  │
  ├─ 8b  validate_and_write_edges(store, B, params.edges, now)        [vnc-015]
  │
  ├─ 8b′ run_carry_forward_loop(store, A, B) ──────────────► returns CarrySummary
  │        │
  │        ├─ store.query_outgoing_edges(A)  ──► Vec<OutgoingEdgeRow>  [agent-declared only]
  │        │      (SQL excludes Supersedes + CoAccess + Informs)
  │        │
  │        └─ for row in rows:
  │             write each onto B (relation_type preserved, source="agent",
  │                                created_at = now — NOT preserved; no provenance marker, ADR-004)
  │             count `carried` only on a true (new-insert) return  [SR-02, ADR-004]
  │             Contradicts → bidirectional via shared helper        [AC-06, ADR-005]
  │             write failure → warn + failed++ , never abort        [SR-01, ADR-002]
  │
  ├─ 8c  run_redirect_loop(store, A, B)                               [vnc-017, unchanged]
  │
  └─ 10  ack: append edges_carried = CarrySummary.carried  (omit when 0)  [AC-11]
```

`edges_carried` = count of **newly inserted** carry edges (the `true` returns), per ADR-004.
An edge already present on B because `params.edges` (8b) wrote the same triple returns
`false` (UNIQUE conflict) and is **not** counted — there is exactly one edge per triple and
it was already counted by its first writer. This keeps the ack honest and idempotent (SR-02).

## Eligibility Predicate (single source of truth — SR-03, ADR-002)

`query_outgoing_edges` mirrors `query_incoming_edges` exactly but on `source_id`, and is the
**only** place the outgoing eligibility filter is expressed. The agent-declared-only set is
enforced at the **SQL level** (consistent with vnc-017 ADR-002 expressing `Supersedes`
exclusion in SQL):

```sql
SELECT target_id, relation_type, created_at
FROM graph_edges
WHERE source_id = ?1
  AND relation_type NOT IN ('Supersedes', 'CoAccess', 'Informs')
```

- `Supersedes` — derived from `entries.supersedes`, rebuilt by the graph tick (vnc-017 precedent).
- `CoAccess`, `Informs` — tick-generated affinity classes, re-materialize on their own.
- Everything else (`Contradicts, Supports, Prerequisite, Advances, Motivates, RelatedTo`, …)
  is agent-declared and **carries**.

This predicate is documented as an invariant: **"no ceiling" is safe only while
eligibility = agent-declared-only** (SR-04). The exclusion list bounds agent-declared
out-degree; if a future agent-declarable type is added to the engine taxonomy
(`graph.rs:139`) it carries automatically (accepted, SCOPE Assumptions). The incoming query
excludes only `Supersedes` because `CoAccess`/`Informs` are never *incoming-relevant* to a
correction target the way they are *outgoing* from a hub entry — the outgoing predicate is a
**superset** exclusion and is documented as such so the two cannot be mistaken for drift
(see ADR-002 for why they legitimately differ).

## Integration Surface

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `query_incoming_edges` | `async fn(&self, target_id: u64) -> Result<Vec<IncomingEdgeRow>>` | `unimatrix-store/src/read.rs:1694` (model to mirror) |
| `IncomingEdgeRow` | `{ source_id: u64, relation_type: String, created_at: u64 }` | `unimatrix-store/src/read.rs:1781` |
| **`query_outgoing_edges`** (NEW) | `async fn(&self, source_id: u64) -> Result<Vec<OutgoingEdgeRow>>` | `unimatrix-store/src/read.rs` (new) |
| **`OutgoingEdgeRow`** (NEW) | `{ target_id: u64, relation_type: String, created_at: u64 }` — note: `created_at` is read for ordering/observability only; the carry **re-stamps `created_at = now`**, source row's value is NOT written onto B (ADR-004) | new, same module |
| `write_graph_edge` | `async fn(store, source_id, target_id, relation_type: &str, weight: f32, created_at: u64, source: &str, metadata: &str) -> bool` | `unimatrix-server/src/services/nli_detection.rs:78` |
| `write_graph_edge` return contract | `true`=new insert; `false`=UNIQUE conflict (idempotent, no warn); `false`=SQL error (warns internally). No `Err`. | pattern #4041 |
| `validate_and_write_edges` | `async fn(store, source_id: u64, edges: &[EdgeInput], created_at: u64) -> Result<(), EdgeValidationError>` (discards per-edge bool) | `unimatrix-server/src/mcp/edge_write.rs:152` |
| `EDGE_SOURCE_AGENT` | `const &str = "agent"` | `unimatrix-server/src/mcp/edge_write.rs:28` |
| `redirect_graph_edge` | `async fn(store, source_id, old_target_id, new_target_id, relation_type: &str, created_at: u64) -> Result<(), EdgeRedirectError>` | `unimatrix-server/src/mcp/edge_write.rs:300` |
| `run_redirect_loop` | `async fn(store, original_id: u64, new_entry_id: u64) -> Option<RedirectSummary>` (pub(super), test-visible) | `unimatrix-server/src/mcp/tools.rs:4660` |
| **`run_carry_forward_loop`** (NEW) | `async fn(store, original_id: u64, new_entry_id: u64) -> CarrySummary` (pub(super), test-visible) | `unimatrix-server/src/mcp/tools.rs` (new) |
| **`CarrySummary`** (NEW) | `pub(super) struct { found: usize, carried: usize, failed: usize }` | `unimatrix-server/src/mcp/tools.rs` (new) |
| `context_correct` handler | step 8b′ insertion + `edges_carried` ack | `unimatrix-server/src/mcp/tools.rs:1015` |
| `graph_edges` UNIQUE | `UNIQUE(source_id, target_id, relation_type)` (the carry-forward triple) | `INSERT OR IGNORE` in `write_graph_edge` |
| `idx_graph_edges_source_id` | index on `source_id` (verify exists; mirror of `idx_graph_edges_target_id`) | migration — **open question O-1** |

### Error boundaries

- `query_outgoing_edges` failure → `run_carry_forward_loop` logs `warn!`, returns
  `CarrySummary { found: 0, carried: 0, failed: 0 }`; correction already committed, never
  aborts (mirrors `run_redirect_loop`'s `None` on query failure).
- Per-edge `write_graph_edge` failure → `false` from a SQL error path (already warned
  internally); the loop increments `failed`, continues. `edges_carried` reflects only
  successful inserts. **This is the SR-01 observable failure path** (ADR-002).
- `Contradicts` reverse-direction write failure → accepted partial-write (ADR-003 vnc-015),
  warn-and-continue.

## Technology Decisions (see ADRs)

| Decision | ADR |
|----------|-----|
| Carry-forward step placement & composition order (8b′ between 8b and 8c) | ADR-001 |
| New `query_outgoing_edges` + single-source eligibility predicate (agent-declared-only) | ADR-002 |
| `edges_carried` count contract — count `true` inserts; own write loop (not `validate_and_write_edges`'s discarded bool) | ADR-003 |
| Additive-on-triple upsert composition via `INSERT OR IGNORE` UNIQUE dedupe | ADR-004 |
| Warn-and-continue posture parity; never roll back; observable failure path | ADR-002 |
| `Contradicts` bidirectional carry + disjointness from incoming redirect | ADR-005 |

## Risk Coverage Map

| Risk | Mitigation in this design |
|------|---------------------------|
| **SR-01** (silent missing failure-path test) | ADR-002: `failed` counter + per-edge warn make the Err path observable; carry loop returns `CarrySummary` so a test can assert correction + already-carried edges persist on a forced write failure. Spec must name the test (lesson #4473). |
| **SR-02** (rows-affected/count drift) | ADR-003 + ADR-004: `edges_carried` counts only `true` returns from `write_graph_edge`; UNIQUE conflict (`false`, Ok) is not counted; carry loop captures the bool that `validate_and_write_edges` discards. |
| **SR-03** (eligibility filter drift) | ADR-002: predicate expressed once, at SQL level in `query_outgoing_edges`, mirroring `query_incoming_edges`; documented superset rationale so it cannot be mistaken for drift. |
| **SR-04** (no-ceiling safety) | ADR-002: invariant stated — "no ceiling" valid only while eligibility = agent-declared-only; any future defense is observability-only, never truncating. |
| **SR-05** (doc/ack coupling) | AC-10 + AC-11 kept coupled; `edges_carried` ack is what makes docs non-load-bearing. (Spec concern; noted, not an ADR.) |
| **SR-06** (`Contradicts` double-touch) | ADR-005: outgoing-carry (8b′) and incoming-redirect (8c) act on disjoint edge sets (A's outgoing vs A's incoming); a self-loop A→A is impossible (self-ref forbidden). No pair is touched by both. |
| **SR-07** (tick-window staleness) | Carried edges visible to DB reads immediately, to BFS path-mode after next tick (lesson #4526). Path-mode tests must tick/drain first. (Spec/test concern.) |
| **SR-08** (shed targets Active new entry) | Documented (AC-05/AC-10): shed via `context_edge remove/redirect` against B (Active), not A (Deprecated). (Doc concern.) |

## Open Questions

- **O-1 (build surface):** Does an index on `graph_edges.source_id` exist? `query_incoming_edges`
  relies on `idx_graph_edges_target_id`. The carry query filters on `source_id`; the tick's
  full-table scan never needed a source index. If absent, the developer should confirm whether
  one is warranted (carry runs inline on the correction path, one query per correction — likely
  fine without, but verify and note). **For the developer to resolve during delivery; not a
  blocker.**
- **O-2 (module split):** Whether `query_outgoing_edges` lands in `read.rs` or a new
  `read_outgoing.rs` depends on `read.rs`'s current line count against the 500-line rule. The
  query is ~35 lines mirroring `query_incoming_edges`; **developer decides at implementation
  time** based on the live line count.
