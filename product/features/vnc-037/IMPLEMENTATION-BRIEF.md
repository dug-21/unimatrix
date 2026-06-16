# vnc-037 — Implementation Brief

A **next-hop navigation affordance**: surface an entry's most-relevant, **ranked, capped
(≤3)** depth-1 typed graph edges on `context_get`, with honest **uncapped** split totals.
Read-path only — no new edge type, no schema migration, no multi-hop.

> **REGENERATED for the next-hop reframe (2026-06-15).** Supersedes the prior edge-dump
> brief. The display cap is **3, not 10**; **ranking (D-09/ADR-006) is the core of the
> feature** — with 3 slots, *which 3* is the whole question; **symmetric edges are
> canonicalized to one `↔` in SQL BEFORE ranking and counting (D-10/ADR-007)** — a blocker;
> a per-call **latency budget** (AC-12) is added; the display cap is a **single named constant**
> `GET_EDGE_DISPLAY_LIMIT` (=3, FR-18/C-12/AC-13/ADR-006 #5054). 19 FR, 14 AC.
>
> **Updated 2026-06-16 — OQ-A RESOLVED = FAIL LOUD (human-directed).** Locked as **FR-19 /
> C-13 / AC-14**: on the default-on path, a post-primary-read edge/count/title failure **fails
> the whole `context_get`** via the primary-read error mapping — no degrade-with-note, no silent
> edge omission (silent omit is indistinguishable from a true zero-edge entry and would poison
> the next-hop signal). Verified by a named **RED** failure-path test. OQ-A moved to Resolved.

## Source Document Links

| Document | Path |
|----------|------|
| Scope | product/features/vnc-037/SCOPE.md |
| Scope Risk Assessment | product/features/vnc-037/SCOPE-RISK-ASSESSMENT.md |
| Specification | product/features/vnc-037/specification/SPECIFICATION.md |
| Architecture | product/features/vnc-037/architecture/ARCHITECTURE.md |
| Risk-Test Strategy | product/features/vnc-037/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/vnc-037/ALIGNMENT-REPORT.md |

ADRs (files):
- product/features/vnc-037/architecture/ADR-001-reuse-direct-neighbors-read-path.md (**CHANGED** — SQL rank-and-limit + split COUNT; #5009 corrected)
- product/features/vnc-037/architecture/ADR-002-get-edge-shape-projection-discovery-list.md (**MINOR UPDATE** — cap-3 reinforces no-enrichment; #5010 corrected)
- product/features/vnc-037/architecture/ADR-003-serializer-seam-none-key-absent.md (**UNCHANGED** — #5011 confirmed)
- product/features/vnc-037/architecture/ADR-004-additive-source-rawedgerow.md (**EXTENDED** — ranked variant adds confidence LEFT JOIN + canonicalization; #5012 corrected)
- product/features/vnc-037/architecture/ADR-005-json-totals-and-empty-subgroup-rendering.md (**UPDATED** — markdown sub-split DROPPED, `↔` glyph, symmetric-once totals; #5013 corrected)
- product/features/vnc-037/architecture/ADR-006-ranking-rule-authored-first-target-confidence.md (**NEW** — the ranking rule; **+ display-cap-as-named-constant `GET_EDGE_DISPLAY_LIMIT`**; #5054 corrected)
- product/features/vnc-037/architecture/ADR-007-symmetric-edge-canonicalization.md (**NEW** — the canonicalization blocker)

## Component Map

The architecture identifies these components. Pseudocode and test-plan file paths are
populated during Session 2 Stage 3a.

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| store-ranked-query (new ranked variant: canonicalize → LEFT JOIN confidence → `ORDER BY…LIMIT ?` bound to `GET_EDGE_DISPLAY_LIMIT`) | pseudocode/store-ranked-query.md | test-plan/store-ranked-query.md |
| store-display-cap-constant (`GET_EDGE_DISPLAY_LIMIT: i64 = 3` in `read.rs`, re-exported via `lib.rs`; single cap source for SQL `LIMIT`, `…N more` render, tests) | pseudocode/store-display-cap-constant.md | test-plan/store-display-cap-constant.md |
| store-split-count (new split `COUNT(*)` over canonicalized set, post-canonicalization) | pseudocode/store-split-count.md | test-plan/store-split-count.md |
| store-neighbor-source (additive `source` on plain `query_direct_neighbors`/`RawEdgeRow`/4 SELECTs) | pseudocode/store-neighbor-source.md | test-plan/store-neighbor-source.md |
| get-edge-assembly (handler: opt-out resolve, projection, batch title join, build `EdgesView`) | pseudocode/get-edge-assembly.md | test-plan/get-edge-assembly.md |
| get-edge-vocabulary (`GetEdge`/`EdgeTotals`/`EdgesView`) | pseudocode/get-edge-vocabulary.md | test-plan/get-edge-vocabulary.md |
| serializer-seam (`format_single_entry` edges arg + 3-format render, `↔` glyph; `…N more` threshold references `GET_EDGE_DISPLAY_LIMIT`, no literal 3) | pseudocode/serializer-seam.md | test-plan/serializer-seam.md |
| get-params (`GetParams.include_edges` + internal-caller opt-out call sites) | pseudocode/get-params.md | test-plan/get-params.md |

### Cross-Cutting Artifacts (populated during Stage 3a)

| Artifact | Path | Consumed By |
|----------|------|-------------|
| Pseudocode Overview | pseudocode/OVERVIEW.md | Stage 3b (all agents), Gate 3a |
| Test Strategy + Integration Plan | test-plan/OVERVIEW.md | Stage 3c (tester), Gate 3a, Gate 3c |

## Goal

On every agent-facing `context_get`, surface a **ranked, capped (≤3)** set of the entry's
depth-1 typed edges (both directions) as a *next-hop affordance* — at most 3 high-value
pointers chosen by an explicit selection rule (D-09), not a complete edge list — reading
**live `graph_edges` via SQL** (immediate freshness on a point read). Always report honest,
**uncapped** totals split inbound/outbound, counting a canonicalized symmetric edge **once**.
The change is surgical: one new SELECT column (no migration), a ranked/limited/canonicalized
read path + a split `COUNT(*)`, one serializer seam; every non-opted-in payload byte-identical.

## THE REFRAME (load-bearing — read first)

The feature is **not an edge dump.** It is a **ranked ≤3 next-hop affordance**. Three
structural consequences govern every downstream decision:

1. **Ranking IS the core (D-09 / ADR-006).** With only 3 display slots, *which 3* is the
   whole feature. The rule: **authored edges fill slots first** (`source = 'agent'`); inferred
   fills the remainder **only if authored < 3**; inferred is ranked by **target-entry
   `entries.confidence`** (the cached Bayesian Beta-Binomial composite), **NOT**
   `graph_edges.weight` (frozen first-write-wins, outcomes ~always `success` per ass-079 → no
   discriminating signal). The exact, locked SQL ordering:
   `ORDER BY (source='agent') DESC, t.confidence DESC NULLS LAST, target_id ASC LIMIT ?`
   via a **LEFT JOIN** on `entries.confidence` (deterministic NULL ordering so dangling/
   cold-start targets are retained and rank last). The `LIMIT ?` binds the single named cap
   constant **`GET_EDGE_DISPLAY_LIMIT`** (FR-18/C-12/ADR-006 #5054) — never a literal `3`.
2. **Symmetric canonicalization is a BLOCKER (D-10 / ADR-007 / SR-08).** `Contradicts`,
   `CoAccess`, `Informs` store as **two reciprocal rows** (A→B and B→A); `Both` does
   `outgoing.extend(incoming)` with **no dedup**. They MUST collapse to **one `↔` edge in
   SQL, BEFORE `ORDER BY…LIMIT 3` AND BEFORE `COUNT(*)`** — both the displayed set and the
   totals dedup. A miss double-renders (consumes 2 of 3 slots) AND double-counts. `direction`
   (`→`/`←`) is meaningful **only** for asymmetric types (`Prerequisite`, `Supports`);
   symmetric types carry `↔` and **no** directional arrow.
3. **Cap is display-only; totals are honest and uncapped (D-05).** The ≤3 cap never touches
   the counts. Totals are exact, split inbound/outbound, computed by a **separate `COUNT(*)`
   post-canonicalization** so a `↔` edge counts once. This is what keeps the visible-empty-box
   feedback loop and #744/#745 inbound-degree observability intact. The cap lives in **one named
   constant `GET_EDGE_DISPLAY_LIMIT`** (=3, FR-18/C-12/ADR-006 #5054), bound by the SQL `LIMIT`
   and the `…N more` render; the **uncapped** `COUNT(*)` totals and `↔` canonicalization never
   reference it, so retuning the cap is a one-line edit that changes only the rendered set size.

## DISCOVERY-LIST GUARDRAIL (human-directed — ADR-002)

`context_get` edges are a **discovery list, not a detail view.** The per-edge payload is
**EXACTLY** `{ edge_type, direction, target_id, target_title, authored }` — only enough for a
reader to decide whether to go read a related entry. **No enrichment**: no `source_id`,
`depth`, `metadata`, weight, raw `source` string, or `target_confidence` on the get payload.
**Cap-3 sharpens this boundary, not relaxes it** — with only 3 slots, every byte of
enrichment is wasted. Full per-edge detail and multi-hop traversal stay in `context_graph`;
the surfaced `target_id`s are the entry points back into it. Any proposal to add a field to
the get edge is a boundary violation requiring a new ADR.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Read mechanism | **New ranked variant** beside `query_direct_neighbors`: canonicalize symmetric → `LEFT JOIN entries.confidence` → `ORDER BY (source='agent') DESC, t.confidence DESC NULLS LAST, target_id ASC LIMIT 3`; live SQL on `read_pool_server`; **separate split `COUNT(*)`** for totals; opt-out skips both queries. Plain function gains only the `source` column. | D-01 / SR-14 | ADR-001 |
| Edge payload shape | `{edge_type, direction, target_id, target_title, authored}` — projection of `EdgeRecord` (drop `source_id`/`depth`/`metadata`, add `target_title`/`authored`, add get-only `↔`); cap-3 reinforces no-enrichment | D-02 / SR-06 | ADR-002 |
| Serializer seam | `format_single_entry` gains `edges: Option<&EdgesView>`; `entry_to_json`/markdown helper signatures UNCHANGED; `edges` key/section injected by get path only — `None ⇒ key absent` structural invariant | D-07 / SR-01 | ADR-003 |
| `source` column / `authored` | Add `source` additively to `RawEdgeRow` + all 4 plain neighbor SELECTs; ranked variant additionally `LEFT JOIN`s `entries.confidence` + canonicalizes; `authored = (source == "agent")`; `context_graph` neighbors verified unaffected empirically; no DDL | D-03 / SR-02 | ADR-004 |
| JSON totals shape | Nested `edge_totals: {inbound, outbound}` object (matches `co_access`/`correction_chains`/`security` house style); post-canonicalization (`↔` once) | OQ-01 / D-05 | ADR-005 |
| Markdown grouping | **Author-asserted/Inferred sub-split DROPPED** — ranking front-loads authored, so the sub-split is redundant. Single flat ranked ≤3 list; `↔` glyph for symmetric; `_…and N more — use context_graph_` pointer when capped | OQ-02 / D-08 | ADR-005 |
| Depth / opt-out | Depth-1, both directions; `include_edges: Option<bool>`, `None`/`Some(true)` surface, `Some(false)` suppress (skips ranked select + count + title join entirely) | D-01 | ADR-001 |
| Supersession | Excluded for free via empty `edge_types` inheriting `!= 'Supersedes'` filter; `supersedes`/`superseded_by` sole representation | D-04 | (ADR #4461 reused) |
| **Ranking rule (the core)** | **Authored-first; inferred fills only when authored < 3; inferred ranked by target-entry `entries.confidence` (NOT `graph_edges.weight` — frozen per ass-079).** Exact `ORDER BY (source='agent') DESC, t.confidence DESC NULLS LAST, target_id ASC LIMIT ?` — `?` bound to `GET_EDGE_DISPLAY_LIMIT` | D-09 | ADR-006 (#5054) |
| **Display cap is one named constant** | `GET_EDGE_DISPLAY_LIMIT: i64 = 3` defined **once** in `unimatrix-store/src/read.rs` (below `CO_ACCESS_GRAPH_MIN_COUNT`), re-exported via `lib.rs`; referenced by the SQL `LIMIT`, the `…N more` threshold, and tests — **no magic literal `3` at any cap site**. Decoupled from the uncapped totals + canonicalization; retune is a one-line edit. Follows the `EDGE_SOURCE_*` / `CO_ACCESS_GRAPH_MIN_COUNT` constants-location convention | FR-18 / C-12 | ADR-006 (#5054) |
| **Symmetric canonicalization (blocker)** | `Contradicts`/`CoAccess`/`Informs` collapse to one `↔` edge **in SQL, before ranking AND counting**; `Prerequisite`/`Supports` stay asymmetric with `→`/`←`. "Counted once" is an invariant on display AND totals, tested separately | D-10 | ADR-007 |
| Display cap / totals | Render **≤3** edges (was 10); totals **uncapped** + direction-split, symmetric counted once (post-canonicalization); `…N more — use context_graph` affordance when capped | D-05 | ADR-005 / ADR-007 |
| Internal-caller opt-out | Hook path, briefing by-ID fetches, by-ID loop fetches default `include_edges: Some(false)`; agent-facing `context_get` MCP tool stays default-on (`None`). Each internal call site enumerated as an asserted test | OQ-03 (architect) | ADR-001 |
| **Edge-query failure: FAIL LOUD (OQ-A RESOLVED)** | On the **default-on path**, if the ranked/edge query, the split `COUNT(*)`, or the batched title join fails **after** the primary `entry_store.get` succeeded, `context_get` **fails the whole call** via the **same error mapping as the primary-read failure path** (mapped `ServerError`; no `.unwrap()`/`expect()` on the edge path). **No** degrade-with-note, **no** silent edge omission — silent omit is indistinguishable from a true zero-edge entry (FR-12) and poisons the next-hop signal. One consistent failure contract; no new partial-success response shape. **Scoped to default-on only** — the opt-out path (`Some(false)`) skips the edge/count/title queries entirely and **cannot reach this failure**. If a degrade path is ever reintroduced (C-13), it MUST carry an explicit "edges unavailable" marker distinct from "no edges". | OQ-A / FR-19 / C-13 (human-directed) | (R-16; primary-read mapping reused) |

## Files to Create / Modify

| File | Change |
|------|--------|
| `crates/unimatrix-store/src/read.rs` | **New const** `pub const GET_EDGE_DISPLAY_LIMIT: i64 = 3;` immediately below `CO_ACCESS_GRAPH_MIN_COUNT`; single source of truth for the display cap (ADR-006 #5054) |
| `crates/unimatrix-store/src/lib.rs` | Re-export `GET_EDGE_DISPLAY_LIMIT` in the existing `pub use read::{…}` block (established edge-constant convention) |
| `crates/unimatrix-store/src/graph_queries.rs` | `RawEdgeRow` gains `pub source: String` (~line 73); ranked variant also carries `pub target_confidence: Option<f64>` |
| `crates/unimatrix-store/src/graph_queries_neighbors.rs` | `map_edge_row` reads `source` via `try_get` (~line 95); add `source` to all 4 plain SELECTs (`run_outgoing_query`/`run_incoming_query`, empty-type + IN-type branches) |
| `crates/unimatrix-store/src/graph_queries_ranked.rs` (new — pre-authorized, OQ-B) | **Ranked variant**: canonicalize symmetric → `LEFT JOIN entries.confidence` → `ORDER BY (source='agent') DESC, t.confidence DESC NULLS LAST, target_id ASC LIMIT ?` with `?` **bound to `GET_EDGE_DISPLAY_LIMIT`** (never a literal 3); **split `COUNT(*)`** over the same canonicalized set (post-canonicalization, **uncapped** — does not reference the constant). Static SQL + positional binds. **FR-19 fail-loud**: these queries return `Result<_, StoreError>` and **never** `.unwrap()`/`expect()` — a query failure propagates so the caller can map it as a primary-read error |
| `crates/unimatrix-server/src/mcp/tools.rs` | `GetParams.include_edges: Option<bool>` (~line 243); `context_get` handler edge assembly + opt-out skip (~line 924); internal-caller opt-out (`Some(false)`) at enumerated by-ID call sites. **FR-19 fail-loud (default-on path)**: after the primary `entry_store.get` succeeds, any edge/ranked-query, split-`COUNT(*)`, or title-join `Err` is mapped to the **same `ServerError`** as the primary-read failure and **returned** — never degraded-with-note, never edges-omitted. The opt-out branch (`Some(false)`) skips these queries and so **cannot reach this failure** |
| `crates/unimatrix-server/src/mcp/get_edges.rs` (new — pre-authorized, OQ-B) | Edge assembly: projection (`→`/`←`/`↔`), batch title join over ≤3 targets, build `EdgesView` — keeps `tools.rs` under 500 lines. **FR-19**: propagates the title-join `Result` (no `.unwrap()`); a join failure surfaces as a mapped error, never a silent `target_title: null` fill or an omitted edge set |
| `crates/unimatrix-server/src/mcp/response/entries.rs` | `format_single_entry` gains `edges: Option<&EdgesView>`; get-format branching |
| `crates/unimatrix-server/src/mcp/response/edges.rs` (new — pre-authorized, OQ-B) | `GetEdge`/`EdgeTotals`/`EdgesView` types + 3-format render helpers (`↔` glyph, capped pointer) — `…N more` threshold/arithmetic references `GET_EDGE_DISPLAY_LIMIT` (`N = total − cap`), no literal 3; keeps `entries.rs` under 500 lines |

`entry_to_json` (`response/mod.rs:121`) and `format_entry_markdown_section`
(`response/mod.rs:160`) signatures stay UNCHANGED (ADR-003 byte-identity invariant).

**OQ-B file-size pre-authorization**: sibling modules `graph_queries_ranked.rs` (store),
`mcp/get_edges.rs`, and `response/edges.rs` (server) are pre-authorized — split cleanly onto
them rather than discovering the 500-line limit mid-build.

## Data Structures

```rust
// read.rs — single source of truth for the display cap (ADR-006 #5054, FR-18/C-12)
// i64 to match the sqlx `LIMIT ?` bind convention (parallel to CO_ACCESS_GRAPH_MIN_COUNT).
// Totals (COUNT) are UNCAPPED and never reference this; one-line retune.
pub const GET_EDGE_DISPLAY_LIMIT: i64 = 3;   // re-exported via lib.rs pub use read::{…}

// graph_queries.rs — additive fields
struct RawEdgeRow {
    source_id: u64,
    target_id: u64,
    relation_type: String,
    source: String,                     // additive (ADR-004); 'agent' = authored
    target_confidence: Option<f64>,     // ranked-variant only; None for dangling (LEFT JOIN);
                                        // used for the inferred tiebreak, NEVER surfaced
}

// split-count return (store)
struct EdgeCountSplit { inbound: usize, outbound: usize }   // post-canonicalization, ↔ once

// new get-edge vocabulary (response/edges.rs)
struct GetEdge {
    edge_type: String,                  // = EdgeRecord.relation_type
    direction: &'static str,            // "inbound" | "outbound" | "both"  ("both" renders ↔)
    target_id: u64,                     // the OTHER endpoint
    target_title: Option<String>,       // null when target unresolved (dangling, retained)
    authored: bool,                     // source == "agent"
    // NO source_id, depth, metadata, source, weight, or target_confidence (guardrail / ADR-002)
}
struct EdgeTotals { inbound: usize, outbound: usize }       // uncapped, ↔ counted once
struct EdgesView { edges: Vec<GetEdge>, totals: EdgeTotals } // edges ≤3

// tools.rs — additive field
struct GetParams { /* …existing… */ include_edges: Option<bool> }  // #[serde(default)]
```

## Function Signatures

```rust
// plain neighbor query — extended additively (gains `source` column only)
async fn query_direct_neighbors(
    pool: &SqlitePool, id: u64, edge_types: &[&str], direction: NeighborDirection,
) -> Result<Vec<RawEdgeRow>, StoreError>;
// plain SELECT (after): SELECT source_id, target_id, relation_type, source FROM graph_edges WHERE …

// NEW ranked variant (get-only; does not touch the plain function)
async fn query_ranked_neighbors(
    pool: &SqlitePool, id: u64,   // direction = Both; cap = GET_EDGE_DISPLAY_LIMIT (bound to LIMIT ?)
) -> Result<Vec<RawEdgeRow>, StoreError>;
// canonicalize symmetric → ↔ ; LEFT JOIN entries t ON t.id = <other endpoint> ;
// ORDER BY (source='agent') DESC, t.confidence DESC NULLS LAST, target_id ASC LIMIT ?
//   ← `?` bound to GET_EDGE_DISPLAY_LIMIT, never a literal 3 (FR-18/C-12)

// NEW split count (get-only; over the SAME canonicalized set)
async fn count_neighbors_split(
    pool: &SqlitePool, id: u64,
) -> Result<EdgeCountSplit, StoreError>;   // COUNT(*) in SQL, ↔ once, never materializes rows

// serializer seam (after — only context_get passes Some)
fn format_single_entry(
    entry: &EntryRecord, format: ResponseFormat, edges: Option<&EdgesView>,
) -> CallToolResult;

// batched title join (precedent fetch_nodes_batch, positional binds): over the ≤3 displayed targets
//   SELECT id, title FROM entries WHERE id IN (?, …)
```

Projection rule (per ranked row): `direction = "both"` (`↔`) for a canonicalized symmetric
type; otherwise `"outbound"` (`→`) if anchor is `source_id`, else `"inbound"` (`←`).
`target_id` = the other endpoint. `authored = (source == "agent")`. `target_title` from the
title-map (Option). **Rank-and-limit and counting happen in SQL — never fetch-all-then-slice
in Rust (C-7/SR-14).** Only the ≤3 displayed targets are title-resolved; the uncapped set is
never materialized.

## Output Renderings (D-08 / ADR-005 — acceptance surface, NFR-7)

- **summary/null**: digest on the entry line showing the true split, distinguishing asymmetric
  direction from symmetric, plus an authored tally — proposed `… | edges: 5↑ 2↓ ↔3 (2 authored)`
  (asymmetric out `↑` / in `↓` arrows, symmetric `↔` count, authored tally); zero → `edges: none`.
  Exact glyph order/form and whether the authored tally counts the displayed-3 or the full set:
  **OQ-02, architect's call** — pick a form consistent with existing entry-line conventions.
- **markdown**: `### Related` after the footer showing the **flat ranked ≤3** set — **NOT**
  split into Author-asserted/Inferred sub-headers (dropped; ranking front-loads authored). Each
  line `- {edge_type} {→|←|↔} #{target_id} "{target_title}"` using `↔` for canonicalized
  symmetric types. When more edges exist than displayed, a single
  `_…and N more — use context_graph_` pointer — the "more than displayed" test (`total >
  GET_EDGE_DISPLAY_LIMIT`) and `N = total − GET_EDGE_DISPLAY_LIMIT` reference the constant, no
  literal 3; zero-edge → `No related entries.`
- **json**: `"edges": [{edge_type,direction,target_id,target_title,authored}]` (the ranked ≤3)
  plus `"edge_totals": {"inbound": N, "outbound": M}` (uncapped, symmetric-once). Both keys
  present iff edges surfaced; zero-edge → `edges: []`, `edge_totals: {0,0}`; opt-out → neither key.

## Constraints (hard)

- **C-1** Read-path only, no schema migration / DDL / migration file. Single net-new cost: `source` in the SELECT/`RawEdgeRow`.
- **C-2** Do not double-represent supersession — `supersedes`/`superseded_by` authoritative; `Supersedes` typed edge excluded.
- **C-3** Do not break `context_graph` neighbors — additive `source` only on the plain path, re-verified **empirically** (existing neighbors suite green, unedited). The get-only `↔`/canonicalization MUST NOT leak into the neighbors contract.
- **C-4** `None ⇒ key absent` on the shared serializer — a **tested invariant**, not a convention.
- **C-5** Minimal per-edge payload — exactly the 5 fields, no enrichment (guardrail; cap-3 reinforces).
- **C-6** Symmetric canonicalization **before rank AND count** (blocker — SR-08, D-10). "Counted once" is an invariant on **both** display and totals, tested separately.
- **C-7** Rank-and-limit (`LIMIT 3`) and the split `COUNT(*)` execute **in SQL**, not Rust. Fetch-all-then-slice/count-in-Rust is prohibited — it satisfies the output contract but violates the memory/latency intent invisibly (SR-14).
- **C-8** Locked ranking order: exactly `ORDER BY (source='agent') DESC, t.confidence DESC NULLS LAST, target_id ASC LIMIT 3` via a **LEFT JOIN** on target confidence. Ranking by `graph_edges.weight` is prohibited (frozen, non-discriminating — ass-079).
- **C-9** AC-12 latency numbers (≤5 ms p50 / ≤15 ms p95) are **provisional until a measured edge-free baseline** (high-degree node in scope) confirms them. **OPEN — see below.**
- **C-10** `authored` boolean honest only while inferred sources are statistical (NLI dark, ASS-037); keep `source` string underneath; revival is the documented trigger to revisit D-03.
- **C-11** Workspace rules: no `.unwrap()` in non-test code; ≤500 lines/file; cumulative test infra (extend existing `context_get`/response/graph fixtures).
- **C-12** Display cap is a **single named constant** `GET_EDGE_DISPLAY_LIMIT` (=3, unchanged) — referenced by the SQL `LIMIT`, the "more than displayed" comparison, and the `N more` arithmetic; **no literal `3` at any cap site**. Changing the cap is a **one-line edit** to the constant and changes **only** the rendered set size — never the (uncapped) totals (FR-10) and never canonicalization (FR-8). Tests reference the constant, not `3` (FR-18 / AC-13 / ADR-006 #5054).
- **C-13** **Fail-loud edge contract; no silent-omit (OQ-A RESOLVED; FR-19 / AC-14; human-directed).** On the default-on path, a post-primary-read edge/count/title failure **MUST** fail the whole `context_get` via the primary-read error mapping. A silent-omit (edges absent on failure) is **prohibited** — it is indistinguishable from a true zero-edge entry (FR-12) and poisons the next-hop signal. There is **one** failure contract and **no** new partial-success response shape. **If a degrade-with-note path is ever reintroduced**, it MUST carry an explicit **"edges unavailable" marker distinct from "no edges"** (the FR-12 empty state) so callers can tell the two apart — a bare omission is never acceptable.
- **NFR-1** Opt-in adds **two bounded SQL queries** (ranked `LIMIT 3` confidence-join select + split `COUNT(*)`) plus one batched title join over ≤3 targets — none materializes a hub node's full fan-out; opt-out adds **zero** query cost.
- **NFR-3** Ranked select, count, and title join use `read_pool_server` (ADR #3595) over indexed columns (`idx_graph_edges_source_type`/`idx_graph_edges_target_type`, `entries.id`).
- **Security**: positional binds for the title `IN (…)` list and all edge queries — never string-interpolated ids; `LIMIT`/`ORDER BY`/canonicalization `CASE` are static SQL, not assembled from input.

## Documented Non-Bug Behaviors (assert as expected, not defects — SR-07)

- **DNB-1 Dangling target**: `target_id` with no `entries` row → `target_title: null`, edge **retained** and ranks deterministically last among inferred (`NULLS LAST`). Null is signal.
- **DNB-2 Corrected-entry transient**: post-`context_correct`, authored edges carry forward (vnc-035) but inferred edges (co-access/Informs) re-earn next tick — so a just-corrected entry legitimately shows its authored edges (which now rank first and fill the slots) while inferred candidates are sparse/absent. Honest live state, not loss. Under the reframe the old author/inferred markdown sub-split is dropped, so this manifests only as *which edges win the ≤3 slots*.
- **DNB-3 Visible zero**: edge-free entry renders explicit empty state in all three formats — the empty box is the mechanism.

## OPEN — AC-12 latency baseline (human decision pending)

The AC-12 budget (**≤5 ms p50 / ≤15 ms p95** added over the edge-free `context_get`
baseline) is **provisional**. Before the numbers are locked, delivery MUST produce a
**measured edge-free baseline on a representative store including a high-degree node** (C-9,
OQ-C, R-13). If the baseline shows the budget unattainable on hub nodes with default-on, the
**human chooses** among: (a) relax the budget, (b) mandate the OQ-03 internal-caller opt-out,
or (c) revisit default-on. The escalation path is specified; the number lock is gated on the
measurement. The rank-and-limit-in-SQL design (ADR-001) is what makes the budget reachable on
hubs — the measurement confirms it.

## Dependencies

- **ass-076 FINDINGS** (`product/research/ass-076/`, origin #708) — research input; every D-0x traces to an answered RQ.
- **ass-079 FINDINGS** (`product/research/ass-079/`) — frozen-`Informs`-weight rationale grounding D-09's "rank by target confidence, not edge weight" (C-8).
- **Author-asserted-edge convention** (`uni-architect`/`uni-store-adr`) — the assert half; this is the surface half. Input, not under revision.
- **vnc-035 carry-forward** (#749) — surfaced live-`graph_edges` view reflects post-carry state (DNB-2); carried-forward edges classify **authored** (`source='agent'`, FR-17, SR-10).
- **`entries.confidence`** (`db.rs:549`; `unimatrix-engine/src/confidence.rs`) — the inferred-edge rank key, joined via `target_id` (C-8, FR-9).
- **`context_graph` `EdgeRecord`** (ADR #4478) + **Supersedes SQL exclusion** (ADR #4461) — the shape/filter projected from and reused.
- **`query_direct_neighbors`** (`graph_queries.rs:200`) / `RawEdgeRow` / `map_edge_row` — co-owned with `context_graph` neighbors; **plain call is no longer the sole reuse target** (returns unranked/un-canonicalized/unbounded rows). A new ranked variant + count query is needed.
- **`fetch_nodes_batch`** (`graph_read_subgraph.rs:568`) — batched-title precedent.
- **ADR-001 (#5009) reconciliation (OQ-04)** — predates the reframe; architect updates it via `context_correct` (not deprecate+store) to the rank-and-limit-in-SQL strategy. Owned, tracked.
- Relates to **#744 / #745** (edge-loss cluster) — this makes their effect observable; no ordering dependency either way.
- Crates: existing workspace only (`unimatrix-store`, `unimatrix-server`); no new external crates.

## NOT in Scope

- Multi-hop / traversal at the get layer (stays in `context_graph`).
- **An edge dump** — this is a ≤3 next-hop affordance, not a complete edge list.
- A new edge type or schema migration.
- Re-representing supersession.
- Backfilling historical edges (deferred per #708).
- Changing the edge-assertion conventions.
- Putting edges on `search`/`lookup`/`store`/`correct` (serializer gains the capability; only `context_get` opts in; list views byte-identical).
- Exposing per-edge detail (metadata/weights/depth/raw `source`/`source_id`/`target_confidence`) on the get payload — `context_graph`'s domain.
- A `provenance` enum (`authored` boolean for now; `source` string kept underneath).
- **Ranking by edge weight** — inferred ranking is by target-entry confidence (C-8); `weight` is frozen/non-discriminating (ass-079).
- **The author/inferred markdown sub-split** — dropped; ranking front-loads authored (FR-14).
- Fixing edge loss (#744 redirect cap, #745 orphans) — makes loss observable, not repaired.

## Alignment Status

**PASS — 4 PASS / 2 WARN / 0 variance / 0 fail** (ALIGNMENT-REPORT.md, 2026-06-15, re-check
after the next-hop reframe). Vision (Principle 4, typed relationship graph — served as a
*curated pointer* at the point of consumption), milestone (Vinculum read-path), scope gaps,
architecture consistency, and risk completeness all PASS. The reframe **narrows** the surface
(cap 10→3, display-only) while **raising** the correctness bar (canonicalization, ranking
determinism, latency budget) — discipline, not creep. The "which 3" rule reuses existing
`entries.confidence` read-only: **no new scoring model, no new signal, no tuning surface**.

Two **soft WARNs** (accepted, no FAIL, no vision drift):

1. **WARN — OQ-03 internal-caller opt-out** expands footprint beyond SCOPE's open question
   (hook path / briefing by-ID / by-ID loop fetches default `Some(false)`, each asserted as a
   named test). **Accept with confirmation**: human confirms (a) the enumerated internal call
   sites are correct/complete and (b) **no agent-facing path is flipped default-off** (that
   would weaken the proactive-delivery loop). D-01 stays default-on at the type level; OQ-03
   only decides which *callers* set it false.
2. **WARN — AC-12 latency numbers provisional** (unbacked until measured). **Accept the
   obligation as written**: require the measured edge-free baseline (high-degree node in
   scope) before the numbers lock (C-9); if unattainable, the documented options
   (relax / mandate OQ-03 opt-out / revisit default-on) go to the human. Escalation path
   already specified — no doc change needed.

## Resolved (human-directed)

- **OQ-A — degrade vs fail on edge-query error: RESOLVED = FAIL LOUD.** The human resolved this
  in favor of **(a) fail the whole call**. On the **default-on path**, if the ranked/edge query,
  the split `COUNT(*)`, or the batched title join fails **after** the primary `entry_store.get`
  succeeded, `context_get` **fails the whole call** via the **same error mapping as the
  primary-read failure path** (mapped `ServerError`; no `.unwrap()`/`expect()` on the edge path).
  It does **NOT** degrade-with-note and does **NOT** silently omit edges. **Rationale:** a
  silent-omit response is indistinguishable from a true zero-edge entry (FR-12) and would poison
  the very next-hop signal this feature exists to provide; the contract is one consistent failure
  shape with no new partial-success response. Locked as **FR-19 / C-13 / AC-14**, verified by the
  named **RED** failure-path test (`edge-query-failure-fails-loud`, #4876) plus a zero-vs-failure
  distinction assertion. **Scoped to default-on only** — the opt-out path (FR-3, `Some(false)`)
  skips the edge/count/title queries entirely and therefore **cannot reach this failure**. No
  longer open.

## Open Questions (for delivery — resolve before / early in implementation)

- **OQ-B — file-size pre-authorization (architect).** Sibling modules `graph_queries_ranked.rs`,
  `mcp/get_edges.rs`, `mcp/response/edges.rs` are **pre-authorized** — split cleanly onto them
  rather than discovering the 500-line limit mid-build. (Locally resolvable; no human gate.)
- **OQ-C / AC-12 measured baseline** — see OPEN section above. Blocks only the AC-12 number lock.
