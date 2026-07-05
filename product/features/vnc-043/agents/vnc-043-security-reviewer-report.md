# Security Review: vnc-043-security-reviewer

PR #909 · branch `feature/vnc-043` · GH #903. Fresh-context review.

## Risk Level: low

## Summary
The depth-1 live dispatch + doc-drift fix introduces no new security surface. The promoted live path is fully parameterized SQL, the `require_cap(Read)` gate is unchanged, all input validation precedes the new dispatch, no dependencies or secrets are added, and blast radius stays read-only and capped. No blocking findings.

## Findings

### F1 — Injection: promoted depth-1 SQL path is parameterized
- **Severity**: low (informational — no vulnerability)
- **Location**: `crates/unimatrix-store/src/graph_queries_neighbors.rs:13,54`; `graph_queries.rs:219`; `graph_read_subgraph.rs:484,639,691`
- **Description**: `max_depth == 1` now routes every subgraph call through `subgraph_via_db` → `query_direct_neighbors` → `run_outgoing_query`/`run_incoming_query`. All SQL is parameterized: `IN (...)` placeholders are generated from `edge_types.len()` (arity only, never content), every value is `.bind()`-bound, `id` is bound. `edge_types` is additionally pre-validated against the `RelationType` enum in `handle_subgraph` (unknown types → `ERROR_INVALID_PARAMS`) before dispatch. `fetch_nodes_batch`/`fetch_edge_metadata` use the same positional-placeholder pattern. No untrusted string interpolation into SQL.
- **Recommendation**: none.
- **Blocking**: no

### F2 — Access control unchanged
- **Severity**: low (informational)
- **Location**: `crates/unimatrix-server/src/mcp/tools.rs:4021`; dispatch at `graph_read_subgraph.rs:171`
- **Description**: `require_cap(&ctx.agent_id, Capability::Read)` still runs before `handle_graph`. The new `max_depth == 1` early return is inside `handle_subgraph`, well past the gate — no bypass. The path reads via `store.read_pool_server()` (read-only pool); no write surface introduced.
- **Recommendation**: none.
- **Blocking**: no

### F3 — Input validation precedes dispatch
- **Severity**: low (informational)
- **Location**: `graph_read_subgraph.rs:76-162` (validation) → `:171` (dispatch)
- **Description**: seed_ids non-empty, `max_depth` 1..=10, `max_nodes` 1..=200, `direction` enum, `edge_types` enum-parse all execute before the depth-1 early return. Exact `== 1` match cannot capture depth>1; the depth>1 cold-start `use_fallback` branch is byte-unchanged. Deserialization of edge metadata uses `serde_json::from_str(...).ok()` → `None` on malformed input, no panic.
- **Recommendation**: none.
- **Blocking**: no

### F4 — README resolve_supersessions default is stale (doc accuracy, not security)
- **Severity**: low
- **Location**: `README.md` context_graph row vs. `graph_read_subgraph.rs:162`
- **Description**: the rewritten README row lists `resolve_supersessions (… subgraph … — default false)`, but subgraph defaults it to **true** (`.unwrap_or(true)`, per bugfix-881). This feature's theme is closing #903 doc drift and this PR rewrote that exact row, so the inaccuracy is worth correcting — but it is pre-existing and outside security scope.
- **Recommendation**: correct the README default to `true` for subgraph/neighbors (non-blocking, editorial).
- **Blocking**: no

## Blast Radius Assessment
Worst case from a subtle bug in the `max_depth == 1` dispatch or the shared `sort_subgraph_output`: a wrong or partial ordering/membership of a graph neighborhood the caller already holds `Read` on — bounded at 200 nodes / 1000 edge-metadata rows / 1000 neighbors-per-node. Not information disclosure beyond the existing Read cap, not data corruption (read-only pool, no write), not privilege escalation. `sort_subgraph_output` operates on `&mut [T]` slices (sort-in-place), so it cannot add or drop set members; its edge comparator is a total order over `(source_id, target_id, &relation_type)`, so no `sort_by` panic risk. The live path is strictly safer than the warm path on the `MAX_EDGES_UPPER` guard — it hard-`truncate`s to 1000 (`graph_read_subgraph.rs:567`) where the warm path relies on a `debug_assert!` only.

## Regression Risk
Depth-1 subgraph moves from the in-memory tick cache to a live per-query SQL BFS — the intended freshness change, mirroring `neighbors` depth-1 (ADR-005 vnc-018). Both paths share seed phase, R-02 dedup, R-05 dangling filter, hydration (incl. tags via ADR-006), metadata, and ordering, so the node/edge SET is preserved (only freshness differs). Non-existent seeds and empty/`[]` edge_types behave identically on both paths. depth>1 (warm BFS + cold-start `use_fallback`) is untouched at the SET level. The risk strategy covers these via R-01/R-02 dispatch + R-03 dual-path SET-parity + R-04 promoted-path regression scenarios. No new deps (verified: no Cargo.toml/Cargo.lock change), no secrets.

## PR Comments
- Posted 1 review comment (state COMMENTED) on PR #909 via gh CLI.
- Blocking findings: no.

## Knowledge Stewardship
- Queried: reviewed vnc-043 RISK-TEST-STRATEGY stewardship notes (#5396 mirror-const byte-equality guard, #4474 execution-path-asymmetry description text, #4473 warn+continue masking) — already recorded.
- Stored: nothing novel to store — the parameterized-SQL-on-promoted-path and doc-drift patterns for this change are already captured by the existing entries above; no cross-feature security anti-pattern emerges that isn't recorded.
