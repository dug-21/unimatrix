# crt-042: PPR Expander — Pseudocode Overview

## Components Involved

| Component | File | Why Modified |
|-----------|------|-------------|
| `graph_expand` | `crates/unimatrix-engine/src/graph_expand.rs` (new) | Pure BFS traversal function; returns reachable entry IDs from seed set |
| Phase 0 in search.rs | `crates/unimatrix-server/src/services/search.rs` (modify) | Async orchestration: calls graph_expand, fetches/scores expanded entries, merges into pool |
| `InferenceConfig` | `crates/unimatrix-server/src/infra/config.rs` (modify) | Three new operator-facing fields; four coordinated addition sites |
| Eval profile | `product/research/ass-037/harness/profiles/ppr-expander-enabled.toml` (new) | Profile A for A/B eval gate measurement |

`graph.rs` is also modified to declare the `#[path]` submodule and re-export — but contains no
new algorithmic logic.

---

## Data Flow

```
HNSW k=20 results
    → results_with_scores: Vec<(EntryRecord, f64)>  [after Steps 6a, 6b]
    → [if ppr_expander_enabled AND !use_fallback]
        Phase 0:
            seed_ids: Vec<u64>  ← results_with_scores.iter().map(|(e,_)| e.id)
            expanded_ids: HashSet<u64>  ← graph_expand(&typed_graph, &seed_ids, depth, max)
            in_pool: HashSet<u64>  ← seed_ids (deduplication guard)
            for each expanded_id NOT in in_pool:
                entry: EntryRecord  ← entry_store.get(expanded_id) [async]
                check SecurityGateway::is_quarantined(&entry.status) → skip if quarantined
                emb: Vec<f32>  ← vector_store.get_embedding(expanded_id) [async, O(N)]
                cosine_sim: f64  ← cosine_similarity(&query_embedding, &emb)
                results_with_scores.push((entry, cosine_sim))
    → Phase 1: seed_scores built from ALL results_with_scores (seeds + expanded)
    → Phase 2–5: existing PPR pipeline (unchanged)
```

---

## Shared Types

All types are from existing codebase. No new types are introduced.

| Type | Crate | Used By |
|------|-------|---------|
| `TypedRelationGraph` | `unimatrix-engine` | `graph_expand` (read-only), Phase 0 caller |
| `RelationType` | `unimatrix-engine` | `graph_expand` edge-type filter |
| `NodeIndex` | `petgraph` (via `unimatrix-engine`) | `graph_expand` internal BFS state |
| `HashSet<u64>` | `std` | Return type of `graph_expand` |
| `EntryRecord` | `unimatrix-core` | Phase 0 result accumulation |
| `InferenceConfig` | `unimatrix-server` | Config source for three new fields |
| `SecurityGateway` | `unimatrix-server` | Quarantine check in Phase 0 |

---

## Combined Expansion Ceiling

```
After HNSW (Step 6):          k=20 entries  (HNSW_K = 20)
After Phase 0 (crt-042):    + max_expansion_candidates (default 200)  → up to 220 entries
After Phase 5 (existing):   + ppr_max_expand (default 50)             → up to 270 entries
                                                                         ^^^^^^^^^^^^^^^^^^^^
                                                                         DOCUMENTED MAXIMUM
```

- Phase 0 injects entries via graph reachability (BFS, positive edges).
- Phase 5 injects entries via PPR mass diffusion exceeding ppr_inclusion_threshold.
- The two mechanisms produce disjoint additions when Phase 0 runs first: Phase 5's
  `existing_ids` HashSet is built from `results_with_scores` AFTER Phase 0 has appended,
  so Phase 5 cannot reinject Phase 0 entries.
- The ceiling is enforced by the independent caps of each phase, not a combined ceiling check.

---

## Lock Ordering

The typed graph is cloned under a short read lock acquired between Steps 6b and 6d (existing
behavior, ~line 673 in search.rs). Phase 0 uses the pre-cloned `typed_graph` value. No lock is
held during BFS traversal or the async fetch loop. This is the same pre-clone pattern used by
all existing PPR phases (pattern entry #3753).

`graph_expand` itself holds no locks, acquires no locks, and issues no async calls.

---

## Sequencing Constraints (Build Order)

1. `graph_expand.rs` (pure function, unimatrix-engine) — no new dependencies; can be built first.
2. `InferenceConfig` additions (config.rs) — no dependency on graph_expand.
3. Phase 0 in search.rs — depends on both: calls `graph_expand`, reads new `InferenceConfig` fields.
4. `SearchService::new()` parameter additions — depends on InferenceConfig additions.
5. Eval profile — no code dependency; can be created anytime after InferenceConfig is tested.

---

## SR-03 Blocking Gate (not pseudocode — delivery prerequisite)

Before Phase 0 code is written, the delivery agent must confirm S1/S2 (Informs) edge
directionality. If single-direction only (source_id < target_id), a back-fill issue must be
filed before crt-042 ships. This is a process gate, not a code design question. The pseudocode
below is written assuming the gate passes or is in progress.
