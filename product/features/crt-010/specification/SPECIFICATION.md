# SPECIFICATION: crt-010 Status-Aware Retrieval

## Objective

Unimatrix's search pipeline insufficiently weights lifecycle signals (deprecation, supersession), allowing stale knowledge to outrank current knowledge in retrieval results. This feature hardens the retrieval layer so that UDS (silent injection) delivers only Active, non-superseded entries, MCP search aggressively de-ranks deprecated entries, supersession chains inject successor candidates, and co-access boost excludes deprecated entries. Vector index compaction is extended to prune deprecated/quarantined embeddings.

## Functional Requirements

### FR-1: Dual Retrieval Modes

FR-1.1. SearchService exposes a retrieval mode parameter distinguishing **strict** (UDS) and **flexible** (MCP) filtering behavior.

FR-1.2. The mode parameter defaults to **flexible** to preserve backward compatibility for existing callers (SR-09).

FR-1.3. In **strict mode**, all entries where `status != Active` OR `superseded_by.is_some()` are excluded from results after HNSW fetch. Zero deprecated or superseded entries appear in output.

FR-1.4. In **flexible mode**, quarantined entries are excluded (existing behavior). Deprecated entries receive a multiplicative penalty of `DEPRECATED_PENALTY` applied to their final score. Superseded entries (deprecated + `superseded_by` set) receive a harsher multiplicative penalty of `SUPERSEDED_PENALTY`.

FR-1.5. When strict mode produces zero results, the response is an empty result set — not a fallback to flexible mode. Wrong information is worse than no information (SR-04).

### FR-2: Supersession Candidate Injection

FR-2.1. After HNSW fetch and before re-ranking, the pipeline scans results for entries with `superseded_by` set.

FR-2.2. Successor entries are batch-fetched by ID (single store read, not per-entry).

FR-2.3. A successor is injected into the candidate pool only if: (a) it is Active, (b) it is not already in the result set, (c) it does not itself have `superseded_by` set.

FR-2.4. Injected successors receive a similarity score computed via cosine similarity between the query embedding and the successor's stored embedding vector. No inherited or synthetic similarity scores.

FR-2.5. Injected successors flow through the normal re-rank pipeline (confidence weighting, co-access boost) with no special treatment.

FR-2.6. Only single-hop supersession is followed. If successor B is itself superseded by C, the chain is not traversed.

FR-2.7. If a `superseded_by` ID references a non-existent entry (dangling reference), the injection is silently skipped for that entry. No panic, no error propagation (SR-03 assumption).

### FR-3: Co-Access Deprecated Exclusion

FR-3.1. `compute_search_boost()` accepts status information (set of deprecated entry IDs or equivalent) and skips co-access pairs where either the anchor or the partner entry is deprecated.

FR-3.2. Co-access pair storage is unchanged — pairs involving deprecated entries continue to be recorded. Only the boost computation excludes them.

FR-3.3. The interface for passing status information into the engine crate avoids importing server-crate types. A `HashSet<u64>` of excluded IDs or a filter callback is acceptable (SR-07).

### FR-4: UDS Path Hardening

FR-4.1. `handle_context_search()` in the UDS listener passes strict mode to SearchService.

FR-4.2. BriefingService injection history filtering excludes deprecated entries before payload assembly.

FR-4.3. CompactPayload formatting removes the `[deprecated]` indicator branch — deprecated entries never reach this code path under strict mode.

### FR-5: Vector Index Pruning During Compaction — ALREADY SATISFIED

FR-5.1. ~~When `maintain: true` triggers compaction, the pipeline queries all entries with vector mappings and excludes entries where `status == Deprecated` or `status == Quarantined`.~~ **Already implemented.** Since col-013, maintenance runs on a background tick (`background.rs:234-257`). The tick calls `StatusService::run_maintenance()` with `active_entries` — filtered to `Status::Active` at `status.rs:175-181`. Compaction at `status.rs:608-637` already passes only Active entries to `VectorIndex::compact()`.

