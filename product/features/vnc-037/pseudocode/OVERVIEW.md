# vnc-037 Pseudocode — OVERVIEW

A next-hop navigation affordance: on `context_get`, surface a **ranked, capped (≤3)** set of
an entry's depth-1 typed edges plus **honest, uncapped** inbound/outbound totals. Read-path
only — no schema migration, no multi-hop. This file fixes the shared vocabulary, the data flow
across component boundaries, the canonicalization CASE, the ranked SQL, and the build order.
Per-component pseudocode lives in the sibling files.

## Components (8)

| Component | File | Crate / location |
|-----------|------|------------------|
| store-display-cap-constant | `store-display-cap-constant.md` | `unimatrix-store/src/read.rs` (+ `lib.rs` re-export) |
| store-neighbor-source | `store-neighbor-source.md` | `unimatrix-store/src/graph_queries.rs`, `graph_queries_neighbors.rs` |
| store-ranked-query | `store-ranked-query.md` | `unimatrix-store/src/graph_queries_ranked.rs` (new) |
| store-split-count | `store-split-count.md` | `unimatrix-store/src/graph_queries_ranked.rs` (new, same module) |
| get-edge-vocabulary | `get-edge-vocabulary.md` | `unimatrix-server/src/mcp/response/edges.rs` (new) |
| serializer-seam | `serializer-seam.md` | `unimatrix-server/src/mcp/response/entries.rs`, `edges.rs` |
| get-edge-assembly | `get-edge-assembly.md` | `unimatrix-server/src/mcp/get_edges.rs` (new) |
| get-params | `get-params.md` | `unimatrix-server/src/mcp/tools.rs` |

No split/merge from the brief's Component Map. `store-ranked-query` and `store-split-count`
are distinct functions but co-located in the same new file `graph_queries_ranked.rs` (they MUST
share the canonicalization CASE; see ADR-007 — drift between them re-introduces a double-count).

## Shared Types (definitions binding on all component files)

```
// --- unimatrix-store ---

// read.rs — single source of truth for the display cap (ADR-006 #5054, FR-18/C-12).
// i64 to match the sqlx `LIMIT ?` bind convention (parallel to CO_ACCESS_GRAPH_MIN_COUNT).
// Totals (COUNT) are UNCAPPED and never reference this constant; retune is a one-line edit.
pub const GET_EDGE_DISPLAY_LIMIT: i64 = 3

// graph_queries.rs — RawEdgeRow gains two additive fields (ADR-004).
struct RawEdgeRow {
    source_id: u64                       // existing
    target_id: u64                       // existing
    relation_type: String                // existing
    source: String                       // ADDITIVE: graph_edges.source; 'agent' = authored. Populated by ALL paths (plain + ranked).
    target_confidence: Option<f64>       // ADDITIVE: ranked variant ONLY. None on the plain path AND for dangling targets (LEFT JOIN). Inferred-tiebreak input, NEVER surfaced.
}

// graph_queries_ranked.rs — split-count return type.
struct EdgeCountSplit { inbound: usize, outbound: usize }   // post-canonicalization, ↔ counted once

// --- unimatrix-server: mcp/response/edges.rs ---

// The thin discovery-list payload (ADR-002 guardrail — EXACTLY these 5 fields).
struct GetEdge {
    edge_type: String                    // = RawEdgeRow.relation_type
    direction: &'static str              // "inbound" | "outbound" | "both"   ("both" renders ↔)
    target_id: u64                       // the OTHER endpoint (entry point back into context_graph)
    target_title: Option<String>         // None when target unresolved (dangling — retained, DNB-1)
    authored: bool                       // RawEdgeRow.source == "agent"
    // NO source_id, depth, metadata, source string, weight, or target_confidence — enrichment forbidden.
}

struct EdgeTotals { inbound: usize, outbound: usize }        // uncapped, ↔ counted once (= EdgeCountSplit projected)
struct EdgesView  { edges: Vec<GetEdge> /* ≤cap */, totals: EdgeTotals }

// --- unimatrix-server: mcp/tools.rs ---
struct GetParams { /* …existing fields… */ include_edges: Option<bool> /* #[serde(default)] */ }
```

