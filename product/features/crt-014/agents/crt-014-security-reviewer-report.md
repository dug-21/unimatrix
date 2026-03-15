# Security Review: crt-014-security-reviewer

## Risk Level: low

## Summary

crt-014 adds a supersession DAG (`petgraph` + `graph.rs`) to replace two hardcoded scalar penalty constants with topology-derived penalty multipliers. The change operates entirely within the existing trust boundary: inputs flow from an internal SQLite store through typed Rust structs, not from external callers. No injection, path traversal, deserialization, or access control risks were found. Three low-severity observations are noted; none are blocking.

---

## Findings

### Finding 1: `graph_opt.as_ref().unwrap()` — invariant is safe

- **Severity**: info (not a risk)
- **Location**: `crates/unimatrix-server/src/services/search.rs` lines 337, 377
- **Description**: `graph_opt` is `None` only when `use_fallback == true`. Both call sites that call `.unwrap()` are guarded by `if use_fallback { ... } else { graph_penalty(..., graph_opt.as_ref().unwrap(), ...) }`. The invariant holds structurally: the two booleans were set together in a `match` on the same `Result`. The code compiles without `Option::unwrap` warnings precisely because the compiler cannot statically prove the invariant, but the logic is sound.
- **Recommendation**: The pattern is safe. If the team wants to make it statically provable, refactor to `if let Some(graph) = &graph_opt { ... }` in the `else` branch. This is a code-quality preference, not a security concern.
- **Blocking**: no

### Finding 2: `dfs_active_reachable` has no depth cap

- **Severity**: low
- **Location**: `crates/unimatrix-engine/src/graph.rs:255-284`
- **Description**: The private helper `dfs_active_reachable` (called from `graph_penalty`) performs a DFS with a visited-set guard but no depth limit. The public `find_terminal_active` and `bfs_chain_depth` both honour `MAX_TRAVERSAL_DEPTH`, but `dfs_active_reachable` does not. Because `build_supersession_graph` rejects cycles via `is_cyclic_directed` before returning `Ok`, the visited-set alone guarantees termination on a valid DAG — the traversal cannot loop. The worst-case traversal is bounded by the node count, not by depth.
- **Blast radius**: If a cycle were somehow present in the graph (impossible after cycle detection rejects it), the visited-set would still terminate the DFS. The absence of a depth cap in this specific helper is a documentation gap, not an exploitable condition.
- **Recommendation**: Add a comment noting that `dfs_active_reachable` relies on the post-`is_cyclic_directed` DAG guarantee for termination rather than an explicit depth cap. This prevents future maintainers from introducing a depth-inconsistency bug if the function is ever called outside that invariant. Not blocking.
- **Blocking**: no

### Finding 3: Full-store query on every search call — DoS/perf surface

- **Severity**: low
- **Location**: `crates/unimatrix-server/src/services/search.rs:272-292`
- **Description**: Every `search()` call now executes four sequential `query_by_status` SQL queries (Active, Deprecated, Proposed, Quarantined) to build the supersession graph. The architecture document cites NFR-01 (≤5ms at 1,000 entries). The queries run inside `spawn_blocking` on a dedicated thread pool thread, so they do not block the async executor. No denial-of-service amplification vector exists through MCP inputs because the query is unconditional and does not accept user-supplied filters.
- **Blast radius**: At extreme store sizes (tens of thousands of entries), the four queries could introduce measurable search latency. No data corruption or information-disclosure risk. The NFR benchmark test (mentioned in the risk register) is the correct mitigation.
- **Recommendation**: Confirm the NFR-01 benchmark test is included in the integration test suite. The current implementation is correct and safe; the observation is a performance-at-scale note, not a security finding.
- **Blocking**: no

---

## OWASP Checklist

