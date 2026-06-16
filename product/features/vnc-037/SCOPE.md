# vnc-037 — A next-hop navigation affordance: surface an entry's most-relevant typed edges on `context_get`

> Having read an entry, **here are a few genuinely related entries worth pulling next.**
> `context_get` returns an entry's fields but not its typed graph edges — so an agent
> reading an ADR gets no signal about what it depends on, contradicts, or is supported
> by, and no pointer to where to go next. This feature surfaces, by default, a **small
> ranked set** of the entry's depth-1 edges — *not* a complete edge dump. It is a
> next-hop affordance: at most a few high-value pointers, with honest uncapped totals so
> the empty box and high-degree nodes stay observable. **Read-path only — no new edge
> type, no schema migration, no multi-hop.**

## Problem Statement

`context_get` (`crates/unimatrix-server/src/mcp/tools.rs:920-1009`) does
`entry_store.get(id)` then `format_single_entry` — edges are never touched. To see an
entry's relationships today you must make a *separate* `context_graph` neighbors call.
Two costs:

1. **No next-hop signal at the point of retrieval.** The richest read in the system — an
   agent studying one entry — surfaces none of its typed structure and gives no pointer
   to *where to read next*. The fix is **not** to dump every edge: a hub node has
   hundreds, and a wall of edges is as useless as none. What's missing is a *navigation
   affordance* — a handful of genuinely-related entries worth pulling next. That reframe
   is why the display cap is small (3) and **ranking is the core of the feature**: with
   only three slots, *which three* is the whole question.
2. **The author-asserted-edge convention has no feedback loop.** `uni-architect` /
   `uni-store-adr` now ask authors to declare `Prerequisite`/`Contradicts`/`Supports`
   edges (high bar, default-none). Asserting edges nobody ever *sees* gives authors no
   signal; a zero-edge entry stays invisibly zero. Surfacing on read closes the loop —
   the empty box becomes visible exactly where the knowledge is consumed. Authored edges
   are the scarce, high-trust signal, so they take the slots first (D-09).

This feature is the **surface** half of assert-and-surface. The assert half already
shipped (the authoring convention); backfill of historical edges remains deferred until
both ends exist.

## Goals

1. On every `context_get`, surface a **ranked, capped** set of the entry's **depth-1**
   typed edges (**both directions**) as a *next-hop affordance* — at most 3 (D-05),
   chosen by an explicit selection rule (D-09), not a complete edge list. Read **live
   `graph_edges` via SQL**, not the in-memory `TypedRelationGraph` (so a just-written
   edge is visible immediately; no snapshot-staleness on a point read).
2. **Rank with intent.** Authored edges fill the slots first; inferred edges fill the
   remainder ranked by **target-entry confidence** (D-09). The displayed set is the few
   highest-value next hops, not an arbitrary first-N.
