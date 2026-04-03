# Agent Report: crt-042-researcher

## Summary

Researched the PPR expander problem space. SCOPE.md written to
`product/features/crt-042/SCOPE.md`.

## Key Findings

### The root cause is architectural, not parametric

The current PPR implementation in `search.rs` Step 6d builds its personalization vector
exclusively from `results_with_scores` — the HNSW k=20 result set. `personalized_pagerank`
only scores nodes in `typed_graph.node_index` that receive non-zero personalization mass.
Cross-category ground truth entries that share graph edges with HNSW seeds receive near-zero
mass after alpha=0.85 diffusion over multiple hops, and their PPR score never exceeds
`ppr_inclusion_threshold=0.05`. This is the documented zero-delta mechanism.

### Exact code locations

| Concern | File | Location |
|---------|------|----------|
| HNSW search (Step 5) | `crates/unimatrix-server/src/services/search.rs` | Lines ~622–653 |
| Candidate fetch + quarantine filter | `search.rs` | Lines ~655–667 |
| TypedGraph read lock + clone | `search.rs` | Lines ~669–684 |
| PPR Step 6d (full block) | `search.rs` | Lines ~852–972 |
| personalization vector build (Phase 1) | `search.rs` | Lines ~857–881 |
| PPR call (Phase 2) | `search.rs` | Lines ~895–902 |
| PPR-only injection (Phase 5) | `search.rs` | Lines ~943–971 |
| `personalized_pagerank` function | `crates/unimatrix-engine/src/graph_ppr.rs` | Lines 40–131 |
| `TypedRelationGraph` struct + `edges_of_type` | `crates/unimatrix-engine/src/graph.rs` | Lines ~178–214 |
| `TypedGraphState::rebuild` (tick reads GRAPH_EDGES) | `crates/unimatrix-server/src/services/typed_graph.rs` | Lines ~91–140 |
| `write_graph_edge` | `crates/unimatrix-server/src/services/nli_detection.rs` | Lines ~78–118 |
| GRAPH_EDGES schema | `crates/unimatrix-store/src/migration.rs` | Lines ~336–350 |
| `query_graph_edges` store method | `crates/unimatrix-store/src/read.rs` | Lines ~1323–1366 |
| `InferenceConfig` PPR fields | `crates/unimatrix-server/src/infra/config.rs` | Lines ~471–530 |
| Eval harness | `product/research/ass-039/harness/run_eval.py` | Lines 1–230 |
| Eval profiles dir | `product/research/ass-037/harness/profiles/` | - |

### GRAPH_EDGES schema

```sql
CREATE TABLE graph_edges (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    source_id     INTEGER NOT NULL,
    target_id     INTEGER NOT NULL,
    relation_type TEXT NOT NULL,    -- 'CoAccess','Supports','Informs','Supersedes','Contradicts','Prerequisite'
    weight        REAL NOT NULL DEFAULT 1.0,
    created_at    INTEGER NOT NULL,
    created_by    TEXT NOT NULL DEFAULT '',
    source        TEXT NOT NULL DEFAULT '',
    bootstrap_only INTEGER NOT NULL DEFAULT 0,
    metadata      TEXT DEFAULT NULL,
    UNIQUE(source_id, target_id, relation_type)
)
```

No `signal_origin` column exists in the current schema. The crt-041 roadmap references
`signal_origin` as a label; it is stored in the `source` column (e.g., 'S1', 'S2', 'S8').

### Feature flag pattern

No dedicated feature flag infrastructure. Existing on/off gates use `bool` fields in
`InferenceConfig` (e.g., `nli_enabled`). The expander flag follows this pattern:
`ppr_expander_enabled: bool`, default `false`.

### `graph_expand` module placement

Established pattern (crt-030): new graph traversal functions live in dedicated submodule
files (`graph_ppr.rs`, `graph_suppression.rs`), declared via `#[path]` in `graph.rs`, and
re-exported. A new `graph_expand.rs` follows this pattern. Tests in `graph_expand_tests.rs`
if the file would exceed 500 lines.

### `get_embedding` latency risk

`vector_store.get_embedding(id)` is O(N) — linear scan of the HNSW in-memory index (confirmed
ADR from crt-029, Unimatrix entry #3658). At 200 expanded entries × O(N), this is the dominant
latency cost of the expander. Feature flag + measurement required before default enablement.

### Eval gate

Baseline: MRR=0.2856, P@5=0.1115 (live DB, 2026-04-02, conf-boost-c). The roadmap specifies
"first gate where P@5 should respond" — P@5 > 0.1115 is the primary evidence the expander works.

## Scope Boundaries Proposed

### In scope
- `graph_expand` pure function in `unimatrix-engine/src/graph_expand.rs`
- Integration in `search.rs` Step 6d as Phase 0 (before personalization vector build)
- 3 new `InferenceConfig` fields: `ppr_expander_enabled`, `expansion_depth`, `max_expansion_candidates`
- New eval profile `ppr-expander-enabled.toml`

### Out of scope
- PPR algorithm changes (graph_ppr.rs unchanged)
- Schema migration (no new GRAPH_EDGES columns)
- Background tick changes
- Enabling expander by default (flag ships as false)

## Open Questions for Human

Five open questions in SCOPE.md. The two most load-bearing:

1. **Both directions vs. Outgoing-only for graph_expand?** Outgoing-only is simpler and
   consistent with PPR semantics; both-directions surfaces more cross-category entries but
   doubles traversal fan-out.

2. **Initial similarity for expanded entries**: true cosine similarity (computed from stored
   embedding) vs. a small constant floor (e.g., `0.01`)? True cosine means semantic distance
   is honored; a constant floor means PPR mass alone drives ranking of expanded entries.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- returned 18 entries; entries #3739, #3732,
  #3737 (crt-030 ADRs on PPR memory, function signature, dual-role blend weight) directly
  applicable. Entry #3658 (O(N) get_embedding risk) identified as relevant constraint.
- Stored: see /uni-store-pattern call below (graph_expand module split pattern).
