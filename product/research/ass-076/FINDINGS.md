# FINDINGS: Surfacing an entry's typed graph edges on `context_get` retrieval

**Spike**: ass-076 | **Date**: 2026-06-12 | **Approach**: investigation + design-space evaluation | **Confidence**: directional

## Codebase grounding (file:line)

- **`context_get` handler**: `crates/unimatrix-server/src/mcp/tools.rs:920-1009`. Does `entry_store.get(id)` (single entry, no edges) then `format_single_entry(&entry, ctx.format)` at `tools.rs:957`. Edges are never touched. Surfacing plugs in here.
- **`format_single_entry`**: `mcp/response/entries.rs:13-36` — three arms: Summary (one pipe line, `:16-22`), Markdown (`format_entry_markdown_section`), Json (`entry_to_json`). Single edit site.
- **Entry helpers**: `entry_to_json` (`response/mod.rs:121-138`), `format_entry_markdown_section` (`response/mod.rs:160-191`, footer ends `*Entry #… | Created … | Updated …*`).
- **`context_graph` neighbors (cross-tool shape)**: handler `graph_read_neighbors.rs:90-180`; depth=1 path `neighbors_sql` `:187-222`. Per-edge wire type **`EdgeRecord`** (`graph_read.rs:134-144`): `{source_id, target_id, relation_type, direction("incoming"|"outgoing"), depth, metadata}`; envelope `NeighborsResponse { edges }` (`graph_read.rs:170-172`).
- **Depth-1 SQL**: `query_direct_neighbors` (`graph_queries.rs:200-216`) → `run_outgoing_query`/`run_incoming_query` (`graph_queries_neighbors.rs:13-92`), returning `RawEdgeRow {source_id,target_id,relation_type}` (`graph_queries.rs:73-77`). **Selects only those 3 columns — not `source`, not `target_title`.**
- **`graph_edges` schema**: `db.rs:952-964` — has `source` (provenance) and `created_by` columns; `UNIQUE(source_id,target_id,relation_type)`.
- **Provenance constants**: `read.rs:1803-1855` + `edge_write.rs:28`. `EDGE_SOURCE_AGENT="agent"` (authored) vs `nli`/`co_access`/`cosine_supports`/`S1`/`S2`/`S8` (inferred).
- **Batch-title precedent**: subgraph `fetch_nodes_batch` (`graph_read_subgraph.rs:568-600`) already does `SELECT … FROM entries WHERE id IN (…)`.
- **Authoring convention**: `.claude/agents/uni/uni-architect.md:114-119`, `.claude/skills/uni-store-adr/SKILL.md:98-134` — authored types `Prerequisite`/`Contradicts`/`Supports`, high bar, default-none, zero-edge ADRs normal.
- **Prior ADRs**: #4478 (vnc-018 — `EdgeRecord` location/shape, reuse-intended, `metadata` always null); #4461 (vnc-017 — **Supersedes excluded at SQL level** because `entries.supersedes` is authoritative, `graph_edges` Supersedes rows are derived/rebuilt each tick).

## Findings

