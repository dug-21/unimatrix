# Agent Report: vnc-015-agent-3-read-rs

**Agent ID:** vnc-015-agent-3-read-rs
**Task:** Implement Components 5 and 7 — both modifying `crates/unimatrix-store/src/read.rs`
**Branch:** feature/vnc-015
**Commit:** 03a460f8

---

## Files Modified

- `crates/unimatrix-store/src/read.rs`

---

## Component 5: stale_dependency_edges

### What was done

1. Added field `pub stale_dependency_edges: u64` to `GraphCohesionMetrics` struct with doc comment.

2. Added Query 3 in `compute_graph_cohesion_metrics()` — a JOIN between `graph_edges` and `entries` counting Prerequisite edges with a Deprecated source:
   ```sql
   SELECT COUNT(*)
   FROM graph_edges ge
   JOIN entries e ON e.id = ge.source_id
   WHERE ge.relation_type = 'Prerequisite'
     AND e.status = 1
   ```
   Hardcoded string literals (no format-string interpolation — SQL injection guard per RISK-TEST-STRATEGY.md).

3. Added `query_stale_prerequisite_edges_for_cycle(feature_cycle: &str) -> Result<Vec<(u64, u64)>>` — the per-cycle scoped variant for the DependencyOnDeprecated detection rule injection. Joins `feature_entries` on `entry_id = ge.source_id`. Returns `(source_id, target_id)` pairs.

4. Added 7 unit tests covering: zero edges, active source not counted (R-14 critical filter-direction check), deprecated source counts, multiple deprecated sources, only Prerequisite type counts (not Advances/Supports), quarantined source not counted, active/deprecated mix.

---

## Component 7: query_contradicts_edges_for_entry Bidirectional Fix

### Caller audit result

```
grep -rn "query_contradicts_edges_for_entry" crates/ --include="*.rs"
```

**Result: 1 location — the function definition itself at `read.rs:1525`. No external callers exist in the workspace.**

The function is defined but not called from any other module. The behavior change carries zero caller regression risk.

### Current query (before fix)

The function queried UNIDIRECTIONALLY:
```sql
WHERE target_id = ?1 AND relation_type = 'Contradicts'
```
This returned only rows where the given entry was the TARGET — missing rows where it was the SOURCE.

### After fix

Bidirectional OR clause:
```sql
WHERE (source_id = ?1 OR target_id = ?1) AND relation_type = 'Contradicts'
```

Added a comment explaining the pre-vnc-015 / post-vnc-015 transition period rationale.

### 6 unit tests added

- `test_query_contradicts_returns_source_direction` — source direction found via OR
- `test_query_contradicts_returns_target_direction` — target direction found via OR (transition compat)
- `test_query_contradicts_bidirectional_post_vnc015` — both A→B and B→A returns 2 rows
- `test_query_contradicts_both_endpoints_return_same_rows` — both endpoints return 2 rows
- `test_query_contradicts_only_contradicts_relation_type` — Supports edges not leaked
- `test_query_contradicts_no_results_for_unrelated_entry` — unrelated entry returns 0

---

## Tests

**308 passed, 0 failed** (unimatrix-store, including 13 new tests added in this task).

Cargo clippy: 0 warnings.

---

## Issues / Blockers

None. The workspace has a pre-existing compile error in `unimatrix-server` from the `default_rules()` signature change (Component 6 — different agent). This is expected; `unimatrix-store` builds and tests cleanly in isolation.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found pattern #3600 (test helper pitfall for pre-v13 schema), #4421/4422/4426 (vnc-015 ADRs confirming constructor injection, edge_write.rs extraction, failure posture).
- Stored: Superseded entry #2934 → new entry **#4432** "cargo fmt on a partially-broken workspace silently reverts all file changes" via `/uni-store-pattern`. Extended with vnc-015 recurrence and recovery protocol: grep-verify critical insertions after every fmt call.