## The canonicalization CASE (↔) — D-10 / ADR-007 (BLOCKER)

Three relation types store as **two reciprocal rows** (A→B and B→A): `Contradicts`, `CoAccess`,
`Informs`. They MUST collapse to **one** logical `↔` row **in SQL, BEFORE `ORDER BY…LIMIT` AND
BEFORE `COUNT(*)`** — on BOTH the display set and the totals. Asymmetric single-row types
(`Prerequisite`, `Supports`) pass through unchanged with a meaningful `→`/`←`. The symmetric
type list is a hard-coded set (A2) — adding a future symmetric type without updating it
double-counts (documented hazard).

Canonicalization is done by a **shared SQL fragment** used identically by the ranked select and
the split count. Strategy (anchor = lower endpoint id):

```
-- A neighbor query for anchor `?1` is the UNION of the outgoing and incoming legs.
-- For SYMMETRIC types, keep exactly one row per {relation_type, unordered endpoint pair}:
--   keep the row where MIN(source_id,target_id) < MAX(...) is represented once.
-- For ASYMMETRIC types, keep every (single) row as-is.
--
-- Practical form: build a unified row set with the OTHER endpoint and a canonical-pair key,
-- then DISTINCT/GROUP on (relation_type, min_endpoint, max_endpoint) for symmetric types only.

WITH nbr AS (
    -- outgoing leg: anchor is source_id, other endpoint is target_id
    SELECT source_id, target_id,
           target_id              AS other_id,
           relation_type, source,
           'outbound'             AS leg
    FROM graph_edges
    WHERE source_id = ?1 AND relation_type != 'Supersedes'
    UNION ALL
    -- incoming leg: anchor is target_id, other endpoint is source_id
    SELECT source_id, target_id,
           source_id              AS other_id,
           relation_type, source,
           'inbound'              AS leg
    FROM graph_edges
    WHERE target_id = ?1 AND relation_type != 'Supersedes'
),
canon AS (
    SELECT
        relation_type, source, other_id,
        -- symmetric ⇒ "both" (↔); asymmetric ⇒ its leg's direction
        CASE WHEN relation_type IN ('Contradicts','CoAccess','Informs')
             THEN 'both' ELSE leg END                       AS direction,
        -- canonical-pair key: unordered {anchor, other} for symmetric dedup
        MIN(?1, other_id) AS pair_lo,
        MAX(?1, other_id) AS pair_hi
    FROM nbr
),
-- collapse the two reciprocal rows of a symmetric edge to ONE; asymmetric rows untouched.
deduped AS (
    SELECT relation_type, source, other_id, direction
    FROM canon
    GROUP BY relation_type, pair_lo, pair_hi,
             CASE WHEN direction = 'both' THEN 1 ELSE other_id END
    -- symmetric rows of a pair share (relation_type,pair_lo,pair_hi,1) ⇒ one group.
    -- asymmetric rows group by their distinct other_id ⇒ never merged across distinct neighbors.
)
```

`deduped` is the **canonicalized set**. Both the ranked select and the split count build on it.
The `?1` anchor, the `IN (…)` symmetric list, and the `!= 'Supersedes'` filter are **static SQL**
(never assembled from input); `?1` is a positional bind. See `store-ranked-query.md` for the full
ranked statement and `store-split-count.md` for the count over the same `deduped` CTE.

> Anchor convention for symmetric direction bucketing: a `↔` row's `direction='both'`. The split
> count must attribute each `↔` to exactly ONE direction bucket (convention: **inbound** — see
> `store-split-count.md`), so it is counted once, not once per direction.

## The ranked SQL (locked — D-09 / ADR-006, C-8)

Applied to the **canonicalized** set (`deduped` above), LEFT JOIN target confidence, then:

```
SELECT d.relation_type, d.source, d.other_id AS target_id, d.direction,
       t.confidence AS target_confidence
FROM deduped d
LEFT JOIN entries t ON t.id = d.other_id          -- LEFT: dangling target retained (D-02/SR-11)
ORDER BY (d.source = 'agent') DESC,               -- 1. authored first (D-09.1)
         t.confidence DESC NULLS LAST,            -- 2. inferred by TARGET confidence (D-09.3); dangling/cold last
         target_id ASC                            -- 3. deterministic tiebreak
LIMIT ?                                            -- ← bound to GET_EDGE_DISPLAY_LIMIT, NEVER a literal 3
```