3. **Always report honest, uncapped totals**, split inbound/outbound and counting
   symmetric edges once (D-05, D-10). The cap is display-only — totals stay exact so the
   visible-empty-box (the feedback loop) and high-degree observability (#744) survive.
4. Distinguish **author-asserted** edges from **inferred** (co-access / cosine) edges, so
   a reader knows whether a human/agent declared the relationship or a statistic noticed it.
5. Render the edges across all three output formats (`summary`/null, `markdown`, `json`)
   with one edge vocabulary aligned to `context_graph`'s `EdgeRecord`.
6. Keep the change **surgical**: one new SELECT column (read-path, no migration), reused
   queries, one serializer seam; rank-and-limit in SQL so a hub node never pulls its full
   fan-out into memory; zero change to any existing payload that doesn't opt in.

## Non-Goals

- **Multi-hop / traversal.** Depth-1 only at the get layer; `target_id`s are the entry
  points into `context_graph`. Multi-hop stays in `context_graph`.
- **A new edge type or schema migration.** This is read-path surfacing of edges that
  already exist. The one net-new read cost (adding `source` to the neighbor SELECT) is
  not a migration.
- **Re-representing supersession.** `supersedes`/`superseded_by` remain the sole
  representation; the `Supersedes` typed edge stays excluded (already free at SQL).
- **Backfilling historical edges.** Deferred (per #708) until assert + surface both exist.
- **Changing the edge-assertion conventions** (`uni-architect`/`uni-store-adr`) — input,
  not under revision.
- **Putting edges on `search`/`lookup`/`store`/`correct`.** The serializer gains the
  *capability* (optional param), but only `context_get` opts in. List views stay
  byte-identical (see Decisions D-07).

## Background Research (grounded — from ass-076 FINDINGS)

> Full code grounding is in `product/research/ass-076/FINDINGS.md`. Key sites:

- **`context_get` handler**: `tools.rs:920-1009`; formats via `format_single_entry`
  (`mcp/response/entries.rs:13-36`) — the single edit site for all three formats.
- **Entry serializers**: `entry_to_json` (`response/mod.rs:121-138`),
  `format_entry_markdown_section` (`response/mod.rs:160-191`). `entry_to_json` is
  **shared** by search/lookup/store/correct — mutating it changes all of them (the reason
  for the optional-param seam, D-07).
- **Cross-tool shape**: `context_graph` neighbors returns `EdgeRecord`
  (`graph_read.rs:134-144`): `{source_id, target_id, relation_type, direction, depth,
  metadata}`. The get projection aligns to this (D-02).
- **Depth-1 query**: `query_direct_neighbors` (`graph_queries.rs:200-216`) →
  `run_outgoing_query`/`run_incoming_query` (`graph_queries_neighbors.rs:13-92`).
  Already filters `relation_type != 'Supersedes'` (ADR #4461) — supersession de-dup is
  free. **Selects only `source_id,target_id,relation_type`** — must add `source` for D-03.
  For `Both`, `query_direct_neighbors` simply `outgoing.extend(incoming)` — **no
  dedup** — so symmetric two-row edges return twice (drives D-10).
- **Target-entry confidence column (D-09 grounding)**: `entries.confidence` — a `REAL
  NOT NULL DEFAULT 0.0` column on the `entries` table (`db.rs:549`). It is a cached
  six-component **Bayesian Beta-Binomial composite** (`unimatrix-engine/src/confidence.rs`;
  helpfulness term is a Beta-Binomial posterior mean, cold-start α₀=3.0/β₀=3.0 — *not*
  literally a Wilson interval, but it is the canonical per-entry confidence score). It
  lives **directly on `entries`**, so it is joinable as
  `JOIN entries t ON t.id = graph_edges.target_id ... ORDER BY t.confidence DESC` — no
  separate confidence table. This is the rank key for inferred edges.
- **Frozen Informs weight (D-09 rationale, ass-079)**: `graph_edges.weight` is a weak
  ranker for behavioral `Informs`. The weight is set first-write-wins via `INSERT OR
  IGNORE` and mapped from cycle outcome by `outcome_to_weight` → `1.0` for `success`,
  `0.5` otherwise (`behavioral_signals.rs`). Closed cycles are ~always `success` (the
  delivery workflow reworks failure pre-close), so the weight is effectively a constant
  → no discriminating signal. **Rank by target confidence, not edge weight.**
- **Symmetric vs asymmetric storage (D-10 grounding)**: three relation types are stored
  as **two reciprocal rows** (A→B *and* B→A):
  - `Contradicts` (authored) — `edge_write.rs:211-223` writes the reverse row.
  - `CoAccess` (behavioral, S8) — `graph_enrichment_tick.rs:442-478` writes both
    directions (relation_type literal `"CoAccess"`, source `S8`).
  - `Informs` (behavioral, S1/S2) — `behavioral_signals.rs:244-308` writes forward and
    reverse.
  All other types are **single-row / asymmetric**: `Prerequisite`, `Supports`
  (authored, one row, direction meaningful), `Supersedes` (excluded), and the newer
  semantic types. Because `Both` does not dedup, symmetric edges today would render
  twice and double-count — D-10 collapses them to one `↔` edge before ranking and
  counting.
- **Provenance**: `graph_edges.source` (`db.rs:960`) = `'agent'` (authored) vs
  `behavioral`/`co_access`/`S8`/cosine (inferred). NLI is dark (ASS-037), so the inferred
  bucket today is **co-access/cosine only** — making a boolean the honest shape (D-03).
- **Batch-title precedent**: `fetch_nodes_batch` (`graph_read_subgraph.rs:568-600`) —
  one `SELECT id,title FROM entries WHERE id IN (…)` resolves all titles in one round trip.
- **Prior ADRs**: #4478 (vnc-018 — `EdgeRecord` shape, reuse-intended), #4461 (vnc-017 —
  Supersedes excluded at SQL).

## Design Decisions (locked in uni-zero scoping — design session inherits these)

- **D-01 — Depth + opt-out.** Depth-1, both directions, surfaced **by default** via a new
  `include_edges: Option<bool>` field on `GetParams` — `None` ⇒ surface, `Some(false)` ⇒
  suppress. Default-on preserves the feedback-loop goal; the explicit opt-out is an escape
  hatch for latency/payload-sensitive callers (bulk reads, agents that don't want the
  relational payload). Additive `Option<T>` field — backward-compatible, existing callers
  unaffected (GraphParams additive-field precedent, ADR-002 vnc-020). *Note: this
  consciously diverges from ass-076 RQ-5's "no flag" recommendation. RQ-5 rejected a flag
  that would default **off** and defeat the loop; an opt-out that defaults **on** keeps
  the loop intact while adding the escape hatch — a different decision, not a contradiction
  of the research.*
  **Query strategy — rank-and-limit in SQL (revised).** Do **not** pull the full neighbor
  set into memory. Run a query that joins target confidence, canonicalizes symmetric edges
  (D-10), orders by `(authored DESC, target_confidence DESC)` (D-09) and `LIMIT 3` (D-05);
  run a **separate cheap `COUNT(*)`** for totals (split by direction, post-canonicalization).
  This bounds hub-node fan-out (a 1000-edge node returns 3 rows + two counts, never 1000
  rows). The plain `query_direct_neighbors(pool, id, &[], Both)` is no longer the right
  reuse target on its own — it returns unranked, un-canonicalized, unbounded rows; the
  read path needs the ranked/limited variant plus the count query (design session sizes
  the exact SQL/seam).
- **D-02 — Per-edge payload.** `{ edge_type, direction, target_id, target_title,
  authored }`. Drop anchor-constants `source_id`/`depth`. `target_title` via one batched
  join; unresolved target → `target_title: null` (dangling = signal, don't drop the edge).
  **Direction semantics (D-10 fix):** `direction` (`→`/`←`) is meaningful **only for
  asymmetric types** (`Prerequisite`, `Supports`). For canonicalized symmetric types
  (`Contradicts`, `CoAccess`, `Informs`) the edge is bidirectional and carries the `↔`
  glyph; do not emit a spurious `→`/`←` for them. *(RQ-2/RQ-7.)*
- **D-03 — Provenance is a boolean.** `authored = (source == 'agent')`. Not a `provenance`
  enum — the live inferred sources are all statistical (NLI dark), so binary is the honest
  trust split. Keep the `source` string available underneath for future revival. Requires
  adding `source` to the neighbor SELECT + `RawEdgeRow` (read-path only, **no migration**).
  *(RQ-4.)*
- **D-04 — Supersession.** Excluded at SQL for free (empty `edge_types` inherits the
  `!= 'Supersedes'` filter); `supersedes`/`superseded_by` stay sole representation. *(RQ-3.)*
- **D-05 — Display cap 3; total uncapped, split by direction, symmetric counted once.**
  Render **at most 3** edges (down from 10 — this is a next-hop affordance, not an edge
  dump; with 3 slots, the selection rule D-09 *is* the feature). Always emit an exact,
  **uncapped** count, **split `inbound` / `outbound`**, computed by a separate `COUNT(*)`
  **after** symmetric canonicalization (D-10) so a `↔` edge counts once, not twice. The
  cap is **display-only**; totals stay honest — that is what keeps the visible-empty-box
  feedback loop and #744/#745 inbound-degree observability intact. The direction split is
  load-bearing: it makes inbound degree (and the #744 redirect-cap question) observable.
  *(RQ-5; cap revised 10 → 3 under the next-hop reframe.)*
- **D-06 — Zero is explicit.** A no-edge entry renders a visible "No related entries" /
  `"edges": [], total 0` — inverting the usual omit-at-zero convention, because here
  visibility *is* the mechanism. *(RQ-5.)*
- **D-07 — Optional-param seam, `None` ⇒ key absent.** The shared serializer gains an
  optional `edges` argument; **`None` emits no `edges` key at all** (search/lookup/store/
  correct stay byte-identical). `context_get` passes `Some(...)` **only when
  `include_edges` resolves true** (D-01); an opted-out get passes `None` and is itself
  edge-free, indistinguishable from the list-view tools. The DB query lives in the handler
  (skipped entirely on opt-out), not the serializer. One vocabulary, surgical blast radius.
  *(ass-076 Out-of-Scope flag, resolved.)*
- **D-08 — Format renderings (reworked for cap-3 + `↔`).** All three render the **same
  honest split totals** and the **same ranked ≤3** set.
  - **summary/null** = a count digest on the entry line that shows the true split,
    distinguishing asymmetric direction from symmetric, and authored count — e.g.
    `… | edges: 5↑ 2↓ ↔3 (2 authored)` (asymmetric out/in arrows, symmetric `↔` count,
    authored tally); zero → `edges: none`. Design session fixes the exact glyph order/form.
  - **markdown** = a `### Related` section after the footer showing the **ranked 3** (not
    split into Author-asserted/Inferred sub-headers anymore — the ranking already front-
    loads authored), each line `- {edge_type} {→|←|↔} #{target_id} "{target_title}"` using
    `↔` for canonicalized symmetric types; when more exist, a single
    `_…and N more — use context_graph_` pointer (directs the reader to the full-graph tool
    rather than implying the get view is complete).
  - **json** = `"edges": [{edge_type,direction,target_id,target_title,authored}]` (the
    ranked ≤3) plus direction-split, symmetric-once totals. *(RQ-6.)*
- **D-09 — Selection / ranking rule (the core of the feature).** With only 3 display
  slots, *which 3* is the feature. The rule:
  1. **Authored edges first.** `Prerequisite` / `Contradicts` / `Supports`
     (`source == 'agent'`) fill slots before any inferred edge — they are the scarce,
     high-trust signal.
  2. **Inferred fills the remainder only if authored < 3.** If ≥3 authored edges exist,
     show **no** inferred edge at all.
  3. **Rank inferred by TARGET-ENTRY confidence, not edge weight.** Order inferred
     candidates by `entries.confidence` of the *target* entry (Bayesian composite,
     joinable via `target_id`; see Background Research). **Not** `graph_edges.weight`:
     behavioral `Informs` weight is frozen first-write-wins and outcomes are ~always
     `success` (ass-079), so weight carries no discriminating signal — it is a weak ranker.
  Implemented in SQL as `ORDER BY (source='agent') DESC, target.confidence DESC LIMIT 3`
  (post-canonicalization, D-10). *(New under the next-hop reframe; grounded in ass-079.)*
- **D-10 — Symmetric-edge canonicalization (blocker — currently missing).** Three
  relation types are stored as **two reciprocal rows** (A→B and B→A): `Contradicts`
  (authored — `edge_write.rs`), `CoAccess` (S8 — `graph_enrichment_tick.rs`), and
  `Informs` (behavioral — `behavioral_signals.rs`). `query_direct_neighbors(Both)` does
  `outgoing.extend(incoming)` with **no dedup**, so today these would **render twice and
  double-count**. **Collapse symmetric relation types to one `↔` edge BEFORE ranking
  (D-09) and BEFORE counting (D-05).** `direction` (`→`/`←`) is meaningful **only** for
  asymmetric types (`Prerequisite`, `Supports`); symmetric types carry the `↔` glyph
  (added to D-08) and no directional arrow (the D-02 direction-semantics fix). Single-row
  asymmetric types are unaffected. *(New blocker surfaced under the reframe.)*

## Acceptance Criteria

- AC-01: `context_get` surfaces depth-1 edges, both directions, **by default**, via
  `query_direct_neighbors` reading live `graph_edges` (D-01).
- AC-02: Each surfaced edge carries `{edge_type, direction, target_id, target_title,
  authored}`; titles resolve in one batched join; an unresolved target yields
  `target_title: null` and the edge is retained (D-02).
- AC-03: `authored` is `true` iff `source == 'agent'`; all current inferred sources map to
  `false`; the `source` column is added to the read query without a schema migration (D-03).
- AC-04: `Supersedes` never appears in the surfaced edges; `supersedes`/`superseded_by`
  remain the only supersession representation (D-04).
- AC-05: Output renders **≤3** edges, selected by the D-09 rule (authored first; inferred
  fill only when authored < 3; inferred ranked by target-entry `entries.confidence`); it
  always reports exact, **uncapped** counts split `inbound`/`outbound` with symmetric edges
  counted **once** (post-canonicalization); an entry with >3 edges shows the
  "…N more — use context_graph" affordance (D-05, D-09, D-10).
- AC-06: A zero-edge entry renders an explicit empty state in all three formats (D-06).
- AC-07: `context_get` JSON gains `edges` + direction-split totals; `context_search`,
  `context_lookup`, `context_store`, `context_correct` payloads are **byte-identical** to
  pre-vnc-037 (no `edges` key) (D-07).
- AC-08: summary, markdown, and json each render per D-08 — cap-3 ranked set, `↔` glyph
  for canonicalized symmetric edges, honest symmetric-once split totals, and the
  `…and N more — use context_graph` pointer when capped.
- AC-09: The get edge shape is a documented projection of `context_graph`'s `EdgeRecord`
  (same `relation_type`/`target_id`/`direction`); `context_graph`'s neighbors contract is
  unchanged (additive only).
- AC-10: `cargo build --workspace` and `cargo test --workspace` pass; new tests cover:
  - **symmetric canonicalization** — a `Contradicts` (and `CoAccess`/`Informs`) pair
    stored as both rows collapses to **one `↔` edge**, not two (D-10);
  - **authored-priority-under-cap** — with >3 edges where authored ≥ 3, only authored
    edges show; **inferred-fill-only-when-authored<3** — inferred appears only to top up
    to 3 (D-09);
  - **ranking-by-target-confidence** — among inferred candidates, the higher
    `entries.confidence` target ranks first (and edge `weight` does *not* decide order)
    (D-09);
  - **opt-out path** — `include_edges:false` emits no `edges` key and skips the query
    (cross-refs AC-11);
  - **high-degree node hits the SQL `LIMIT`, not memory** — a node with many edges returns
    3 rows + counts, proving rank-and-limit is in SQL (D-05/D-01);
  - **carried-forward + `context_edge` edges classify as authored** — an edge carried
    forward via vnc-035 or written via `context_edge` has `source='agent'` and is treated
    as authored by the ranking; lock this with a test;
  - plus the existing zero-edge and dangling-title cases.
- AC-11: `GetParams` gains `include_edges: Option<bool>`; `None` and `Some(true)` surface
  edges, `Some(false)` suppresses them (the get response then carries no `edges` key, and
  the neighbor query is skipped). The field is additive and backward-compatible — a
  pre-vnc-037 caller sending no field behaves as default-on (D-01).
- AC-12 (latency NFR, P3): the default-on edge path adds **two bounded SQL queries** (a
  ranked `LIMIT 3` select with a confidence join, plus a split `COUNT(*)`) to the hottest
  read tool — `context_get` also feeds the co-access loop, so its per-call cost compounds.
  The added edge work must stay within a **stated per-call latency budget** (proposed:
  **≤ 5 ms p50 / ≤ 15 ms p95** added over the edge-free `context_get` baseline on a
  representative store, including a high-degree node; design session confirms the exact
  numbers against measured baseline). The rank-and-limit-in-SQL design (D-01) is what makes
  this achievable on hub nodes — no full fan-out into memory. (OQ-03 reduces the load by
  letting internal callers opt out.)

## Constraints (hard — from ass-076)

- **Read-path only, no schema migration.** Reuse the existing typed-edge model/storage.
  The single net-new cost is adding `source` to the neighbor SELECT/`RawEdgeRow`.
- **Do not double-represent supersession.** `supersedes`/`superseded_by` authoritative.
- **Do not break `context_graph` neighbors.** Alignment extends (a new consumer), never
  incompatibly changes, the contract.
- **`None` ⇒ key absent** on the shared serializer — no existing consumer payload changes.
- Workspace rules: no `.unwrap()` in non-test code, ≤500 lines/file, cumulative test infra
  (extend the existing `context_get` / response / graph fixtures).

## Design Notes (carry into the brief — not features)

- **Corrected-entry transient.** On `context_correct`, authored outgoing edges carry
  forward (vnc-035) but inferred edges (co-access/Informs) are *not* carried — they
  re-earn on the next tick. So a just-corrected entry legitimately shows a populated
  Author-asserted section and a sparse Inferred one. Honest, not a bug — note it so the
  emptiness isn't misread as signal.
- **Makes edge-loss observable.** The inbound direction-split count (D-05) surfaces the
  effect of the #744 redirect cap (50) and #745 historical orphans. This feature is
  **downstream** of those — it makes them visible, it is not blocked on them.
- **vnc-035 is benign here.** The surfaced view reads live `graph_edges`, so it auto-
  reflects post-carry-forward state — a one-line note, not a feature.

## Open Questions (for the design session — minor; calls largely locked above)

- **OQ-01 — JSON total field shape.** `{"inbound": N, "outbound": M}` object vs two scalar
  keys (`inbound_edges`/`outbound_edges`). Either satisfies D-05; pick one consistent with
  existing response naming.
- **OQ-02 — summary digest glyph form.** D-08 proposes `edges: 5↑ 2↓ ↔3 (2 authored)`
  (asymmetric out/in arrows + symmetric `↔` count + authored tally). Confirm the exact
  glyph order and whether the authored tally counts the displayed-3 or the full set; pick
  a form consistent with existing entry-line conventions. (The prior Author-asserted /
  Inferred markdown sub-grouping is dropped — ranking front-loads authored — so the old
  per-subgroup-empty question is moot.)
- **OQ-03 — internal-caller default.** Programmatic call sites (the hook path, the
  briefing pipeline, by-ID loop fetches) pay the edge-query cost but never consume the
  next-hop affordance — it's for an agent *reading* an entry, not a machine bulk-fetching.
  Decide whether these internal callers should pass `include_edges:false` by default (and
  *which* ones), so the hot read tool isn't taxed where the affordance is unused. Trades
  off latency (AC-12) against keeping a single default-on code path.

## Dependencies

- **ass-076 FINDINGS** (`product/research/ass-076/SCOPE.md` + `FINDINGS.md`, origin #708) —
  the research input; every D-0x traces to an answered RQ.
- **ass-079 FINDINGS** (`product/research/ass-079/`) — the frozen-Informs-weight rationale
  that grounds D-09's "rank by target confidence, not edge weight" decision.
- **Author-asserted-edge convention** (`uni-architect`, `uni-store-adr`) — the assert half;
  this is the surface half. Pairs with it.
- **vnc-035 carry-forward** (#749) — surfaced view reflects post-carry edge state.
- **`context_graph` `EdgeRecord`** (ADR #4478) + **ADR #4461** (Supersedes SQL exclusion) —
  the shapes/filters this reuses.
- **Relates to #744 / #745** (edge-loss cluster) — this makes their effect observable; no
  ordering dependency either way.

## Tracking

- GitHub Issue: #754
