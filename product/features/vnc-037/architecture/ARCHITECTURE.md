# vnc-037 — Architecture: A next-hop navigation affordance — surface an entry's most-relevant typed edges on `context_get`

> **REVISED under the next-hop reframe.** Read-path only. No schema migration.
> On every `context_get`, surface a **ranked, capped (≤3)** set of an entry's depth-1
> typed edges as a *next-hop affordance* — not an edge dump. **Ranking is the core**
> (authored-first, then inferred by target-entry confidence) and **symmetric edges are
> canonicalized to a single `↔` edge in SQL before ranking and counting.**
> Design decisions D-01..D-10 in the REVISED SCOPE.md are LOCKED inputs.

## What changed in this revision

The prior architecture surfaced an unranked, unbounded depth-1 edge set capped at 10 in
Rust. The reframe makes it a *navigation affordance*: at most 3 high-value next hops,
chosen by an explicit rule, with honest uncapped totals. Three structural changes flow
from that:

1. **Rank-and-limit moves into SQL.** A 1000-edge hub must return **3 rows + two counts**,
   never 1000 rows into memory (SR-14). The plain `query_direct_neighbors(…, &[], Both)`
   is no longer the reuse target on its own — it returns unranked, un-canonicalized,
   unbounded rows.
2. **Symmetric-edge canonicalization is a hard blocker (SR-08).** `Contradicts` / `CoAccess`
   / `Informs` store as two reciprocal rows; `Both` does `outgoing.extend(incoming)` with
   no dedup. They must collapse to one `↔` edge **in SQL, before `ORDER BY…LIMIT` and
   before `COUNT(*)`** — display and totals both dedup.
3. **A ranking rule with a precise `ORDER BY`.** Authored edges (`source='agent'`) fill
   slots first; inferred fill the remainder only if authored < 3, ranked by **target-entry
   `entries.confidence`** (not `graph_edges.weight` — frozen per ass-079). With only 3
   slots, *which 3* is the feature (D-09).

ADRs revised: **ADR-001** (SQL rank-and-limit + split COUNT), **ADR-002** (reinforced
under cap-3), **ADR-004** (extended: confidence JOIN + canonicalization on the read
query), **ADR-005** (markdown sub-split dropped, `↔` glyph, symmetric-once totals).
ADR-003 (serializer seam) is **unchanged**. New: **ADR-006** (ranking rule),
**ADR-007** (symmetric canonicalization).

## System Overview

`context_get` is the richest single read in Unimatrix: an agent studying one entry. Today
it returns the entry's fields and stops — relationships are invisible at the point of
consumption, and the author-asserted-edge convention (Prerequisite / Contradicts /
Supports) has no feedback loop. vnc-037 closes that loop by surfacing, **by default**, a
**ranked handful** of the entry's depth-1 typed edges — a pointer to where to read next.

This feature is **purely a read-path consumer**. No new edge type, no storage change, no
traversal. It threads four existing seams together:

