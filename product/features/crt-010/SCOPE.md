# crt-010: Status-Aware Retrieval

## Problem Statement

Unimatrix's knowledge evolution chain produces three lifecycle signals — deprecation, supersession, and quarantine — that represent deliberate agent decisions about content quality. Research (ass-015, GH #118) reveals these signals are insufficiently weighted in the search pipeline, allowing stale and replaced knowledge to compete with current knowledge in retrieval results.

The root cause is architectural: the HNSW vector index contains embeddings for all entries regardless of status, and the search pipeline applies only quarantine filtering post-fetch. Deprecated entries receive a negligible ~0.008 final score penalty (base_score delta diluted through two layers of weighting). The `superseded_by` field — the strongest signal in the chain, an explicit "this replaces that" — is never checked during search. Co-access boost amplifies deprecated entries alongside active ones.

In a knowledge base with 123 deprecated entries vs 53 active, this means search results are likely polluted with outdated knowledge. For UDS (silent injection), agents receive wrong information without any ability to evaluate it. For MCP (agent-directed search), deprecated entries outrank active entries when they have marginally higher vector similarity.

## Goals

1. **UDS hard filter**: Silent injection path delivers only Active, non-superseded entries. Wrong information is worse than no information.
2. **MCP soft ranking**: Agent-directed search returns deprecated entries with aggressive de-ranking. Active entries always outrank deprecated entries at comparable similarity. Agents can still explicitly request deprecated entries via status filter.
3. **Supersession chain as retrieval signal**: When search finds a deprecated entry with `superseded_by` set and the successor is Active, inject the successor into the candidate pool. The successor competes on its own merits (own similarity, own confidence) — no forced ranking.
4. **Co-access hygiene**: Deprecated entries excluded from co-access boost calculation. They should not amplify other entries or receive amplification.
5. **Vector index pruning**: Compaction excludes deprecated and quarantined entries from the rebuilt HNSW graph, reducing index size and eliminating stale candidates from ANN search.

## Non-Goals

- **New MCP tools or parameters** — no new tools; behavior changes are internal to existing `context_search`, `context_lookup`, `context_briefing` pipelines
- **Confidence formula changes** — the additive weighted composite (crt-002) is unchanged; status impact is addressed at the retrieval layer, not the scoring layer
- **Schema changes** — `superseded_by`, status, and all required fields already exist
- **Quarantine changes** — quarantine filtering already works correctly across all paths; this feature addresses deprecated and superseded gaps only
- **Restore-from-deprecated workflows** — if a deprecated entry is restored to Active, it re-enters the pipeline naturally; no special handling needed
- **Embedding model changes** — vector representations unchanged; this is retrieval-layer only
- **Multi-hop supersession chains** — follow one level of `superseded_by`; transitive chains (A superseded by B superseded by C) are not traversed

## Background Research

### Validated Findings (ass-015, GH #118)

| Signal | Expected Impact | Actual Impact | Root Cause |
|--------|----------------|---------------|------------|
| Quarantined | Fully hidden | Excluded post-fetch | Working correctly |
| Deprecated | Strongly de-emphasized | ~0.008 score penalty | base_score delta diluted through W_BASE (0.18) × confidence weight (0.15) |
| Superseded (`superseded_by` set) | De-emphasized or excluded | Zero effect | Field never checked in search pipeline |
| Co-access on deprecated | No boost | Full boost | `compute_search_boost()` doesn't filter by status |

### Quantitative Example

Two entries about "error handling conventions":
- Entry A: Active, similarity=0.88, confidence=0.65 → score=0.845
- Entry B: Deprecated, similarity=0.90, confidence=0.59 → score=0.854

The deprecated entry wins. A 2% similarity advantage overwhelms the entire status penalty.

### Two Retrieval Paths

**MCP (`context_search` via `tools.rs:259-298`):**
- With topic/category/tags: forces `Status::Active` via QueryFilter
- Without filters (common case): `filters: None` → unfiltered HNSW → returns Active + Deprecated
- Internal asymmetry within the same tool

**UDS (`handle_context_search` via `listener.rs:737-791`):**
- Always `filters: None` → unfiltered HNSW
- Has `similarity_floor=0.5` and `confidence_floor=0.3` but these barely help
- Used for silent injection — agents cannot evaluate what they receive

### Supersession as Candidate Injection

When search matches a deprecated entry X that has `superseded_by: Y`:
- Fetch Y. If Active and not already in results, add to candidate pool.
- Y gets scored normally — own similarity, own confidence.
- X gets deprecated penalty (MCP) or is dropped (UDS).

This uses the knowledge graph edges as a recall booster. The successor might have different wording or embedding and wouldn't appear in HNSW top-k, but the chain tells us it's semantically related. No forced weights — the successor earns its rank through normal scoring.

