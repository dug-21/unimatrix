# Pseudocode: search.rs (MODIFIED)

**File**: `crates/unimatrix-server/src/services/search.rs`
**Change type**: MODIFY (diff-style — only changed regions shown)
**Preserves**: All crt-018b additions (EffectivenessState snapshot, utility_delta, generation cache, lock ordering)

---

## Purpose

Replace hardcoded `DEPRECATED_PENALTY`/`SUPERSEDED_PENALTY` scalar usage in Steps 6a and 6b with topology-derived graph calls. Add graph construction before Step 6a. Update tests that reference the removed constants.

---

## Change 1: Import Line (line 18)

**Remove**:
```
use crate::confidence::{DEPRECATED_PENALTY, SUPERSEDED_PENALTY, cosine_similarity, rerank_score};
```

**Replace with**:
```
use crate::confidence::{cosine_similarity, rerank_score};
use unimatrix_engine::graph::{
    build_supersession_graph, find_terminal_active, graph_penalty, GraphError, FALLBACK_PENALTY,
};
```

Note: `ORPHAN_PENALTY`, `DEAD_END_PENALTY`, etc. are NOT imported into `search.rs` — only `FALLBACK_PENALTY` is needed here. `graph_penalty` returns the appropriate constant value internally.

---

## Change 2: Graph Construction (before Step 6a, around line 264)

Insert this block immediately before the `// Step 6a:` comment block, still inside `search()`.

**Context**: this sits after the `results_with_scores` Vec is populated (Step 6, quarantine filter) and before the `match params.retrieval_mode` block.

```
// crt-014: Load all entries for supersession graph construction.
// Required for topology-derived penalties and multi-hop successor injection.
// Full-store read is synchronous (Store uses Mutex<Connection>); runs in the
// spawn_blocking context established around this synchronous work (ADR-002).
// QueryFilter::default() returns all entries regardless of status (IR-01).

let store_for_graph = Arc::clone(&self.store);
let (all_entries, graph_result) =
    tokio::task::spawn_blocking(move || {
        let entries = unimatrix_core::Store::query(
            &store_for_graph,
            QueryFilter::default(),
        )?;
        let graph = build_supersession_graph(&entries);
        Ok::<_, ServiceError>((entries, graph))
    })
    .await
    .map_err(|e| ServiceError::Internal(e.to_string()))??;

// Unpack graph result — on cycle, activate fallback mode (ADR-005).
let (graph_opt, use_fallback) = match graph_result {
    Ok(graph) => (Some(graph), false),
    Err(GraphError::CycleDetected) => {
        tracing::error!(
            "supersession cycle detected in knowledge graph — \
             search falling back to flat FALLBACK_PENALTY"
        );
        (None, true)
    }
};
```

### Implementation Notes

- The `Store::query` call must use the raw `Store` type (via `Arc<Store>`), not the async `AsyncEntryStore` wrapper, because this runs inside `spawn_blocking`. Check which `Store` accessor is available on `SearchService` — the existing field is `store: Arc<Store>`.
- `QueryFilter::default()` must return entries of all statuses including Deprecated and Quarantined (IR-01). Verify this against `unimatrix-store/src/read.rs:282`.
- The `ServiceError::Internal` variant may need to be added if not present. Check `ServiceError` definition — if it does not have `Internal(String)`, use the closest variant (e.g., `ServiceError::Core(...)` or wrap via an existing error path). Flag this as a gap if ServiceError has no generic internal variant.
- If `Store::query` is only available as a method on `Store` directly and not through the async wrapper, use `self.store.query(QueryFilter::default())` inside the closure. The `store` field is `Arc<Store>`, and `Store::query` takes `&self`.

---

## Change 3: Step 6a — Penalty Map (Flexible mode, no explicit status filter)

**Current code** (lines ~283–296 in the worktree):
```rust
RetrievalMode::Flexible => {
    if explicit_status_filter.is_none() {
        for (entry, _) in &results_with_scores {
            if entry.superseded_by.is_some() {
                penalty_map.insert(entry.id, SUPERSEDED_PENALTY);
            } else if entry.status == Status::Deprecated {
                penalty_map.insert(entry.id, DEPRECATED_PENALTY);
            }
        }
    }
}
```