FR-5.2. Only Active and Proposed entries are passed to `VectorIndex::compact()`. **Already true.**

FR-5.3. ~~VECTOR_MAP entries for excluded IDs are removed via `Store::rewrite_vector_map()`.~~ **Already handled by compact()** which rebuilds the graph and rewrites mappings.

FR-5.4. A deprecated entry later restored to Active requires re-embedding. This is the expected cost of restore — no special handling (SR-03).

**Implementation scope:** Verification integration test only — no new code.

### FR-6: MCP Filter Asymmetry Fix

FR-6.1. `context_search` without explicit filters uses flexible mode (deprecated penalized, quarantined excluded, supersession injection active) — identical behavior whether or not topic/category/tags are provided.

FR-6.2. When the agent provides an explicit `status` parameter, that filter is honored as-is. No penalties are applied to entries matching the requested status. Supersession injection is disabled when the explicit status filter is set to `Deprecated` — the agent has a reason for requesting deprecated content and can follow `superseded_by` references themselves.

## Non-Functional Requirements

### NFR-1: Search Latency

NFR-1.1. Successor similarity computation (cosine from stored embedding) must not regress p95 search latency by more than 15% compared to current baseline, measured on a knowledge base with 200 entries and 50% deprecation ratio (SR-01).

NFR-1.2. Successor batch-fetch is a single store read operation, not N individual fetches.

### NFR-2: Configuration

NFR-2.1. `DEPRECATED_PENALTY` and `SUPERSEDED_PENALTY` are named constants in `confidence.rs` (or equivalent engine module), not magic numbers inlined at call sites (SR-02).

NFR-2.2. Default values: `DEPRECATED_PENALTY = 0.7`, `SUPERSEDED_PENALTY = 0.5`.

### NFR-3: Memory

NFR-3.1. No new persistent storage tables or schema changes. All mode signaling is in-memory, per-request only (SR-06).

### NFR-4: Compatibility

NFR-4.1. Existing `context_search` MCP tool parameters unchanged. No new tools or parameters (SCOPE non-goal).

NFR-4.2. SearchService API change is backward-compatible: default mode preserves current behavior (SR-09).

## Acceptance Criteria

| AC-ID | Criterion | Verification |
|-------|-----------|--------------|
| AC-01 | UDS search returns zero deprecated entries and zero superseded entries regardless of similarity scores | Integration test: insert Active + Deprecated + Superseded entries with Deprecated having higher similarity; UDS search returns only Active |
| AC-02 | MCP search returns deprecated entries ranked below Active entries at comparable similarity (within 5% similarity delta) | Integration test: Active entry sim=0.88, Deprecated entry sim=0.90; Active ranks higher after penalty |
| AC-03 | Superseded entries in MCP search receive harsher penalty than plain deprecated entries | Integration test: Deprecated (no successor) score > Superseded (with successor) score at equal base similarity |
| AC-04 | Supersession injection: when deprecated entry X has `superseded_by: Y` (Active), Y appears in results even if Y was not in HNSW top-k | Integration test: query matches X's embedding closely but not Y's; Y still appears in results via injection |
| AC-05 | Injected successor similarity is computed via cosine similarity against query embedding, not inherited from predecessor | Unit test: injected successor's score reflects its own embedding distance, not the deprecated entry's distance |
| AC-06 | Single-hop limit enforced: if successor Y is itself superseded by Z, Z is not injected | Integration test: chain A→B→C; search matching A injects B but not C |
| AC-07 | Dangling `superseded_by` reference (non-existent entry) does not cause error or panic | Unit test: entry with `superseded_by: 99999`; search completes successfully with no injection |
| AC-08 | Co-access boost excludes deprecated entries: deprecated anchor produces no boost; deprecated partner produces no boost | Unit test: co-access pair (Active, Deprecated) returns zero boost for both directions |
| AC-09 | Co-access pairs involving deprecated entries continue to be stored (write path unchanged) | Unit test: record co-access between Active and Deprecated entries; verify pair exists in storage |
| AC-10 | UDS strict mode returning zero results yields empty set, not fallback | Integration test: knowledge base with only deprecated entries for a query; UDS returns empty results |
| AC-11 | BriefingService injection history excludes deprecated entries | Integration test: briefing payload contains no deprecated entries even if they were previously injected |
| AC-12 | Vector compaction excludes deprecated and quarantined entries from rebuilt HNSW (already satisfied — verification of existing col-013 behavior) | Integration test: after background tick compaction, deprecated entry IDs absent from VECTOR_MAP; Active entries present |
| AC-13 | MCP `context_search` without filters applies flexible mode (deprecated penalized, not excluded) | Integration test: unfiltered MCP search returns deprecated entry with reduced score, not excluded |
| AC-14 | MCP `context_search` with explicit `status: Deprecated` returns deprecated entries at full score (no penalty) | Integration test: explicit status filter bypasses penalty multipliers |
| AC-14b | MCP `context_search` with explicit `status: Deprecated` disables supersession injection | Integration test: deprecated entry with `superseded_by` set; explicit status search does NOT inject Active successor |
| AC-15 | No new MCP tools, no new tool parameters, no schema changes | Code review: diff shows no new tool registrations, no new parameters, no migration |
| AC-16 | Search latency p95 regression under 15% on 200-entry knowledge base with 50% deprecation ratio | Benchmark test or manual measurement against baseline |