### RQ-1 — Surfacing depth
**Answer**: Depth-1 only. **Hypothesis CONFIRMED.**
**Evidence**: Depth-1 is a single indexed SQL pair (`graph_queries_neighbors.rs:13-92`) using `idx_graph_edges_{source,target}_type` (`graph_queries.rs:190-191`) — cheap, stateless, live-fresh. Multi-hop is stateful/heavier: `neighbors_bfs` reads the in-memory `TypedRelationGraph` under `RwLock` with cold-start DB-BFS fallback and tick-window staleness (`graph_read_neighbors.rs:234-460`; briefing #4526 — `context_edge` adds are invisible until next snapshot). Putting BFS on a freshness-critical point read (`tools.rs:952` just read live) would create a staleness mismatch within one response.
**Recommendation**: Surface depth-1 via `query_direct_neighbors(pool, id, &[], Both)`. No depth param on `context_get`; `target_id`s are the entry points into `context_graph`.

### RQ-2 — Per-edge payload
**Answer**: `edge_type`(=`relation_type`), `direction`, `target_id`, **`target_title`**. **`target_title` hypothesis CONFIRMED** (via one batched join). Drop `source_id`/`depth` as anchor-constant.
**Evidence**: Goal is readability at retrieval — `Prerequisite → #4742` forces a second lookup; with a title it's self-describing. Join is cheap with a direct precedent (`fetch_nodes_batch`, `graph_read_subgraph.rs:568-600`): one `SELECT id,title FROM entries WHERE id IN(…)` over a small capped set = one round trip regardless of edge count. `direction` is free (derived `row.source_id == id`, `graph_read_neighbors.rs:205-209`) and essential (depends-on vs depended-on-by). `source_id`=anchor and `depth`=1 always at this layer → noise.
**Recommendation**: `{ edge_type, direction, target_id, target_title }` (+ `source` per RQ-4). Batched title resolve; on unresolved target emit `target_title: null` (dangling edge = signal), don't drop the edge.

### RQ-3 — Supersession de-duplication
**Answer**: Exclude Supersedes at SQL level — already done by `query_direct_neighbors`. No new logic.
**Evidence**: `run_outgoing/incoming_query` already filter `AND relation_type != 'Supersedes'` (`graph_queries_neighbors.rs:22-23, 63-64`); absent `edge_types` resolves to the 15 non-Supersedes types (`graph_read_neighbors.rs:65-83, 124-126`). This is codified by ADR #4461 (SQL-level, not loop-level; `entries.supersedes` authoritative). `entry_to_json` already exposes `supersedes`/`superseded_by` (`response/mod.rs:134-135`) — the sole representation; edge list stays disjoint by construction. Hard constraint satisfied structurally.
**Recommendation**: Call with empty `edge_types`; Supersedes excluded for free; no reconciliation.

### RQ-4 — Inferred vs. authored
**Answer**: Distinguish them. Data exists in `graph_edges.source`; requires a small read-path query extension (currently not selected).
**Evidence**: `source` column (`db.rs:960`) = `"agent"` for authored (`edge_write.rs:28`) vs `nli`/`co_access`/`cosine_supports`/`S1`/`S2`/`S8` for inference (`read.rs:1803-1855`). Trust differs sharply — an authored `Contradicts` carries a one-clause justification (`SKILL.md:116`); `CoAccess` is a co-retrieval statistic promoted at count≥3 (`read.rs:1857-1867`). **Gap**: `RawEdgeRow` + both SQL helpers select only `source_id,target_id,relation_type` (`graph_queries_neighbors.rs:95-107`) — must add `source`. Read-path only, **no migration** (Hard constraint preserved); additive to `context_graph`.
**Recommendation**: Add `source` to the SELECT and `RawEdgeRow`; derive `authored = (source == EDGE_SOURCE_AGENT)`. Markdown: group "Author-asserted" vs "Inferred". Json: flat per-edge `authored` flag. Keep the mapping in one helper so new inference sources default to inferred.

### RQ-5 — Response size / default-on
**Answer**: **Default-on. Hypothesis CONFIRMED.** Small cap + always-present uncapped `total`; zero explicit.
**Evidence**: The loop only closes if edges are visible without opting in — authored edges are deliberately rare/default-none, so opt-in means nobody sees the empty box (defeats "zero-edge becomes visible"). Size risk is bounded by precedent: DB-BFS caps fan-out at `MAX_DB_NEIGHBORS_PER_NODE=1000` (`graph_read_neighbors.rs:370`); redirect path uses cap+report-total+"truncated" (`format_redirect_summary`, `entries.rs:265-298`). Get-layer cap should be far smaller (human-readable lookup, not a dump). Zero must be a visible "No related entries" line (inverts the existing omit-at-zero convention because here visibility *is* the mechanism).
**Recommendation**: Default-on, no flag (don't extend `GetParams`). Cap **10** + always emit `total`. Zero → explicit "No related entries" / `"edges":[],"total":0`. Ranked: (1) cap 10+total [recommended]; (2) cap 20+total [conservative]; (3) counts-only-behind-flag [rejected — reintroduces opt-in].

### RQ-6 — Format variants
**Answer**: All three defined; only edit site is `format_single_entry`.
**Evidence**: `entries.rs:13-36` arms; markdown footer ends `*Entry #…*` (`response/mod.rs:184-189`); `context_graph` neighbors ignores `format` and emits raw JSON (`graph_read.rs:301-306`), so `context_get` *establishes* the summary/markdown edge renderings.
**Recommendation**:
- **summary/null**: append count digest to the `#id | title | …` line — `… | edges: 3↑ 1↓ (2 authored)`; zero → `… | edges: none`. Counts only.
- **markdown**: after the footer, `### Related` with sub-groups **Author-asserted** then **Inferred**; line `- {edge_type} {→|←} #{target_id} "{target_title}"`; if capped `_…and N more (12 total)_`; if none `_No related entries._`.
- **json**: add `"edges":[{edge_type,direction,target_id,target_title,authored}]` + `"total_edges"` to the entry object; zero → `"edges":[],"total_edges":0`. Wrap/branch rather than mutate shared `entry_to_json` (see Out-of-Scope).

### RQ-7 — Cross-tool consistency
**Answer**: Align names/semantics to `EdgeRecord`; present a get projection (no `source_id`/`depth`; add `target_title`/`authored`). One vocabulary, not two.
**Evidence**: neighbors returns `NeighborsResponse { edges: Vec<EdgeRecord> }` (`graph_read.rs:134-172`); ADR #4478 placed `EdgeRecord` for reuse (subgraph already reuses). Match exactly: `relation_type`, `target_id`, `direction` with `"incoming"|"outgoing"` (`graph_read.rs:139`). Get adds `target_title`/`authored`, omits anchor-constant `source_id`/`depth`/`metadata` — a superset-minus-constants, not a competing schema. Hard constraint (don't break neighbors) satisfied: this adds a consumer.
**Recommendation**: Define the get edge as a documented projection of `EdgeRecord`; reuse `query_direct_neighbors` so both tools share one query + one direction rule. Document the projection in both tool descriptions. Carry-forward: later retrofit neighbors with `target_title`/`authored` for full symmetry (not required here).

## Unanswered Questions
None — all seven answered with code evidence at directional confidence. Three open design *choices* (not blockers) for the design session: RQ-5 cap value (10 vs 20); RQ-4 provenance surface (boolean `authored` vs `provenance` enum); RQ-6 whether to also extend search/lookup JSON with edges (left out of scope).

## Out-of-Scope Discoveries
- **`context_graph` neighbors ignores `format`** (`graph_read.rs:301-306`) — ass-076 creates the first summary/markdown edge rendering; neighbors could later reuse it. Carry-forward.
- **Neighbor SQL drops `source`** (`graph_queries_neighbors.rs:95-107`) — once added for RQ-4, neighbors could expose authored-vs-inferred too (the always-null `metadata` slot, `graph_read.rs:142-143`, is the carrier). Carry-forward.
- **vnc-035 carry-forward is benign** — surfaced view reads live `graph_edges` at get time, so it auto-reflects post-carry-forward state (#749, MEMORY/#4985). One-line note in design, not a feature.
- **`entry_to_json` is shared** by search/lookup/store/correct (`response/mod.rs:121`) — mutating it to add edges changes all those payloads. Get extension should wrap/branch, unless design deliberately wants edges everywhere. Flagged for a conscious call.

## Recommendations Summary
- **RQ-1**: depth-1 only; reuse `query_direct_neighbors`; multi-hop stays in `context_graph`. Confirmed.
- **RQ-2**: `{edge_type, direction, target_id, target_title}`; batched title join (reuse `fetch_nodes_batch` pattern); drop anchor-constant `source_id`/`depth`. `target_title` confirmed.
- **RQ-3**: exclude Supersedes at SQL via empty `edge_types` (inherits `!= 'Supersedes'`, ADR #4461); `supersedes`/`superseded_by` stay sole representation.
- **RQ-4**: distinguish via `graph_edges.source` (`"agent"`=authored, else inferred); add `source` to SELECT/`RawEdgeRow` (read-path only, no migration); surface `authored` (json) + grouped sections (markdown).
- **RQ-5**: default-on, no flag; cap **10** + always-present uncapped `total`; render zero explicitly. Confirmed.
- **RQ-6**: summary = count digest; markdown = `### Related` split authored/inferred with "…N more"; json = `edges[]` + `total_edges`.
- **RQ-7**: get edge = documented projection of `EdgeRecord` (same `relation_type`/`target_id`/`direction`, +`target_title`/`authored`, −anchor-constants); share `query_direct_neighbors` across both tools.

**All three Hypothesis constraints CONFIRMED** with evidence (default-on RQ-5, depth-1 RQ-1, include `target_title` RQ-2). The one net-new implementation cost surfaced: adding the `source` column to the neighbor SELECT/`RawEdgeRow` for RQ-4 — read-path only, no schema migration.