- `LIMIT ?` is a sqlx bind set to `GET_EDGE_DISPLAY_LIMIT`. No literal 3 in the query string.
- Ranking by `graph_edges.weight` is **prohibited** (frozen / non-discriminating, ass-079).
- `RawEdgeRow.source_id` for the projected row is the anchor; `target_id = other_id`.

## Data flow (across boundaries)

```
context_get handler (tools.rs)
  │  resolve include_edges (get-params): None|Some(true) ⇒ surface ; Some(false) ⇒ skip-all, edges=None
  │  primary read: entry = entry_store.get(id)  ── on Err ⇒ mapped ServerError (existing path)
  │  IF surface:
  │    ─ get-edge-assembly (get_edges.rs):
  │        1. rows  = query_ranked_neighbors(read_pool_server, id)      [store-ranked-query]  → Vec<RawEdgeRow> (≤cap)
  │        2. split = count_neighbors_split(read_pool_server, id)       [store-split-count]   → EdgeCountSplit
  │        3. titles= batch title join over the ≤cap displayed target_ids → HashMap<u64,String>
  │        4. project rows → Vec<GetEdge> (→|←|↔, authored, title-map lookup)  [get-edge-vocabulary]
  │        5. EdgesView { edges, totals: EdgeTotals{split.inbound, split.outbound} }
  │      ── ANY Err in steps 1–3 (post-primary-read) ⇒ SAME ServerError mapping as primary read, RETURNED (FR-19 fail-loud)
  │    edges = Some(&view)
  │  ELSE: edges = None ; steps 1–5 never run (FR-3 — zero query cost)
  │
  └─► format_single_entry(entry, format, edges)   [serializer-seam]
         None       ⇒ no edges key / no Related section (byte-identical to list views)   [ADR-003]
         Some(view) ⇒ render summary | markdown | json                                  [ADR-005]

search/lookup/store/correct ──(call format_single_entry-equivalent paths with edges = None)──► byte-identical
context_graph neighbors ──(query_direct_neighbors plain, gains `source` only)──► EdgeRecord unchanged, no ↔ leak
```

## Sequencing constraints (build order)

1. **store-display-cap-constant** — nothing else compiles against the cap until the const exists.
2. **store-neighbor-source** — `RawEdgeRow` field additions + plain SELECT `source` column.
   (Re-verify `context_graph` neighbors suite green, UNEDITED — SR-02/AC-09.)
3. **store-ranked-query** + **store-split-count** — depend on (1) the const and (2) `RawEdgeRow`.
   Must share the canonicalization CASE.
4. **get-edge-vocabulary** — pure types, no deps beyond `RawEdgeRow` shape knowledge.
5. **serializer-seam** — depends on (4) `EdgesView`.
6. **get-params** — additive field; independent, but consumed by (7).
7. **get-edge-assembly** — depends on (3) store fns, (4) vocabulary, (5) seam, (6) params.

## Non-negotiable invariants (cross-cutting)

- **Cap is display-only**: SQL `LIMIT` and the `…N more` render reference `GET_EDGE_DISPLAY_LIMIT`;
  the split `COUNT(*)` and canonicalization NEVER reference it. No literal `3` at any cap site.
- **Canonicalize before rank AND before count** (BLOCKER); ↔ counted once on BOTH surfaces.
- **Rank-and-limit + count in SQL** — never fetch-all-then-slice/count in Rust (C-7/SR-14).
- **FR-19 fail-loud**: every edge/count/title query returns `Result`, never `.unwrap()`/`expect()`;
  a post-primary-read failure on the default-on path maps to the primary-read `ServerError` and is
  returned. The opt-out path skips the queries and cannot reach this.
- **Serializer byte-identity**: `entry_to_json` / `format_entry_markdown_section` signatures
  UNCHANGED; `None ⇒ key/section absent` is structural (ADR-003).
- **Per-edge payload EXACTLY** `{edge_type, direction, target_id, target_title, authored}` — no
  enrichment (ADR-002 guardrail).