## Domain Models

### Key Entities

| Term | Definition |
|------|------------|
| **Entry** | A knowledge record in Unimatrix with `id: u64`, `status: Status`, `superseded_by: Option<u64>`, `confidence: f64`, and an associated embedding vector |
| **Status** | Enum: `Active`, `Deprecated`, `Quarantined`, `Proposed`. Lifecycle state of an entry |
| **Superseded Entry** | A deprecated entry with `superseded_by` set to the ID of its replacement. Carries the strongest staleness signal |
| **Successor** | The Active entry referenced by a superseded entry's `superseded_by` field |
| **Supersession Chain** | A directed link from a deprecated entry to its successor. Single-hop only in this feature |
| **Retrieval Mode** | Per-request enum: `Strict` (UDS — hard filter, zero tolerance) or `Flexible` (MCP — soft penalty, agent choice) |
| **DEPRECATED_PENALTY** | Multiplicative factor (0.7) applied to a deprecated entry's final score in flexible mode |
| **SUPERSEDED_PENALTY** | Multiplicative factor (0.5) applied to a superseded entry's final score in flexible mode |
| **Candidate Injection** | Adding a successor entry to the search candidate pool after HNSW fetch, scored on its own merits |
| **Co-Access Boost** | Score additive (max 0.03) from co-access pairs. Deprecated entries excluded from both anchor and partner roles |
| **Compaction** | HNSW index rebuild during background tick (col-013). Already excludes deprecated and quarantined embeddings — verification only for crt-010 |

### Relationships

```
Entry --[superseded_by]--> Entry (successor, 0..1)
Entry --[status]--> Status
Entry --[co-access]--> Entry (bidirectional pairs)
SearchService --[uses]--> RetrievalMode
SearchService --[calls]--> VectorIndex (HNSW fetch)
SearchService --[calls]--> Store (entry fetch, successor fetch)
SearchService --[calls]--> compute_search_boost (co-access)
CompactPayload --[assembled by]--> BriefingService
```

## User Workflows

### Workflow 1: Agent Search via MCP (Flexible Mode)

