# Risk-Based Test Strategy: vnc-043

context_graph subgraph — Class-1 doc fix + live depth-1 read via `subgraph_via_db` reuse (GH #903).
Narrow feature: handler dispatch + doc text + tests; no wire/struct/hot-path change. Risk strategy is
proportionate — the concentration is on the two structural hazards (promoted load-bearing path, four-point
doc drift), not on re-validating already-shipped filter logic.

## Risk Register

| Risk ID | Risk Description | Severity | Likelihood | Priority |
|---------|-----------------|----------|------------|----------|
| R-01 | Dispatch capture error — a `<=1`/range guard, or placement after the lock block, captures depth>1 or is skipped for `max_depth==1` | High | Low | High |
| R-02 | Depth>1 cold-start `use_fallback` branch broken/bypassed by the inserted early-return (empty `TypedRelationGraph` no longer falls back to live) | High | Low | High |
| R-03 | Dual-path SET divergence — depth-1 live returns a different node/edge set than the prior cache-BFS for the same seed+filter (edge_types absent/`[]`, Supersedes exclusion, `resolve_supersessions`) | High | Med | Critical |
| R-04 | Promoted load-bearing path latent bug — R-02 dedup (`direction:both` double edges), R-05 dangling filter on mid-hop `max_nodes` cap, `MAX_EDGES_UPPER` metadata cap — now fires on every board read, not just cold-start | High | Med | Critical |
| R-05 | Hydration/tag parity break — depth-1 `EntryRecord` missing a field or tags (`load_tags_for_entries`, ADR-006) vs the cache path | Med | Low | Medium |
| R-06 | Ordering applied to only one path, or an existing fixed-order depth>1 test breaks under the new uniform sort | Med | Med | High |
| R-07 | Four-point doc drift — two description literals (byte-equality guarded) + two schemars docs (NOT guarded); editing three of four, or the two literals diverging, silently reopens the #903 root cause | High | Med | Critical |
| R-08 | Depth-1 path inadvertently acquires the `TypedGraphState` lock, violating "no hot-path touch" (A3/NFR-2/AC-10) | Med | Low | Medium |
| R-09 | Silent truncation at high fan-in — `max_nodes` default (200) caps seed+one-hop and `truncated` is not surfaced/asserted, returning a partial board silently | Med | Low | Medium |
| R-10 | Freshness split not tested both ways — depth-1 write-then-read visibility and depth>1 within-tick staleness rot silently as future edits touch the tick path | Med | Med | High |
| R-11 | Direction label leak — `direction` filter alters the returned `EdgeRecord` label instead of only inclusion (canonical `source→target`, `direction:"outgoing"` must be invariant) | Med | Low | Medium |

## Risk-to-Scenario Mapping

### R-01: Dispatch capture error
**Severity**: High **Likelihood**: Low **Impact**: depth>1 silently served from live per-query BFS (perf/contract regression) or depth-1 never routed live (feature no-op, #903 unclosed).
**Test Scenarios**:
1. `max_depth==1` routes through `subgraph_via_db` (path assertion / it reflects a pre-call write — AC-01/AC-07).
2. `max_depth==2` and `max_depth==10` do NOT route live — served from cache (AC-02); a within-tick write is NOT visible.
3. Boundary: `max_depth==0` and absent `max_depth` — confirm validation rejects/defaults as today, exact `==1` never captures them.
**Coverage Requirement**: exact `==1` dispatch proven at 1 and at ≥2 boundaries, placed before the lock block.

### R-02: Depth>1 cold-start fallback broken
**Severity**: High **Likelihood**: Low **Impact**: depth>1 on a cold/empty graph returns empty instead of falling back to live (#4562/GH #623 regression).
**Test Scenarios**:
1. depth>1 query against an empty `TypedRelationGraph` still fires `use_fallback` → `subgraph_via_db` and returns the live neighborhood.
2. depth>1 warm-graph BFS path unchanged (byte-for-byte behavior, same set as pre-change).
**Coverage Requirement**: cold-start fallback fires at depth>1 post-insertion; warm depth>1 non-regression.

### R-03: Dual-path SET divergence
**Severity**: High **Likelihood**: Med **Impact**: a silent behavior change ships — the board shows different rows depending on tick timing. Historical analog: #4167-class inclusive-filter undercount.
**Test Scenarios**:
1. Same seed + `edge_types` + `direction` + `resolve_supersessions` on a warm+fresh graph (no within-tick writes): depth-1 live node SET == prior cache-BFS node SET, edge SET == edge SET (order-independent).
2. `edge_types` absent vs `[]` — both yield all types except Supersedes, identically on both paths.
3. `resolve_supersessions` default-true vs explicit-false — identical resolution on depth-1 live.
**Coverage Requirement**: set-equality parity across paths for absent/empty/explicit filter and both supersession modes. Freshness is the ONLY permitted difference.

### R-04: Promoted load-bearing path latent bug
**Severity**: High **Likelihood**: Med **Impact**: a formerly cold-start-only bug now corrupts every capability-board read.
**Test Scenarios**:
1. R-02 dedup: `direction:both` on a node with one physical bidirectional-eligible edge returns exactly one `EdgeRecord`, no duplicate.
2. R-05 dangling filter: `max_nodes` caps mid-hop → edges pointing at dropped nodes are filtered, no dangling `EdgeRecord`.
3. `MAX_EDGES_UPPER` (1000) metadata OR-chain guard respected at depth-1 (no over-cap query).
**Coverage Requirement**: dedup, dangling-filter, and metadata-cap each exercised ON the depth-1 live path — not inferred from the cold-start-only history.

### R-05: Hydration/tag parity break
**Severity**: Med **Likelihood**: Low **Impact**: board rows miss `tags`/`status`/`kind`, breaking the curator read.
**Test Scenarios**:
1. Depth-1 node carries id, title, content, status, kind, tags — field-for-field equal to the cache-path hydration of the same node (tagged fixture).
**Coverage Requirement**: full `EntryRecord` field + tag parity on a tagged node.

### R-06: Ordering non-uniform / breaks fixed-order test
**Severity**: Med **Likelihood**: Med **Impact**: DoD one-shot flakes, or depth-1 vs depth>1 diverge in serialized order; or an existing depth>1 test red-bars.
**Test Scenarios**:
1. Depth-1 nodes ascending by `id`, edges ascending by `(source_id, target_id, relation_type)` — DoD one-shot run twice is byte-identical.
2. Depth>1 output carries the SAME ordering keys (one contract across paths).
3. Sweep existing subgraph tests for a fixed-order assertion; any hit updated as presentation-only (set unchanged).
**Coverage Requirement**: deterministic order proven on both depths; no surviving fixed-order assertion that the uniform sort would flip incorrectly.

### R-07: Four-point doc drift
**Severity**: High **Likelihood**: Med **Impact**: the #903 root cause (contract mis-documents an available filter) silently reopens. Supported by #4474 (execution-path-asymmetry tools need exact description text) and #5396 (the byte-equality guard exists because these two literals already drifted once).
**Test Scenarios**:
1. `test_graph_tool_attr_description_matches_const` (#869) stays green — the two description literals are byte-identical after both edits.
2. Substring assertions (`tools.rs:6198+`) extended to require BOTH the filter-availability text and the depth-1-live/depth>1-cache staleness text; assertions present in the description.
3. `edge_types` (`graph_read.rs:85`) and `direction` (`graph_read.rs:82`) schemars docs assert subgraph applicability — these two are NOT covered by the byte-equality guard, so verify them explicitly (doc-string presence or a schema-doc assertion).
**Coverage Requirement**: all four edit points verified — two via the byte-equality guard + extended substrings, two schemars docs via explicit presence check. No point left to manual sync.

### R-08: Depth-1 acquires the lock
**Severity**: Med **Likelihood**: Low **Impact**: weakens the AC-10 "no hot-path touch" claim; potential contention on the read path.
**Test Scenarios**:
1. Code/path review: the `max_depth==1` early return precedes the lock/snapshot block; no `.read()` on `TypedGraphState` on the depth-1 path.
**Coverage Requirement**: depth-1 path takes zero `TypedGraphState` lock (structural assertion / review checklist item).

### R-09: Silent truncation at high fan-in
**Severity**: Med **Likelihood**: Low **Impact**: a >199-neighbor goal returns a partial board with no signal.
**Test Scenarios**:
1. AC-15: ≥30 incoming `Advances` capabilities on the seed → `truncated == false`, all present.
2. Pathological >199-neighbor fixture → `truncated == true` surfaced (partial, not silent).
**Coverage Requirement**: `truncated` asserted both false (realistic ≥30) and true (over-cap) — never unchecked.

### R-10: Freshness split not tested both ways
**Severity**: Med **Likelihood**: Med **Impact**: the ADR-005 asymmetry contract rots undetected. #4473-class: an untested behavioral branch silently degrades.
**Test Scenarios**:
1. depth-1: write edge → immediately query → edge appears (AC-11 forward).
2. depth>1: write edge within the tick window → query → edge does NOT appear (staleness preserved).
**Coverage Requirement**: both directions asserted in one test module; mandatory per ADR-005 precedent.

### R-11: Direction label leak
**Severity**: Med **Likelihood**: Low **Impact**: filter semantics bleed into presentation; downstream id-join breaks.
**Test Scenarios**:
1. `direction:"incoming"` returns the incoming neighbors but each `EdgeRecord` keeps canonical `source_id → target_id` with `direction:"outgoing"` label — inclusion changed, label invariant.
**Coverage Requirement**: label invariant asserted across incoming/outgoing/both at depth-1.

## Integration Risks

- **Handler↔helper contract (R-01/R-02):** the insertion at `graph_read_subgraph.rs:162` sits on the boundary between `handle_subgraph` param resolution and the `subgraph_via_db` reuse. `petgraph_dirs`/`edge_types`/`resolve_supersessions` must be fully resolved before the early return — an ordering slip passes stale/default filter args to the live path. Covered by R-03 parity + R-01 dispatch tests.
- **`subgraph_via_db` dual-caller (R-04):** the function now serves two callers (depth-1 unconditional + depth>1 cold-start). Any change to satisfy depth-1 must not alter cold-start behavior — regression-cover both callers.
- **`query_direct_neighbors` reuse (AC-03):** depth-1 must issue the same single edge query neighbors d1 uses — no per-edge round-trips. Path/query-count assertion.

## Edge Cases

- `max_depth == 0` / absent — must be rejected/defaulted by existing validation; exact `==1` never captures (R-01 scenario 3).
- Empty `seed_ids` — behavior identical to today on the live path (no panic, empty/validation error).
- Non-existent seed id — dangling seed produces empty neighborhood, not an error, matching cache path.
- `max_nodes == 0` — truncation/empty semantics consistent with `subgraph_via_db` today.
- Self-loop edge and duplicate `seed_ids` — dedup (R-02) yields no duplicate edges/nodes.
- `edge_types` absent vs `[]` — both = all types except Supersedes (R-03 scenario 2).

## Security Risks

External input surface: `GraphParams` (`seed_ids`, `edge_types`, `direction`, `max_nodes`, `max_depth`) flowing into `subgraph_via_db` → `query_direct_neighbors`.
- **Untrusted input:** all fields; `edge_types` validated against the `RelationType` enum (rejects unknown types), `direction` parsed to `petgraph` dirs, `max_nodes` hard-capped at 200, `max_depth` constrained 1..=10 — validation is pre-existing and unchanged (AC-12 keeps other modes' wiring intact).
- **Injection:** `query_direct_neighbors` is parameterized SQL — no string interpolation of seed ids/edge types. The doc-only edits add no new input path.
- **Blast radius:** read-only, gated by `require_cap(Read)`; a compromised/malformed call can at worst read graph neighborhoods the caller already has Read on, capped at 200 nodes / 1000 edge-metadata rows. No write, no path/file surface, no deserialization of untrusted structured data beyond existing `GraphParams`.
- **Residual:** none introduced by this feature; the reuse routes through the same validated helper. No new security test beyond confirming validation still rejects an unknown `edge_types` value on the depth-1 path.

## Failure Modes

- **Filter arg resolution fails** → existing validation returns `ErrorData` before dispatch; depth-1 path never reached with bad args.
- **Live DB read errors at depth-1** → `subgraph_via_db` propagates the `Result` error exactly as it does on the cold-start path today; no new swallow/warn-continue.
- **Over-cap fan-in** → `truncated == true` surfaced, partial-but-signaled board (R-09), never a silent partial.
- **Cold/empty graph at depth>1** → `use_fallback` fires to live (R-02); at depth-1 the live path is unconditional so cold-start is a non-issue.

## Scope Risk Traceability

| Scope Risk | Architecture Risk | Resolution |
|-----------|------------------|------------|
| SR-01 (four-point mirror-const/schemars doc drift) | R-07 | Byte-equality guard #869 (#5396) covers the two description literals; extended substrings + explicit schemars-doc checks cover all four points. ADR-002 vnc-043. |
| SR-02 (`subgraph_via_db` promoted to load-bearing) | R-04, R-05 | Dedup/dangling/metadata/hydration regression-covered ON the depth-1 live path, not just the DoD happy one-shot. ADR-001 vnc-043. |
| SR-03 (ordering non-determinism) | R-06 | Uniform sort (nodes by id, edges by canonical triple) applied to both depths; existing fixed-order tests swept. ADR-003 vnc-043. |
| SR-04 (external snapshot/schema pin) | R-08 (doc) / R-07 | Architecture verified NO `.snap`/`insta`/`schema_for` pin exists (Open Q4 resolved); only in-crate substring + byte-equality pins, both handled in-scope. |
| SR-05 (max_nodes silent truncation) | R-09 | Concrete threshold set: ≥30 realistic (truncated==false), >199 pathological (truncated==true surfaced). ADR-003 vnc-043. Board caller does not raise max_nodes. |
| SR-06 (regression to shipped filter path) | R-03, R-11 | Dual-path SET-parity + direction-label-invariant assertions; absent/`[]`/Supersedes-exclusion parity. |
| SR-07 (regression to depth>1 cache/cold-start) | R-01, R-02 | Exact `==1` dispatch before the lock; depth>1 cold-start fallback regression test on empty graph. ADR-001 vnc-043. |
| SR-08 (behavioral-split test debt) | R-10 | Both-direction freshness test mandatory (d1 visible, d>1 within-tick not visible). |

Assumptions A1 (filter already ships/correct), A2 (`subgraph_via_db` satisfies all non-freshness contracts), A3 (depth-1 takes no lock) are load-bearing and mapped to R-03 (A1/A2 divergence would surface as SET mismatch), R-04 (A2 sub-contract gaps), R-08 (A3).

## Coverage Summary

| Priority | Risk Count | Required Scenarios |
|----------|-----------|-------------------|
| Critical | 3 (R-03, R-04, R-07) | 9 |
| High | 4 (R-01, R-02, R-06, R-10) | 9 |
| Medium | 4 (R-05, R-08, R-09, R-11) | 5 |
| Low | 0 | 0 |

## Knowledge Stewardship
- Queried: /uni-knowledge-search (context_search) for graph-dispatch gate lessons, dual-path parity patterns, mirror-const drift -- key hits #5396 (byte-equality guard for mirror-const, vnc-042/bugfix-869), #4474 (execution-path-asymmetry MCP tools need exact description text, vnc-018), #4473 (warn+continue masks missing failure-path tests, vnc-017).
- Stored: nothing novel to store -- the drift-guard pattern (#5396) and the execution-path-asymmetry description pattern (#4474) already capture this feature's cross-cutting risks; no 2+-feature pattern emerges that isn't already recorded.
