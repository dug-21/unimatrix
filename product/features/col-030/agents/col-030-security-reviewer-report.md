# Security Review: col-030-security-reviewer

## Risk Level: low

## Summary

col-030 introduces a pure suppression filter (`suppress_contradicts`) into the search hot path that removes lower-ranked members of contradictory knowledge pairs. The change touches no I/O, no deserialization of external input, no authentication paths, no schema, and introduces no new dependencies. No blocking security findings. Two non-blocking observations are noted below.

---

## Findings

### Finding 1: Direct `graph.inner[edge_ref.target()]` index access bypasses the `edges_of_type` abstraction boundary for node ID resolution

- **Severity**: low
- **Location**: `graph_suppression.rs:65`, `graph_suppression.rs:71`
- **Description**: `graph.inner` is declared `pub(crate)`. The suppression function uses it directly to resolve node IDs after obtaining `EdgeReference` values from the `edges_of_type` iterator — `graph.inner[edge_ref.target()]` and `graph.inner[edge_ref.source()]`. ADR-002 mandates `edges_of_type` as the "sole traversal boundary," but the intent of that ADR is to prevent callers from bypassing the edge-type filter (which would expose wrong edge types). Resolving the node weight (the `u64` entry ID stored as node weight in the `StableGraph`) from a `NodeIndex` that was already returned by the approved iterator is a different operation — it reads the node label, not the edges. petgraph's `StableGraph` `Index<NodeIndex>` does not panic if the `NodeIndex` came from within the same graph, which is guaranteed here because `edges_of_type` iterates the same `inner` graph. The pattern is identical to `graph.rs:348-349`, `graph.rs:464`, and `graph.rs:515,562` — pre-existing uses of the same idiom. This is therefore consistent with the existing codebase convention, and poses no panic or data-access risk at runtime. Noting it because the ADR wording ("no direct `.edges_directed()` or `.neighbors_directed()` calls") is edges-only and does not technically cover node label reads, but an ADR update clarifying this distinction would prevent future reviewers from flagging it.
- **Recommendation**: Consider adding a sentence to ADR-002 clarifying that `graph.inner[node_idx]` for reading node labels from a `NodeIndex` already obtained via `edges_of_type` is permitted. No code change required.
- **Blocking**: no

### Finding 2: `contradicting_entry_id` logged as `Some(id)` not bare `id` (observability gap, not security)

- **Severity**: low
- **Location**: `search.rs:942`
- **Description**: The `tracing::debug!` call uses the `?` format specifier for `contradicting_ids[i]` (type `Option<u64>`), producing `Some(42)` rather than `42` in log output. Both the suppressed entry ID and the contradicting entry ID are present in the log line. This meets NFR-05 / FR-09 ("both IDs must appear"). The `Some(...)` wrapper is cosmetic — operators can correlate it — but automated log parsers expecting a bare integer field value will need to strip the wrapper. Noted by the gate-3b reviewer already.
- **Recommendation**: If a structured log parser is added downstream, this field should emit the inner value. Could be addressed by converting `Option<u64>` to `u64` with a sentinel (e.g., `u64::MAX`) or by extracting with `.unwrap_or(0)` before logging.
- **Blocking**: no

---

## OWASP Concerns Evaluated

| Category | Status | Notes |
|----------|--------|-------|
| Injection (SQL, command, path) | None | No SQL, shell commands, file paths, or format strings with external input in changed code |
| Broken access control | None | Suppression operates post-retrieval on entries the caller already has search access to; does not gate access to entries differently than before |
| Security misconfiguration | None | No config toggle added; cold-start guard is always-on when graph is built |
| Vulnerable components | None | No new dependencies introduced; Cargo.toml diff is empty |
| Data integrity failures | None | Suppression is additive removal only; no data written or mutated |
| Deserialization risks | None | No new deserialization; inputs are `&[u64]` derived from in-memory structs |
| Input validation gaps | None | `result_ids` is `&[u64]` — primitive, no injection surface. `graph.node_index.get(&id)` returning `None` is handled with `continue`, not panic |
| Secrets / credentials | None | No hardcoded secrets anywhere in the diff |

---

## Blast Radius Assessment

The worst case scenario is a subtle bug in `suppress_contradicts` that returns a mask with incorrect length (shorter than `result_ids.len()`). This would cause an out-of-bounds index access at `keep_mask[i]` in the Step 10b loop in `search.rs`, panicking the search request for the affected user. The panic would be caught at the service boundary and returned as an error — not a silent data corruption. The AC-01 unit test (empty graph, single entry, standard cases) guards the length invariant before delivery. This failure mode cannot escalate to information disclosure or privilege escalation.

A false-positive Contradicts edge (adversarially crafted NLI input causing a legitimate result to be suppressed) has blast radius of one result removed from one search response per poisoned edge. The DEBUG log line provides the minimum audit trail. This is a pre-existing NLI attack surface, not new to col-030.

---

## Regression Risk

**Low.** The change is guarded by `if !use_fallback` — during cold-start the entire block is bypassed and the result set passes through unchanged. Existing scenarios in the eval harness have no Contradicts edges, so the suppression block is a no-op for all pre-existing search calls. The parallel Vec invariant (`aligned_len = results_with_scores.len()`, single indexed pass, `final_scores` shadow via if-expression) is tested by T-SC-09, which specifically exercises the floor-removal + suppression combo to detect the R-03 and R-07 failure modes.

All 2,185 existing server tests pass. The zero-regression eval gate passes.

---

## Dependency Safety

No new dependencies. `petgraph` and `std::collections::HashSet` are the only libraries used in `graph_suppression.rs`. Both were already present. Cargo.toml is unchanged. `cargo-audit` was not available in the environment at gate-3b time; the gate report notes this and confirms no new crates were added, making the effective CVE delta zero.

---

## PR Comments

- Posted 1 comment on PR #419 via `gh pr review --comment`
- Blocking findings: no

---

## Knowledge Stewardship

- Stored: nothing novel to store -- the `graph.inner[node_idx]` direct access pattern (Finding 1) is a pre-existing codebase convention already used at graph.rs:348-349, graph.rs:464, graph.rs:515, and graph.rs:562. It is not a new anti-pattern introduced by col-030, and the existing codebase implicitly permits it. No generalizable new lesson emerged.