1. Agent calls `context_search` with topic (no explicit status filter)
2. SearchService runs HNSW fetch → raw candidates
3. Pipeline scans for `superseded_by` entries, batch-fetches successors, injects Active successors with computed cosine similarity
4. Deprecated entries receive `DEPRECATED_PENALTY`, superseded entries receive `SUPERSEDED_PENALTY`
5. Co-access boost computed, excluding deprecated entries from boost
6. Re-ranked results returned to agent — Active entries dominate; deprecated visible but de-ranked
7. Agent can explicitly request `status: Deprecated` to bypass penalties

### Workflow 2: Silent Injection via UDS (Strict Mode)

1. UDS listener receives context search request
2. SearchService runs in strict mode: HNSW fetch → exclude all non-Active and all superseded entries
3. Supersession injection still runs: successors added if Active and non-superseded
4. Co-access boost computed with deprecated exclusion
5. Only current, non-replaced entries returned — zero deprecated, zero superseded
6. If no entries survive filtering, empty result returned (no fallback)

### Workflow 3: Background Compaction (Existing — col-013)

1. Background tick fires periodically (`background.rs:234-257`)
2. `StatusService::compute_report()` collects `active_entries` filtered to `Status::Active` (`status.rs:175-181`)
3. `StatusService::run_maintenance()` passes only Active entries to `VectorIndex::compact()` (`status.rs:608-637`)
4. HNSW rebuilds with only Active embeddings
5. Subsequent HNSW searches have smaller, cleaner index
6. **No new work for crt-010** — verification test only

## Constraints

- **No schema changes** — `superseded_by`, `status`, and all required fields already exist (SCOPE non-goal, SR-06)
- **No new MCP tools or parameters** — behavior changes are internal to existing pipelines
- **No confidence formula changes** — the additive weighted composite (crt-002) is unchanged
- **Single-hop supersession only** — transitive chains not traversed (SR-05)
- **Engine crate decoupling** — co-access filtering interface must not import server-crate types (SR-07)
- **Backward-compatible SearchService API** — default mode preserves current behavior (SR-09)
- **No quarantine changes** — quarantine filtering already works correctly; this addresses deprecated/superseded gaps only

## Dependencies

| Dependency | Type | Notes |
|------------|------|-------|
| `unimatrix-server` (SearchService) | Internal crate | Primary change target: search pipeline, mode parameter |
| `unimatrix-server` (UDS listener) | Internal crate | Strict mode integration, briefing filtering |
| `unimatrix-server` (MCP tools) | Internal crate | Asymmetry fix, status filter passthrough |
| `unimatrix-server` (coherence/compaction) | Internal crate | Pruning logic for deprecated/quarantined |
| `unimatrix-engine` (coaccess) | Internal crate | Deprecated exclusion in boost computation |
| `unimatrix-engine` (confidence) | Internal crate | Penalty constants definition |
| `unimatrix-vector` (VectorIndex) | Internal crate | No changes — compact already accepts filtered embeddings |
| `unimatrix-store` | Internal crate | No changes — all required fields exist |
| `unimatrix-core` | Internal crate | No changes |
| ass-015 research (GH #118) | Research | Validated findings on status signal gaps |
| crt-002 (Confidence Evolution) | Completed feature | Confidence scoring pipeline unchanged |
| crt-004 (Co-Access Boosting) | Completed feature | Co-access storage and boost mechanism |
| crt-005 (Coherence Gate) | Completed feature | Compaction infrastructure (`maintain: true`) |

## NOT In Scope

- **New MCP tools or parameters** — no additions to tool surface area
- **Confidence formula changes** — scoring weights/factors from crt-002 are untouched
- **Schema migrations** — no new tables, no field changes, no version bump
- **Quarantine behavior changes** — already working correctly
- **Multi-hop supersession traversal** — single-hop only; transitive chains deferred
- **Restore-from-deprecated workflows** — natural re-entry; re-embedding is acceptable cost
- **Embedding model changes** — vector representations unchanged
- **Configurable penalty values at runtime** — constants are compile-time; runtime configuration deferred
- **Fallback from strict to flexible mode** — UDS returns empty rather than relaxing filters