**Replace the inner loop with**:
```
RetrievalMode::Flexible => {
    if explicit_status_filter.is_none() {
        // crt-014: Unified penalty condition (IR-02).
        // Both superseded entries and deprecated entries go through graph_penalty.
        // The OR condition: entry with superseded_by.is_some() may have any status;
        // entry with status==Deprecated and superseded_by.is_none() is an orphan.
        for (entry, _) in &results_with_scores {
            if entry.superseded_by.is_some() || entry.status == Status::Deprecated {
                let penalty = if use_fallback {
                    FALLBACK_PENALTY
                } else {
                    // graph_opt is Some when use_fallback is false
                    graph_penalty(entry.id, graph_opt.as_ref().unwrap(), &all_entries)
                };
                penalty_map.insert(entry.id, penalty);
            }
        }
    }
}
```

### Notes

- The unified condition `entry.superseded_by.is_some() || entry.status == Status::Deprecated` replaces the two separate branches. This is intentional per FR-06 and IR-02.
- An entry with `superseded_by.is_some()` but `status == Active` (superseded-but-marked-active data inconsistency) will now receive a graph penalty — previously it received `SUPERSEDED_PENALTY`. This is the correct behavior per IR-02.
- `graph_opt.as_ref().unwrap()` is safe: `use_fallback` is false only when `graph_opt` is `Some`.
- Active entries with `superseded_by.is_none()` never enter this branch → no performance cost of calling `graph_penalty` for every Active entry (IR-03).

---

## Change 4: Step 6b — Multi-Hop Successor Injection

**Current code** (lines ~299–343 in the worktree):
```rust
if should_inject {
    let successor_ids: Vec<u64> = results_with_scores
        .iter()
        .filter_map(|(entry, _)| entry.superseded_by)
        .collect();

    if !successor_ids.is_empty() {
        let unique_successor_ids: HashSet<u64> = successor_ids.into_iter().collect();
        let existing_ids: HashSet<u64> =
            results_with_scores.iter().map(|(e, _)| e.id).collect();

        let to_fetch: Vec<u64> = unique_successor_ids
            .into_iter()
            .filter(|id| !existing_ids.contains(id))
            .collect();

        for successor_id in to_fetch {
            let successor = match self.entry_store.get(successor_id).await {
                Ok(s) => s,
                Err(_) => continue,
            };
            if successor.status != Status::Active { continue; }
            if successor.superseded_by.is_some() { continue; }  // single-hop only
            if let Some(emb) = self.vector_store.get_embedding(successor_id).await {
                let sim = cosine_similarity(&embedding, &emb);
                results_with_scores.push((successor, sim));
            }
        }
    }
}
```

**Replace with**:
```
if should_inject {
    // crt-014: Multi-hop injection via find_terminal_active.
    // Collect entries that have a superseded_by set (the old single-hop candidates).
    let superseded_entries: Vec<&EntryRecord> = results_with_scores
        .iter()
        .filter_map(|(entry, _)| {
            if entry.superseded_by.is_some() { Some(entry) } else { None }
        })
        .collect();

    if !superseded_entries.is_empty() {
        let existing_ids: HashSet<u64> =
            results_with_scores.iter().map(|(e, _)| e.id).collect();

        for entry in superseded_entries {
            // Resolve terminal: multi-hop via graph, or single-hop fallback on cycle
            let terminal_id: Option<u64> = if use_fallback {
                // Fallback: single-hop (old behavior) — ADR-005
                entry.superseded_by
            } else {
                // Multi-hop: follow chain to terminal active node (ADR-003 superseded)
                find_terminal_active(
                    entry.id,
                    graph_opt.as_ref().unwrap(),
                    &all_entries,
                )
            };

            let terminal_id = match terminal_id {
                Some(id) => id,
                None => continue,  // no active terminal reachable; skip injection
            };

            // Skip if already in result set
            if existing_ids.contains(&terminal_id) {
                continue;
            }

            // Fetch and inject the terminal entry
            let terminal = match self.entry_store.get(terminal_id).await {
                Ok(t) => t,
                Err(_) => continue,  // dangling reference — skip (FR-2.7)
            };

            // Validate: terminal must be Active and non-superseded
            // (find_terminal_active guarantees this, but defensive check)
            if terminal.status != Status::Active || terminal.superseded_by.is_some() {
                continue;
            }

            // Compute similarity from stored embedding
            if let Some(emb) = self.vector_store.get_embedding(terminal_id).await {
                let sim = cosine_similarity(&embedding, &emb);
                results_with_scores.push((terminal, sim));
            }
            // If no embedding: skip injection (existing R-01 fallback pattern)
        }
    }
}
```

