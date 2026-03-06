# crt-010: Status-Aware Retrieval — Architecture

## System Overview

crt-010 hardens the retrieval pipeline so that entry lifecycle status (Active, Deprecated, Quarantined) and supersession chains directly influence search ranking and filtering. Today, deprecated entries compete nearly equally with active entries in search results, and the `superseded_by` field is never consulted. This feature introduces two retrieval modes — **strict** (UDS/silent injection) and **flexible** (MCP/agent-directed) — that apply status-appropriate filtering, penalty scoring, supersession candidate injection, and co-access hygiene.

The changes are concentrated in `unimatrix-server` (SearchService, UDS listener, MCP tools, coherence/compaction) with a targeted cross-crate change in `unimatrix-engine` (co-access boost filtering). No schema changes. No new MCP tools or parameters. No changes to the confidence formula (crt-002).

### Position in the Retrieval Pipeline

```
Query → Embed → HNSW Search → [NEW: Status Filter/Penalty] → [NEW: Supersession Injection] → Re-rank → [CHANGED: Co-access Boost] → Truncate → Floors → Return
```

## Component Breakdown

### C1: RetrievalMode Enum + SearchService Status Logic

**Location:** `crates/unimatrix-server/src/services/search.rs`
**Responsibility:** Define retrieval mode semantics and apply status-aware filtering/penalty after HNSW fetch.

The `RetrievalMode` enum is the central design element (see ADR-001). It is added to `ServiceSearchParams` and controls all downstream status behavior:

- **`Strict`**: Drop all entries where `status != Active` OR `superseded_by.is_some()`. Used by UDS.
- **`Flexible`**: Keep deprecated entries but apply multiplicative penalty to their re-rank scores. Used by MCP. Quarantined still excluded (existing behavior).
- **Default**: `Flexible` (backward-compatible, addresses SR-09).

Status filtering happens in a new step between Step 6 (entry fetch) and Step 7 (re-rank) of the current pipeline. Penalty application happens during re-rank (Step 7).

### C2: Supersession Candidate Injection

**Location:** `crates/unimatrix-server/src/services/search.rs`
**Responsibility:** When a deprecated entry with `superseded_by` is found, fetch and inject the successor into the candidate pool.