| OWASP Category | Assessment |
|----------------|-----------|
| Injection (SQL, command, path traversal) | No new SQL constructed from user input. `query_by_status` takes a typed `Status` enum cast to `u8`, not a raw string. No shell commands. No file paths from external input. Clean. |
| Broken access control | Graph construction is gated behind the existing search service call path, which has existing admin/trust-level guards. No new capabilities or MCP tool parameters introduced. |
| Security misconfiguration | No new configuration surface. `petgraph` added with `default-features = false, features = ["stable_graph"]` — feature minimization is correct per ADR-001. |
| Vulnerable components | `petgraph 0.8` — MIT/Apache-2.0, no `unsafe` in the `stable_graph` feature path. No known CVEs for this version. `thiserror = "2"` — widely used, no concerns. |
| Data integrity failures | Cycle detection (`is_cyclic_directed`) prevents corrupted supersession chains from causing traversal failures. Dangling references are skipped with `tracing::warn!`. Defensive re-validation of terminal entry status after graph-based resolution prevents stale-graph TOCTOU. |
| Deserialization risks | No new deserialization. Graph is built from already-deserialized `Vec<EntryRecord>` from the store. |
| Input validation gaps | `graph_penalty` returns `1.0` for unknown node IDs. `find_terminal_active` returns `None` for absent nodes. Depth cap (`MAX_TRAVERSAL_DEPTH = 10`) enforced in `find_terminal_active` and `bfs_chain_depth`. All four `Status` variants covered in full-store query. |
| Secrets/hardcoded credentials | None found. Penalty constants are numeric scoring weights, not credentials. |

---

## Blast Radius Assessment

Worst case if the fix has a subtle bug:

1. **Penalty misclassification**: An entry receives the wrong penalty multiplier (e.g., `ORPHAN_PENALTY` instead of `CLEAN_REPLACEMENT_PENALTY`). Effect: search result ordering degrades silently. No data corruption, no information disclosure, no process crash. Failure mode is soft and detectable through search quality regression.

2. **Graph not built (cycle detected)**: `use_fallback = true` is set. The server falls back to flat `FALLBACK_PENALTY` (0.70) for all penalized entries and single-hop injection. This is the same behavior as pre-crt-014, with a `tracing::error!` log. The search completes successfully.

3. **Full-store query fails**: The `spawn_blocking` result propagates as `ServiceError`, causing the `search()` call to return an error. The client receives a search failure (same as any other store I/O error). No partial state written.

4. **`graph_opt.as_ref().unwrap()` panics**: Structurally impossible — the only way `graph_opt` is `None` is if `use_fallback == true`, in which case the `.unwrap()` call site is in the `else` branch and unreachable.

None of the worst-case scenarios involve data corruption, privilege escalation, or information disclosure.

---

## Regression Risk

**Existing search and briefing behavior that could break:**

1. **Penalty values changed**: The old `SUPERSEDED_PENALTY = 0.5` (hardcoded) is replaced by `CLEAN_REPLACEMENT_PENALTY = 0.40` for depth-1 superseded entries. The old `DEPRECATED_PENALTY = 0.7` is replaced by `ORPHAN_PENALTY = 0.75` for standalone deprecated entries. These are intentional behavior changes with test migration coverage (R-05 verified in the diff). The new constants are named and documented.

2. **Single-hop injection upgraded to multi-hop**: `find_terminal_active` now resolves the full chain. Existing single-hop tests were updated to use `CLEAN_REPLACEMENT_PENALTY`; this is correct. Single-hop still works when the chain length is 1. The defensive re-check (`terminal.status != Status::Active || terminal.superseded_by.is_some()`) guards against stale-graph injection.

3. **`Strict` mode**: Unchanged — the graph construction and penalty logic is entirely inside the `Flexible` branch. Strict mode retains its `retain()` filter with no modifications.

4. **`context_briefing` / `context_search` callers**: No tool signatures changed. The graph construction is internal to `search.rs::search()`. External API surface is unchanged.

---

## Dependency Safety

| Dependency | Version | License | `unsafe` in feature path | Notes |
|-----------|---------|---------|--------------------------|-------|
| `petgraph` | 0.8 | MIT / Apache-2.0 | None in `stable_graph` feature | Feature flag `default-features = false` correctly minimises surface per ADR-001 |
| `thiserror` | 2 | MIT / Apache-2.0 | None | Proc-macro only; no runtime unsafe |

---

## PR Comments
- Posted 1 comment on PR #261
- Blocking findings: no

---

## Knowledge Stewardship

nothing novel to store -- the findings (depth-uncapped internal DFS safe after cycle detection, full-store read per query as intended performance tradeoff) are specific to this PR and not generalizable anti-patterns.