### Notes

- The `existing_ids` set is built before the loop and is NOT updated inside the loop. If the same terminal_id would be injected by two different superseded entries, it is only injected once (first wins; subsequent iterations skip via `existing_ids.contains`). This matches the old behavior where `unique_successor_ids` deduplicated before fetch.
- `find_terminal_active` may return the `terminal_id` of a node that was also a search result from HNSW — `existing_ids.contains` handles this.
- The old `if successor.status != Status::Active { continue; }` check is preserved as a defensive validation after fetch. `find_terminal_active` already guarantees Active + non-superseded, but the fetch result could theoretically differ if store state changed between graph build and fetch.
- In fallback mode (`use_fallback = true`): `entry.superseded_by` gives the direct single-hop id. The old code fetched this id directly via `entry_store.get` and checked `status==Active && superseded_by.is_none()`. The new code does the same: fallback produces `Some(entry.superseded_by)`, then the fetch + validation block applies.

---

## Change 5: Test Updates in search.rs

Several existing unit tests reference `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` directly. These tests use a `penalized_score` helper function that applies a penalty argument. The tests themselves test ranking logic — the penalty value they use must be updated to use topology-derived ordering assertions rather than constant-value assertions.

### Tests to Update (not delete)

**T-SP-01: deprecated_below_active_flexible**
- Currently uses `penalized_score(..., DEPRECATED_PENALTY)`.
- Update: use `ORPHAN_PENALTY` as a representative deprecated penalty (since a standalone deprecated entry with no successors is an orphan). Or restructure as: assert that for any penalty in (0.0, 1.0), active entry ranks above deprecated entry with same similarity when penalty is applied. Use `FALLBACK_PENALTY` as the test penalty value (it is always imported), or import `ORPHAN_PENALTY`.
- Simplest update: replace `DEPRECATED_PENALTY` with `unimatrix_engine::graph::ORPHAN_PENALTY` and `SUPERSEDED_PENALTY` with `unimatrix_engine::graph::CLEAN_REPLACEMENT_PENALTY`.

**T-SP-02: superseded_below_active_flexible**
- Replace `SUPERSEDED_PENALTY` with `unimatrix_engine::graph::CLEAN_REPLACEMENT_PENALTY`.

**T-SP-04: superseded_penalty_harsher**
- This test asserts `SUPERSEDED_PENALTY < DEPRECATED_PENALTY`. After crt-014, the equivalent ordering assertion uses graph constants: `CLEAN_REPLACEMENT_PENALTY < ORPHAN_PENALTY`. Update to: `assert!(CLEAN_REPLACEMENT_PENALTY < ORPHAN_PENALTY)` with an updated doc comment.

**T-SP-06: successor_ranks_above_superseded**
- Replace `SUPERSEDED_PENALTY` with `CLEAN_REPLACEMENT_PENALTY`.

**T-SP-07: penalty_independent_of_confidence_formula**
- Currently uses `DEPRECATED_PENALTY`. Replace with `FALLBACK_PENALTY` (always imported) or `ORPHAN_PENALTY`.

**T-SP-08: equal_similarity_penalty_determines_rank**
- Three penalty tiers test. Replace with:
  - `1.0` for active (unchanged)
  - `ORPHAN_PENALTY` for deprecated (was DEPRECATED_PENALTY)
  - `CLEAN_REPLACEMENT_PENALTY` for superseded (was SUPERSEDED_PENALTY)
  - Assert active_score > orphan_score > superseded_score (same structural assertion, different constant names)

**T-SP-05: deprecated_only_results_visible_flexible**
- Uses `penalized_score(..., DEPRECATED_PENALTY)`. Replace with `ORPHAN_PENALTY`.