Occurs after status filtering but before re-rank. Single-hop only (ADR-003). The successor is scored using cosine similarity computed from its stored embedding against the query embedding (ADR-002). Dangling references (successor doesn't exist or is itself deprecated) are silently skipped.

### C3: Co-Access Deprecated Exclusion

**Location:** `crates/unimatrix-engine/src/coaccess.rs`
**Responsibility:** Exclude deprecated entries from co-access boost computation.

The `compute_boost_internal` function gains a `deprecated_ids: &HashSet<u64>` parameter (ADR-004). Pairs where either anchor or partner is in the deprecated set produce zero boost. This is the only cross-crate interface change.

### C4: UDS Path Hardening

**Location:** `crates/unimatrix-server/src/uds/listener.rs`
**Responsibility:** Pass `RetrievalMode::Strict` to SearchService for all UDS search calls.

`handle_context_search` sets `retrieval_mode: RetrievalMode::Strict` on `ServiceSearchParams`. The `[deprecated]` indicator branch in CompactPayload formatting becomes dead code (deprecated entries no longer reach this point via strict mode).

### C5: MCP Filter Asymmetry Fix

**Location:** `crates/unimatrix-server/src/mcp/tools.rs`
**Responsibility:** Ensure MCP search uses `RetrievalMode::Flexible` regardless of whether topic/category/tags are provided.

Currently, when no topic/category/tags are set, `filters: None` results in unfiltered HNSW search. After this change, `retrieval_mode: RetrievalMode::Flexible` is always set. If the agent explicitly passes `status` as a filter parameter, that choice is honored.

### C6: Vector Index Pruning During Compaction — ALREADY SATISFIED

**Location:** `crates/unimatrix-server/src/services/status.rs`, `crates/unimatrix-server/src/background.rs`
**Status:** No new work required. Verification test only.

Since col-013, `maintain: true` on `context_status` is silently ignored — maintenance now runs on a background tick (`background.rs:234-257`). The background tick calls `StatusService::run_maintenance()` which receives `active_entries` — already filtered to `Status::Active` at `status.rs:175-181`. The compaction path at `status.rs:608-637` passes only these Active entries to `VectorIndex::compact()`.

This means deprecated and quarantined entries are already excluded from HNSW rebuilds. Between compaction runs, their embeddings linger in HNSW but are caught by the new status filtering in C1 (Strict drops them, Flexible penalizes them). Implementation scope for C6 is reduced to a verification integration test confirming the existing behavior.

### C7: Penalty Constants

**Location:** `crates/unimatrix-engine/src/confidence.rs`
**Responsibility:** Define the deprecated and superseded penalty multipliers as named constants.

Two new `pub const` values (ADR-005). These are retrieval-layer constants, not part of the confidence formula. They are consumed by SearchService during re-rank.

## Component Interactions

```
┌─────────────────────────────────────────────────────────────────┐
│                       SearchService (C1)                         │
│  ServiceSearchParams { retrieval_mode: RetrievalMode, ... }     │
│                                                                  │
│  Step 6: HNSW fetch + quarantine filter (existing)               │
│  Step 6a: [NEW] Status filter (Strict: drop non-Active;          │
│            Flexible: mark for penalty)                           │
│  Step 6b: [NEW] Supersession injection (C2)                     │
│            → entry_store.get(superseded_by)                      │
│            → vector_store cosine similarity for successor        │
│  Step 7:  Re-rank with penalty multipliers (C7 constants)        │
│  Step 8:  Co-access boost (calls C3 with deprecated_ids)         │
│  Steps 9-12: Truncate, floors, build results, audit              │
└──────────────┬──────────────────────────────┬────────────────────┘
               │                              │
    ┌──────────▼──────────┐      ┌────────────▼────────────┐
    │   UDS Listener (C4)  │      │   MCP Tools (C5)        │
    │ mode = Strict        │      │ mode = Flexible          │
    └──────────────────────┘      └──────────────────────────┘

    ┌──────────────────────────────────────────────────────────┐
    │  Co-Access (C3) — unimatrix-engine                        │
    │  compute_search_boost(..., deprecated_ids: &HashSet<u64>) │
    │  compute_briefing_boost(..., deprecated_ids: &HashSet<u64>)│
    └──────────────────────────────────────────────────────────┘

    ┌──────────────────────────────────────────────────────────┐
    │  Coherence/Compaction (C6)                                │
    │  Filter entries to Active|Proposed before compact()       │
    └──────────────────────────────────────────────────────────┘
```

## Technology Decisions

| Decision | ADR | Summary |
|----------|-----|---------|
| Retrieval mode signaling | ADR-001 | Enum on ServiceSearchParams, default Flexible |
| Successor similarity via cosine from stored embedding | ADR-002 | Fetch embedding from HNSW graph, compute cosine against query |
| Single-hop supersession only | ADR-003 | Follow one level of `superseded_by`, no transitive chains |
| Co-access filtering via deprecated ID set | ADR-004 | Pass `HashSet<u64>` to engine crate, no server type dependency |
| Penalty multipliers: 0.7x deprecated, 0.5x superseded | ADR-005 | Named constants in confidence.rs, applied at re-rank |

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `RetrievalMode` | `pub(crate) enum RetrievalMode { Strict, Flexible }` | `services/search.rs` (new) |
| `ServiceSearchParams.retrieval_mode` | `pub(crate) retrieval_mode: RetrievalMode` | `services/search.rs` (new field) |
| `DEPRECATED_PENALTY` | `pub const DEPRECATED_PENALTY: f64 = 0.7` | `unimatrix-engine/src/confidence.rs` (new) |
| `SUPERSEDED_PENALTY` | `pub const SUPERSEDED_PENALTY: f64 = 0.5` | `unimatrix-engine/src/confidence.rs` (new) |
| `compute_search_boost` | `pub fn compute_search_boost(anchor_ids: &[u64], result_ids: &[u64], store: &Store, staleness_cutoff: u64, deprecated_ids: &HashSet<u64>) -> HashMap<u64, f64>` | `unimatrix-engine/src/coaccess.rs` (changed) |
| `compute_briefing_boost` | `pub fn compute_briefing_boost(anchor_ids: &[u64], result_ids: &[u64], store: &Store, staleness_cutoff: u64, deprecated_ids: &HashSet<u64>) -> HashMap<u64, f64>` | `unimatrix-engine/src/coaccess.rs` (changed) |
| `compute_boost_internal` | `fn compute_boost_internal(..., deprecated_ids: &HashSet<u64>) -> HashMap<u64, f64>` | `unimatrix-engine/src/coaccess.rs` (changed, private) |
| `VectorIndex::search` (existing) | `pub fn search(&self, query: &[f32], top_k: usize, ef_search: usize) -> Result<Vec<SearchResult>>` | `unimatrix-vector/src/index.rs` (unchanged) |
| `EntryRecord.superseded_by` | `pub superseded_by: Option<u64>` | `unimatrix-store/src/schema.rs` (existing) |
| `EntryRecord.status` | `pub status: Status` | `unimatrix-store/src/schema.rs` (existing) |
| `AsyncEntryStore::get` | `pub async fn get(&self, id: u64) -> Result<EntryRecord, CoreError>` | `unimatrix-core/src/async_wrappers.rs` (existing) |

### Cosine Similarity for Successor Injection

Successor similarity requires computing cosine between the query embedding (already available as `Vec<f32>` from Step 3) and the successor's stored embedding. The HNSW graph stores embeddings internally but hnsw_rs does not expose a raw retrieval API. Two approaches:

**Chosen (ADR-002):** Add a `get_embedding(entry_id: u64) -> Option<Vec<f32>>` method to `VectorIndex` that reads from the internal HNSW data layer. The hnsw_rs `Hnsw` struct stores data points and supports retrieval via `get_point_indexation().get_point_data(data_id)`. This avoids re-embedding and uses the same embedding that was indexed.

New integration point:

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `VectorIndex::get_embedding` | `pub fn get_embedding(&self, entry_id: u64) -> Option<Vec<f32>>` | `unimatrix-vector/src/index.rs` (new) |
| `AsyncVectorStore::get_embedding` | `pub async fn get_embedding(&self, entry_id: u64) -> Option<Vec<f32>>` | `unimatrix-core/src/async_wrappers.rs` (new) |

### Cosine Similarity Helper

A pure function for cosine similarity between f32 vectors:

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `cosine_similarity` | `pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f64` | `unimatrix-engine/src/confidence.rs` (new) |

Returns f64 for scoring pipeline precision (crt-005 ADR-001). Assumes L2-normalized inputs (which they are — both query and stored embeddings are normalized).

## Error Boundaries

| Error Source | Handling |
|---|---|
| Successor entry not found (`entry_store.get` returns Err) | Skip injection, log warning. Do not fail search. |
| Successor is itself deprecated/quarantined/superseded | Skip injection. No recursion. |
| `get_embedding` returns None (entry has no vector mapping) | Skip injection for this successor. |
| Co-access boost with deprecated_ids fails | Existing fallback: returns empty HashMap, search continues without boost. |
| Compaction excludes all entries (all deprecated) | Compact receives empty vec, builds empty HNSW. Valid state — search returns no results until entries are restored/created. |

## Empty Result Handling (SR-04)

When strict mode produces zero results (possible given 123 deprecated vs 53 active entries):
- UDS returns `HookResponse::Entries { items: vec![], total_tokens: 0 }` — this is the existing empty-result path.
- No fallback to flexible mode. Wrong information is worse than no information (per SCOPE goal 1).
- The `total_tokens: 0` signals to the hook caller that no injection occurred.

## Resolved Questions

1. **Explicit status filter + supersession injection interaction**: When an agent explicitly requests `status: "deprecated"`, supersession injection is disabled. The agent has a reason for requesting deprecated content — if they find what they're looking for and look up that specific item, the `superseded_by` field is visible and they can follow the chain themselves. Injection would be unwanted noise in this context.

## Open Questions

1. **Briefing service co-access**: `BriefingService` also calls `compute_briefing_boost`. Its callers need to pass deprecated_ids too. Implementation should audit all call sites of `compute_briefing_boost` and thread the deprecated ID set through.
2. **hnsw_rs point data retrieval**: The `get_point_indexation().get_point_data()` API needs verification during implementation. If hnsw_rs doesn't support this, fall back to re-embedding via `embed_service` (more expensive but functionally equivalent).