## Scope

### Component 1: SearchService Status Filtering

**File:** `crates/unimatrix-server/src/services/search.rs`

Add a `caller_context` parameter (or equivalent) to `SearchService::search()` that controls status filtering behavior:

- **Strict mode (UDS):** After HNSW fetch, exclude entries where `status != Active` OR `superseded_by.is_some()`. Only current, non-replaced entries survive.
- **Flexible mode (MCP):** After HNSW fetch, exclude quarantined (existing). Apply multiplicative penalty to deprecated entries' final scores (e.g., `0.7x`). Apply harsher penalty to superseded entries (e.g., `0.5x`). Penalty values are constants in `confidence.rs`.

### Component 2: Supersession Candidate Injection

**File:** `crates/unimatrix-server/src/services/search.rs`

After Step 6 (post-fetch, pre-rerank):
1. Scan results for entries with `superseded_by.is_some()`
2. Batch-fetch successor entries by ID
3. For each successor: if Active, not already in results, and not itself superseded → add to candidate pool with its own similarity score (re-embed query against successor, or use successor's stored embedding for cosine similarity)
4. Injected successors flow through normal re-rank pipeline (Step 7+)

Design decision needed: computing similarity for injected successors requires either (a) fetching their embedding from the vector index and computing cosine similarity against the query embedding, or (b) using a fixed "inherited similarity" from the deprecated entry. Option (a) is more accurate.

### Component 3: Co-Access Deprecated Exclusion

**File:** `crates/unimatrix-engine/src/coaccess.rs`

In `compute_search_boost()`:
- Accept entry status information (either pass a status map or accept a set of deprecated IDs)
- Skip co-access pairs where either the anchor or the partner is deprecated
- No changes to co-access storage — pairs are still recorded; just not used for boosting

### Component 4: UDS Path Hardening

**File:** `crates/unimatrix-server/src/uds/listener.rs`

- `handle_context_search()`: Pass strict-mode flag to SearchService
- `handle_compact_payload()` / BriefingService injection history path: Filter out deprecated entries from injection history before assembly
- CompactPayload formatting: Remove the `[deprecated]` indicator branch — deprecated entries should no longer reach this point

### Component 5: Vector Index Pruning During Compaction

**Files:** `crates/unimatrix-server/src/infra/coherence.rs`, `crates/unimatrix-server/src/services/status.rs`

During `maintain: true` compaction:
- Query all entries with vector mappings
- Exclude entries where `status == Deprecated` or `status == Quarantined`
- Pass only Active/Proposed entries to `VectorIndex::compact()`
- Clean up VECTOR_MAP entries for excluded IDs via `Store::rewrite_vector_map()`

Note: If a deprecated entry is later restored to Active, it would need re-embedding. This is acceptable — restoration is rare and re-embedding is a natural part of status change.

### Component 6: MCP Filter Asymmetry Fix

**File:** `crates/unimatrix-server/src/mcp/tools.rs`

Fix the internal asymmetry in `context_search`:
- Currently: with topic/category/tags → Active filter; without → no filter
- After: without explicit filters, MCP search uses flexible-mode (deprecated penalized, quarantined excluded, supersession injection active)
- With explicit `status` parameter: honor the agent's choice

## Design Decisions Needed

1. **Deprecated penalty multiplier value**: Proposed `0.7x` for deprecated, `0.5x` for superseded. Need to validate these don't over-suppress in edge cases (e.g., only deprecated entries match a query).
2. **Successor similarity computation**: Cosine similarity from stored embedding (accurate, requires vector fetch) vs inherited similarity from predecessor (simpler, less accurate).
3. **SearchService API**: How to signal strict vs flexible mode — enum parameter, builder pattern, or separate method.

## Affected Crates

| Crate | Changes |
|-------|---------|
| `unimatrix-server` | SearchService, UDS listener, MCP tools, BriefingService, coherence/compaction |
| `unimatrix-engine` | Co-access boost filtering, penalty constants |
| `unimatrix-vector` | No changes (compact already accepts filtered embeddings) |
| `unimatrix-store` | No changes (all required fields exist) |
| `unimatrix-core` | No changes |

## References

- GH #118: [ass-015] Research findings
- `crates/unimatrix-engine/src/confidence.rs`: base_score, rerank_score, PROVENANCE_BOOST
- `crates/unimatrix-server/src/services/search.rs`: SearchService pipeline
- `crates/unimatrix-server/src/uds/listener.rs`: UDS search + compact payload
- `crates/unimatrix-engine/src/coaccess.rs`: compute_search_boost
- `crates/unimatrix-server/src/services/briefing.rs`: BriefingService injection history

## Tracking

https://github.com/anthropics/unimatrix/issues/119