1. **The live depth-1 neighbor query** (`unimatrix-store`) — the live-SQL path that
   `context_graph` neighbors uses at depth=1, reading `graph_edges` directly so a
   just-written edge is visible immediately (no in-memory snapshot staleness — ADR #4479).
   vnc-037 issues a **ranked/limited/canonicalized variant** of this query plus a separate
   **split `COUNT(*)`** (ADR-001), not the plain unbounded call.
2. **`entries.confidence`** — a `REAL NOT NULL DEFAULT 0.0` column directly on `entries`
   (`db.rs:549`), the cached Bayesian Beta-Binomial composite. **JOINed** into the read
   query as the inferred-edge rank key (ADR-006).
3. **The shared entry serializer** (`entry_to_json` / `format_entry_markdown_section` /
   `format_single_entry`) — shared by search/lookup/store/correct. vnc-037 adds an
   **optional `edges` capability** with a `None ⇒ key absent` invariant so every
   non-opting caller's payload stays byte-identical (ADR-003).
4. **A batched title join** — one `SELECT id, title FROM entries WHERE id IN (…)`
   (precedent: `fetch_nodes_batch`) resolving the ≤3 displayed `target_title`s in one
   round trip.

### The DISCOVERY-LIST boundary (human-directed guardrail)

The single most important architectural constraint: **`context_get` edges are a discovery
list, not a detail view.** The per-edge payload is exactly
`{edge_type, direction, target_id, target_title, authored}` — only enough for an agent to
decide *whether to go read a related entry*. **The cap of 3 sharpens this boundary, not
relaxes it**: with only 3 slots, every byte of per-edge enrichment is wasted on a list
whose only job is "which one should I open next?" No metadata, weight, depth, raw `source`
string, or `source_id` is added (ADR-002).

`target_id`s surfaced on get are the **entry points** back into `context_graph`, which is
the detail/traversal tool.

| Tool | Role | Edge shape |
|------|------|-----------|
| `context_get` | Thin **ranked ≤3** pointer list — "which few are worth reading next?" | `{edge_type, direction, target_id, target_title, authored}` |
| `context_graph` | Detail + multi-hop traversal tool | `EdgeRecord { source_id, target_id, relation_type, direction, depth, metadata }` |

This boundary is enforced by **ADR-002** (the get edge shape is a deliberate *projection*
of `EdgeRecord` — fields dropped, not added) and is the reason no enrichment field may be
introduced on the get path without a new ADR.

## Component Breakdown

```
context_get handler  (mcp/tools.rs:924)
  │  resolve include_edges (D-01): None | Some(true) ⇒ surface; Some(false) ⇒ skip all
  │  on surface:
  │    1. RANKED select (store, live SQL on read_pool_server):
  │         canonicalize symmetric → one ↔ row (D-10, ADR-007)
  │         LEFT JOIN entries t ON t.id = <other endpoint>   (rank key + dangling-safe)
  │         ORDER BY (source='agent') DESC, t.confidence DESC NULLS LAST, target_id ASC
  │         LIMIT ? (← GET_EDGE_DISPLAY_LIMIT, not literal 3)  (D-05/D-09, ADR-001/006)
  │       → ≤3 RawEdgeRow{…, source, target_confidence}
  │    2. SPLIT COUNT(*) (store, live SQL) — post-canonicalization, three buckets:
  │         inbound / outbound / both totals, ↔ counted once in `both` (D-05/D-10, ADR-001/007)
  │         + digest-only authored tally over the full set (ADR-005 2026-06-16)
  │    3. batch title join for the ≤3 displayed targets: SELECT id,title … WHERE id IN(…)
  │    4. project RawEdgeRow → GetEdge { edge_type, direction(→|←|↔), target_id,
  │                                      target_title, authored }
  │    5. assemble EdgesView { edges (≤3), totals { inbound, outbound, both }, authored_total }
  │  else (Some(false)): skip steps 1–5 entirely; edges = None
  │
  └─► format_single_entry(entry, format, edges: Option<&EdgesView>)   ← serializer seam
         None       ⇒ no `edges` key / no Related section (byte-identical)  [D-07, ADR-003]
         Some(view) ⇒ render per format (summary / markdown / json)         [D-08, ADR-005]
```

| Component | Responsibility | Location | Change |
|-----------|---------------|----------|--------|
| Ranked neighbor query | Live depth-1 SQL: canonicalize symmetric, JOIN target confidence, `ORDER BY (source='agent') DESC, confidence DESC`, `LIMIT ?` bound to `GET_EDGE_DISPLAY_LIMIT`; `Supersedes` excluded at SQL | `unimatrix-store/src/graph_queries*.rs` | **New ranked variant** (sibling to `query_direct_neighbors`); does **not** mutate the plain function's contract |
| `GET_EDGE_DISPLAY_LIMIT` | Single named cap constant (`= 3`, `i64`); SQL `LIMIT` binds it, render `…and N more` threshold references it, tests seed/assert relative to it | `unimatrix-store/src/read.rs` (re-exported via `lib.rs`) | **New const**; one-line retune; decoupled from uncapped totals (ADR-006) |
| Split edge COUNT | Live depth-1 SQL: `COUNT(*)`/`SUM(CASE…)` split inbound/outbound/both + digest-only authored tally, **post-canonicalization** (↔ once in `both`) | `unimatrix-store/src/graph_queries*.rs` | **New** |
| `RawEdgeRow` | Pre-direction graph_edges row | `unimatrix-store/src/graph_queries.rs:73` | **Additive**: `pub source: String`; ranked variant also carries `target_confidence: Option<f64>` |
| neighbor SELECT (plain) | shared with `context_graph` neighbors | `graph_queries_neighbors.rs` | **Additive**: add `source` column only (ADR-004); rank/JOIN/canonicalization live in the **new variant**, never on the shared path |
| `GetParams` | `context_get` params | `mcp/tools.rs:243` | **Additive**: add `include_edges: Option<bool>` |
| Edge assembly | Project rows → get-edge view, batch titles, build totals | `mcp/tools.rs` handler (or sibling `mcp/get_edges.rs` if file nears 500 lines) | **New** |
| `GetEdge` / `EdgeTotals` / `EdgesView` | The thin get-edge vocabulary | `mcp/response/` (new small module or in `entries.rs`) | **New** |
| Serializer seam | Optional `edges` arg, `None ⇒ key absent` | `format_single_entry`, `entry_to_json`, `format_entry_markdown_section` | **Signature extension** (ADR-003) |

> **Shared-path safety (SR-02).** The plain `query_direct_neighbors` / `neighbors_sql`
> path that `context_graph` uses gains **only** the `source` column (ADR-004). The
> rank/JOIN/canonicalization/LIMIT logic lives in a **separate ranked variant** so the
> neighbors contract is untouched and the canonicalization (a get-only concern) never
> leaks into `context_graph`'s edge shape (SR-06).

## Component Interactions

### Read flow (opt-in path, the default)

1. Handler resolves `include_edges`: `None | Some(true) ⇒ surface`, `Some(false) ⇒ skip`.
2. On surface, issue the **ranked select** against `read_pool_server()`. The query
   (ADR-001/006/007):
   - **Canonicalizes** symmetric types (`Contradicts`/`CoAccess`/`Informs`) to one logical
     edge before any ordering — the reciprocal B→A row is folded into the A→B row, marked
     `↔` (ADR-007). Asymmetric types (`Prerequisite`/`Supports`) and all single-row types
     pass through unchanged with a meaningful `→`/`←`.
   - Inherits the existing `relation_type != 'Supersedes'` filter via the empty-type branch
     (ADR #4461 / D-04) — supersession de-dup is free.
   - `LEFT JOIN entries t ON t.id = <the OTHER endpoint>` for the rank key; `LEFT` (not
     inner) so a dangling target is **retained** with `target_confidence = NULL` (D-02,
     SR-11).
   - `ORDER BY (source='agent') DESC, t.confidence DESC NULLS LAST, target_id ASC LIMIT ?`
     — `?` bound to `GET_EDGE_DISPLAY_LIMIT` (the single named cap constant, never a literal
     3); authored-first, then inferred by target confidence, deterministic tiebreak (D-09).
   - A non-existent id returns an empty result, not an error.
3. Issue the **split `COUNT(*)`** against `read_pool_server()` — three counts (inbound /
   outbound / both) plus a digest-only authored tally, computed over the **same
   canonicalized** edge set, so a `↔` edge counts once (in `both`), never twice and never
   folded into inbound (D-05/D-10; ADR-005 three-bucket contract). This is a cheap aggregate
   over the indexed neighbor predicate; it never materializes rows.
4. Collect the **≤3** displayed `target_id`s; one batched `IN (…)` title query against
   `read_pool_server`. Build a `HashMap<u64, String>`. Unresolved id ⇒ `target_title:
   null` (edge retained, D-02). Only the displayed ≤3 need titles — the uncapped set is
   never title-resolved.
5. Project each ranked `RawEdgeRow` → `GetEdge`:
   - `edge_type` = `relation_type`
   - `direction` = `↔` for canonicalized symmetric types; otherwise `→`/`←` (`outbound` if
     the reader is the source endpoint, else `inbound`) — the D-02 direction-semantics fix
   - `target_id` = the *other* endpoint
   - `target_title` = title-map lookup (Option)
   - `authored` = `source == "agent"` (D-03)
6. Assemble `EdgesView { edges (≤3), totals { inbound, outbound, both }, authored_total }`;
   pass `Some(view)` into `format_single_entry`. On opt-out, pass `None`.

> **Why two queries, not one.** A single windowed query could in principle produce both
> the ranked top-3 and the totals, but a separate `COUNT(*)` is simpler, cheaper on the
> common path, and keeps the rank query's `LIMIT` honest (the count is **uncapped** — never
> affected by `GET_EDGE_DISPLAY_LIMIT`). Both are bounded SQL over the indexed neighbor predicate (ADR-001) — neither
> pulls the full fan-out into memory (SR-14).

### Serializer seam (the byte-identity boundary — SR-01) — UNCHANGED

`format_single_entry` gains a third parameter `edges: Option<&EdgesView>`. Only
`context_get` ever passes `Some`. `entry_to_json`'s signature is **unchanged**; the `edges`
array + `edge_totals` object are injected by the get path after the base object is built.
The `### Related` section is appended by the get handler **after**
`format_entry_markdown_section` returns. The summary digest is appended by the get path
only. The invariant — **`None ⇒ key absent / section absent`** — is structural (the key is
never added, the section never appended for list views), not a runtime convention a future
edit can silently break (the #3449 lesson). Full detail in **ADR-003** (unaffected by the
reframe).

## Technology Decisions (see ADRs)

| ADR | Decision | Status this revision |
|-----|----------|----------------------|
| ADR-001 | Reuse the live depth-1 neighbor seam, but **rank-and-limit in SQL** (`ORDER BY (source='agent') DESC, t.confidence DESC LIMIT 3`) + a separate split `COUNT(*)` for totals (post-canonicalization); live SQL on `read_pool_server`; opt-out skips both queries | **CHANGED** (#5009 corrected) |
| ADR-002 | The get edge shape is a deliberate **projection** of `EdgeRecord` — discovery list, not detail view; **cap-3 reinforces the no-enrichment boundary** | **MINOR UPDATE** (#5010 corrected) |
| ADR-003 | Serializer seam: get-only edge rendering with a `None ⇒ key absent` byte-identity invariant; `entry_to_json` signature unchanged | **UNCHANGED** (#5011 confirmed) |
| ADR-004 | `source` added **additively** to `RawEdgeRow` + neighbor SELECT (no migration); the **ranked variant additionally** `LEFT JOIN`s `entries.confidence` and applies canonicalization — still additive, still no DDL; `context_graph` neighbors unaffected | **EXTENDED** (#5012 corrected) |
| ADR-005 | Rendering: JSON nested `edge_totals` (now **three buckets** `{inbound, outbound, both}` — `↔` in its own `both` bucket, 2026-06-16); **markdown author/inferred sub-split DROPPED**; `↔` glyph; symmetric counted **once**; locked summary-digest byte form + full-set authored tally | **UPDATED** (#5013; three-bucket amend 2026-06-16 — Unimatrix re-sync deferred) |
| ADR-006 | **Ranking rule (D-09):** authored-first, then inferred by `entries.confidence` (Beta-Binomial); exact `ORDER BY`; weight NOT used (ass-079 frozen / first-write-wins). **Display cap = one named constant** `GET_EDGE_DISPLAY_LIMIT` (=3) in `read.rs`, bound into the SQL `LIMIT ?`; render + tests reference it; decoupled from uncapped totals | **NEW** (#5018 corrected — cap-as-constant added) |
| ADR-007 | **Symmetric canonicalization (D-10, SR-08 blocker):** `Contradicts`/`CoAccess`/`Informs` collapse to one `↔` in SQL **before** ranking AND counting; `Prerequisite`/`Supports` stay asymmetric | **NEW** |

## Integration Points

- **Depends on** the live depth-1 neighbor seam (`unimatrix-store`) — a **new ranked
  variant** is added beside `query_direct_neighbors`; the plain function gains only the
  `source` column (ADR-004), keeping `context_graph` neighbors byte-stable.
- **JOINs** `entries.confidence` (`db.rs:549`) as the inferred rank key — no new table, no
  migration; the column is read-only here (ADR-006).
- **Shares the `RawEdgeRow` type** with `context_graph` neighbors mode — the additive
  `source` field must not alter its behavior (verify empirically, ADR-004 / SR-02).
- **Depends on** the shared entry serializer in `mcp/response/` — extended with the
  byte-identity invariant (ADR-003, unchanged).
- **Reflects** vnc-035 carry-forward state automatically (reads live `graph_edges`). Per
  ADR-002 vnc-035 (#4984), carry-forward and `context_edge` writes are **agent-declared
  edges only** stamped `source='agent'`, so a carried-forward edge classifies as
  **authored** and keeps slot priority (SR-10 — locked by test in AC-10).
- **Makes observable** (downstream, not blocked on) #744 redirect cap and #745 orphans via
  the inbound direction-split count (now symmetric-once).

## Integration Surface

### Existing interfaces (consumed / extended)

| Integration Point | Type / Signature | Source |
|-------------------|------------------|--------|
| `query_direct_neighbors` (plain — gains `source` only) | `async fn(pool: &SqlitePool, id: u64, edge_types: &[&str], direction: NeighborDirection) -> Result<Vec<RawEdgeRow>, StoreError>` | `unimatrix-store/src/graph_queries.rs:200` |
| `NeighborDirection` | `enum { Outgoing, Incoming, Both }` | `unimatrix-store/src/graph_queries.rs:51` |
| `RawEdgeRow` (before) | `struct { source_id: u64, target_id: u64, relation_type: String }` | `unimatrix-store/src/graph_queries.rs:73` |
| `map_edge_row` | `fn(&SqliteRow) -> Result<RawEdgeRow, StoreError>` | `unimatrix-store/src/graph_queries_neighbors.rs:95` |
| neighbor SELECT (plain, ×4) | `SELECT source_id, target_id, relation_type FROM graph_edges WHERE …` | `graph_queries_neighbors.rs:20,34,61,75` |
| `entries.confidence` (rank key) | `confidence REAL NOT NULL DEFAULT 0.0` on `entries` (cached Bayesian Beta-Binomial composite) | `unimatrix-store/src/db.rs:549`; `unimatrix-engine/src/confidence.rs` |
| `graph_edges.source` (provenance) | `source TEXT` — `'agent'` (authored) vs `behavioral`/`co_access`/`S8`/cosine (inferred) | `unimatrix-store/src/db.rs:960` |
| symmetric reverse-row writers (canonicalization targets) | `Contradicts` (`edge_write.rs:211-223`), `CoAccess` (`graph_enrichment_tick.rs:442-478`), `Informs` (`behavioral_signals.rs:244-308`) | per file |
| `Store::read_pool_server` | `fn(&self) -> &SqlitePool` | `unimatrix-store/src/db.rs:296` |
| `format_single_entry` (before) | `fn(entry: &EntryRecord, format: ResponseFormat) -> CallToolResult` | `mcp/response/entries.rs:13` |
| `entry_to_json` | `fn(entry: &EntryRecord) -> serde_json::Value` — **signature unchanged** | `mcp/response/mod.rs:121` |
| `format_entry_markdown_section` | `fn(num, entry, similarity: Option<f64>) -> String` — **unchanged** | `mcp/response/mod.rs:160` |
| `EdgeRecord` (the projection source) | `struct { source_id, target_id, relation_type, direction: String, depth: u8, metadata: Option<Value> }` | `mcp/graph_read.rs:135` |
| batch-title precedent | `SELECT … FROM entries WHERE id IN (?, …)` positional binds | `mcp/graph_read_subgraph.rs:568` (`fetch_nodes_batch`) |
| `GetParams` (before) | `struct { id, agent_id, format, feature, helpful, session_id }` | `mcp/tools.rs:243` |
| carry-forward / `context_edge` source stamp | agent-declared edges only → `source='agent'` | ADR-002 vnc-035 (#4984); `context_edge` write path |

### New interfaces (introduced by vnc-037)

| Interface | Definition | Notes |
|-----------|------------|-------|
| `RawEdgeRow.source` | `pub source: String` (added field) | Additive; populated from `graph_edges.source` (ADR-004) |
| `RawEdgeRow.target_confidence` | `pub target_confidence: Option<f64>` (added field, populated by the **ranked variant** only) | `None` for dangling targets (LEFT JOIN); used only for the inferred tiebreak, not surfaced (ADR-006) |
| neighbor SELECT (plain, after) | `SELECT source_id, target_id, relation_type, source FROM graph_edges WHERE …` (both empty-type and IN-type branches, both directions) | Read-path only, no DDL (ADR-004) |
| `GET_EDGE_DISPLAY_LIMIT` | `pub const GET_EDGE_DISPLAY_LIMIT: i64 = 3` — display cap, `i64` for the sqlx `LIMIT ?` bind | **New** in `unimatrix-store/src/read.rs` (below `CO_ACCESS_GRAPH_MIN_COUNT`), re-exported from `lib.rs` `pub use read::{…}`; single source of truth for the cap (ADR-006; convention ADR-002 crt-034 / ADR-008 vnc-015) |
| ranked neighbor query | `async fn(pool, id, direction=Both) -> Result<Vec<RawEdgeRow>, StoreError>` — canonicalizes symmetric, `LEFT JOIN entries t`, `ORDER BY (source='agent') DESC, t.confidence DESC NULLS LAST, target_id ASC LIMIT ?` with `?` **bound to `GET_EDGE_DISPLAY_LIMIT`** (never a literal 3) | **New** store fn; get-only; does not change the plain function (ADR-001/006/007) |
| split edge count query | `async fn(pool, id) -> Result<EdgeCountSplit{inbound:usize, outbound:usize, both:usize, authored:usize}, StoreError>` — `COUNT(*)`/`SUM(CASE…)` over the **canonicalized** neighbor set, split into three direction buckets + a digest-only authored tally | **New** store fn; `↔` counted once in `both` (ADR-001/007; three-bucket per ADR-005 2026-06-16) |
| `GetParams.include_edges` | `#[serde(default)] pub include_edges: Option<bool>` | `None`/`Some(true)` ⇒ surface; `Some(false)` ⇒ suppress (D-01, AC-11) |
| `GetEdge` | `{ edge_type: String, direction: &'static str ("inbound"/"outbound"/"both"), target_id: u64, target_title: Option<String>, authored: bool }` | Thin pointer payload (D-02). `"both"` renders `↔`. **No** `source_id`, `depth`, `metadata`, `source`, `weight`, or `target_confidence` |
| `EdgeTotals` | `{ inbound: usize, outbound: usize, both: usize }` (uncapped, **post-canonicalization** — ↔ once in `both`) | Three-bucket split, exact (D-05/D-10); JSON nested `edge_totals` (ADR-005 TOTALS BUCKET CONTRACT, 2026-06-16) |
| `EdgesView` | `{ edges: Vec<GetEdge> (≤3), totals: EdgeTotals }` | Passed as `Some` only on opt-in get |
| `format_single_entry` (after) | `fn(entry: &EntryRecord, format: ResponseFormat, edges: Option<&EdgesView>) -> CallToolResult` | Only `context_get` passes `Some`; all other callers pass `None` (ADR-003) |

### Data flow

```
graph_edges (live SQL)
   │
   ├─ ranked variant: canonicalize symmetric (↔, ADR-007) ─► LEFT JOIN entries.confidence
   │     ─► ORDER BY (source='agent') DESC, confidence DESC NULLS LAST, target_id ASC
   │     ─► LIMIT ? (GET_EDGE_DISPLAY_LIMIT) ──────► Vec<RawEdgeRow{…, source, target_conf}> (≤cap)
   │                                                       │
   │   displayed ≤3 target endpoints ──IN(…) join──► HashMap<id,title>
   │                                                       │
   ├─ split COUNT(*) over canonicalized set ─► EdgeTotals{inbound, outbound, both} + authored_total  (↔ once in both)
   │                                                       │
   │                          project (→|←|↔) + authored  ▼
   │                                       EdgesView { Vec<GetEdge> (≤3), EdgeTotals, authored_total }
   │                                                       │
   context_get only ──Some(view)──► format_single_entry ──► summary | markdown | json
   search/lookup/store/correct ──(no edges arg / None)──► byte-identical payload
   context_get opt-out (Some(false)) ──None──► byte-identical, queries skipped
```

### Error boundaries

- The ranked query, split count, and title batch each return `StoreError` → mapped to
  `ServerError`/`ErrorData` in the handler (same mapping as the existing `entry_store.get`
  call). No `.unwrap()` (workspace rule). Failure posture is OQ-A below.
- **Dangling target** (id in an edge, absent from `entries`): not an error — the `LEFT
  JOIN` yields `target_confidence = NULL` (ranked last via `NULLS LAST`) and the title
  lookup yields `target_title: null`; the edge is **retained** (D-02 / AC-02 / SR-11).
- **NULL confidence ordering** is explicit (`NULLS LAST`) so dangling/cold-start targets
  rank deterministically rather than sorting unpredictably (SR-11).
- Non-existent entry id: the primary `entry_store.get(id)` already errors before edges run.

## Resolved Open Questions

### OQ-01 — JSON totals shape: **nested object** `"edge_totals": {"inbound": N, "outbound": M, "both": S}` (ADR-005)

Nested-object shape matches the existing house style (`co_access`, `correction_chains`,
`security`). **AMENDED 2026-06-16:** the object now carries **three** keys —
`{inbound, outbound, both}` — a `↔` symmetric edge counts once in its own `both` bucket,
**not** folded into inbound (see the ADR-005 TOTALS BUCKET CONTRACT; deciding factors =
honesty + the #744 clean asymmetric-inbound observability signal). Both `edges` and
`edge_totals` appear iff edges were surfaced; zero-edge ⇒ `edges: []`,
`edge_totals: {"inbound":0,"outbound":0,"both":0}` (D-06). On opt-out neither key appears
(D-07). Counts are **post-canonicalization** — `↔` contributes once to `both`.

### OQ-02 — Markdown grouping: **drop the author/inferred sub-split entirely** (ADR-005)

**CHANGED by the reframe.** The prior resolution omitted *empty* sub-group headers. Under
cap-3 with authored-first ranking (D-09), the **sub-grouping itself is dropped**: the
ranking already front-loads authored edges, so a separate `**Author-asserted**` /
`**Inferred**` split is redundant. Markdown renders a single flat ranked list of ≤3 lines.
See ADR-005 for the exact shape and the `↔` glyph.

### OQ-03 — Internal-caller default for `include_edges`: **default OFF for programmatic/internal callers; default ON only for the agent-facing MCP tool**

**Recommendation:** internal/programmatic call sites that fetch an entry by ID but never
present the next-hop affordance to a reading agent should pass **`include_edges: Some(false)`**.
Specifically:

- **The hook / write-back path** and **the briefing pipeline's by-ID fetches** and
  **any by-ID loop fetch** (bulk machine reads) → `Some(false)`. They pay the ranked-select
  + split-count + confidence-JOIN cost (AC-12) for an affordance no human reads. Opt-out
  makes them behave exactly as pre-vnc-037 (ADR-001: opt-out skips both queries entirely).
- **The agent-facing `context_get` MCP tool** (an agent studying one entry) → leave
  `include_edges` absent (`None`) ⇒ **default-on**. This is the whole point of the feature
  and where the feedback loop lives.

Rationale: the affordance is *for an agent reading an entry, not a machine bulk-fetching*
(SCOPE OQ-03). Defaulting internal callers off directly relieves SR-12 (latency on the
hottest read) without weakening the loop, because those callers never consumed the
affordance anyway. The cost is a small divergence from a single default-on code path — a
handful of call sites pass an explicit `Some(false)`. The single default-on path is
preserved *at the tool boundary* (the MCP surface stays default-on); only internal
programmatic callers opt out. The spec writer should enumerate the exact internal call
sites (the hook path, the briefing pipeline, by-ID loop fetches named in SCOPE OQ-03) and
make each `Some(false)` an asserted test, so the opt-out is verified, not assumed.

This recommendation is **advisory to the human/spec** — D-01 keeps the field additive and
default-on at the type level; OQ-03 only decides which *callers* set it false.

## Open Questions (for human / downstream)

- **OQ-A (degrade vs fail on edge-query error).** If the ranked query, split count, or
  title join fails but the primary `entry_store.get` succeeded, should the get (a) fail the
  whole call, or (b) return the entry with edges omitted plus a soft note? Architecture
  leans (a) **fail**, matching how the handler treats the primary read and keeping behavior
  simple/testable; a degraded-but-noted path is viable if edge-surfacing must never
  compromise the core read. Flagging for the spec writer / human — not blocking.
- **OQ-B (file size).** The ranked variant + split count in `unimatrix-store`, and the get
  handler's edge-assembly + `GetEdge`/`EdgesView`/render code in `mcp/`, may push
  `graph_queries*.rs`, `tools.rs`, or `response/entries.rs` toward the 500-line limit. The
  spec should pre-authorize sibling modules (e.g. a `graph_queries_ranked.rs` in the store
  and `mcp/get_edges.rs` / `response/edges.rs` in the server) so the implementer splits
  cleanly rather than discovering the limit mid-build.
- **OQ-C (AC-12 measured baseline — SR-12).** AC-12's proposed ≤5 ms p50 / ≤15 ms p95 is
  **unbacked** until measured. The spec must require an **edge-free `context_get` baseline**
  (default-off) measured on a representative store *including a high-degree node* before the
  numbers are locked, since the ranked select + confidence JOIN + split count land on the
  hottest read and `context_get` also feeds the co-access loop (cost compounds). The
  rank-and-limit-in-SQL design (ADR-001) is what makes the budget reachable on hubs; the
  measurement confirms it. Not blocking architecture; blocking the AC-12 number lock.