### Tests in crt-018b block

The crt-018b tests (`test_utility_delta_inside_deprecated_penalty`, `test_utility_delta_inside_superseded_penalty`) use `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` to demonstrate formula placement. These must be updated:

**test_utility_delta_inside_deprecated_penalty**: Replace `DEPRECATED_PENALTY` with `FALLBACK_PENALTY` (semantically equivalent for illustrating penalty multiplication; both are in (0.0, 1.0)) and update the numeric comment to match.

**test_utility_delta_inside_superseded_penalty**: Replace `SUPERSEDED_PENALTY` with `CLEAN_REPLACEMENT_PENALTY` and update the numeric comment.

The assertions in these tests (`step7_score - correct_score < f64::EPSILON`) do not depend on the specific penalty value — they test formula structure (delta inside vs outside penalty), so any valid penalty constant works.

---

## What Is NOT Changed

- All crt-018b additions are preserved exactly:
  - `EffectivenessStateHandle` field and snapshot logic
  - `utility_delta` function and its tests
  - Lock ordering (read guard dropped before mutex)
  - Generation-cached snapshot (`EffectivenessSnapshot`)
  - `cached_snapshot: Arc<Mutex<EffectivenessSnapshot>>` field
- Steps 0–5 (rate check, validation, embedding, HNSW) — unchanged
- Step 7 (re-ranking sort with penalty_map) — unchanged
- Step 8 (co-access boost) — unchanged (penalty_map still used the same way)
- Steps 9–12 (truncate, floors, ScoredEntry build, audit) — unchanged
- `RetrievalMode::Strict` branch — unchanged (hard filter, no graph involvement)
- `explicit_status_filter` logic — unchanged
- `should_inject` guard (`explicit_status_filter != Some(Status::Deprecated)`) — unchanged

---

## Error Handling

| Scenario | Behavior |
|----------|----------|
| `spawn_blocking` for graph build panics | Propagate as `ServiceError::Internal` via `map_err` |
| `Store::query(QueryFilter::default())` fails | Propagate existing `ServiceError` — search fails as before |
| `GraphError::CycleDetected` | `tracing::error!`, `use_fallback = true`, search continues |
| `find_terminal_active` returns None | Skip injection for that entry (no successor added) |
| Terminal entry fetch fails | `continue` — skip injection (existing pattern) |
| Terminal entry has no embedding | Skip injection (existing pattern) |

---

## Key Test Scenarios (Integration)

Integration tests require a real store and embedding setup. Trace to RISK-TEST-STRATEGY.md:

**AC-12: Topology-derived penalties in penalty_map (R-01)**
- Store: entry B (Deprecated, no successors → orphan). Search returns B. Assert penalty_map[B.id] == ORPHAN_PENALTY.
- Store: entry A (Deprecated, supersedes=None, superseded_by=Some(B.id)), entry B (Active, superseded_by=None). Search returns A. Assert penalty_map[A.id] == CLEAN_REPLACEMENT_PENALTY.

**AC-13: Multi-hop injection injects C not B (R-06)**
- Store: A (Deprecated, superseded_by=Some(B.id)), B (Deprecated, superseded_by=Some(C.id)), C (Active, superseded_by=None).
- Search returns A. Assert injected entry ID == C.id (not B.id).

**AC-16: Cycle fallback (R-08)**
- Inject cyclic supersession data (via raw store writes bypassing validation, or by constructing entries directly with cyclic supersedes fields).
- Search succeeds. Log contains "supersession cycle detected". Deprecated entries get FALLBACK_PENALTY.
- Active entries do NOT appear in penalty_map after fallback (R-08).

**IR-01: QueryFilter::default() includes Deprecated**
- Build graph from store containing Deprecated entry. Assert graph has node for that entry.

**IR-02: Unified penalty condition**
- Entry with superseded_by.is_some() AND status=Active: assert receives graph penalty.

**IR-03: No penalty for Active non-superseded**
- Search returns Active non-superseded entries. Assert penalty_map has no entry for them.

**NFR-01 Benchmark**
- Build graph from 1000 EntryRecord values. Assert elapsed < 5ms.
