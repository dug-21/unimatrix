# Architecture: crt-003 Contradiction Detection

## Overview

crt-003 adds three capabilities to Unimatrix: (1) entry quarantine as a new lifecycle status, (2) contradiction detection between semantically similar entries, and (3) embedding consistency checks for relevance hijacking defense. The architecture spans 4 crates and introduces 1 new module in the server crate.

## Component Breakdown

### C1: Status Extension (`unimatrix-store`)

**Scope**: Add `Quarantined = 3` variant to `Status` enum.

**Changes**:
- `schema.rs`: Add `Quarantined = 3` to `#[repr(u8)]` enum
- `schema.rs`: Add `3 => Ok(Status::Quarantined)` to `TryFrom<u8>`
- `schema.rs`: Add `Status::Quarantined => write!(f, "Quarantined")` to `Display`
- `schema.rs`: Add `Status::Quarantined => "total_quarantined"` to `status_counter_key()`
- `test_helpers.rs`: Update test record builders if they have exhaustive matches
- Existing `write.rs` `update_status()`: Already generic -- handles any Status value by removing old STATUS_INDEX entry and inserting new one. No code change needed.

**Exhaustive match sites to update** (non-test, production code):

| File | Function | Current Arms | New Arm |
|------|----------|--------------|---------|
| `unimatrix-store/schema.rs` | `TryFrom<u8>` | 0,1,2 | 3 => Quarantined |
| `unimatrix-store/schema.rs` | `Display::fmt` | Active,Deprecated,Proposed | Quarantined |
| `unimatrix-store/schema.rs` | `status_counter_key` | Active,Deprecated,Proposed | Quarantined |
| `unimatrix-server/confidence.rs` | `base_score` | Active,Proposed,Deprecated | Quarantined |
| `unimatrix-server/response.rs` | `status_to_str` | Active,Deprecated,Proposed | Quarantined |
| `unimatrix-server/validation.rs` | `parse_status` | active,deprecated,proposed | quarantined |

**Decision**: Quarantined entries get `base_score = 0.1` -- lower than Deprecated (0.2) because quarantined entries are actively suspected of being harmful, not merely outdated. See ADR-001.

### C2: Retrieval Filtering (`unimatrix-server`)

**Scope**: Exclude quarantined entries from search, lookup, and briefing results.

**Changes to `tools.rs`**:

1. **`context_search`**: After HNSW search returns candidates, the result processing already filters by metadata (topic, category, tags). Add a status filter: exclude entries where `status == Quarantined`. This is a post-search filter (HNSW does not know about status).

2. **`context_lookup`**: `QueryFilter` defaults to `Status::Active` when no status is specified. For backward compatibility, when no status is provided, the default `Some(Status::Active)` already excludes Quarantined. When status is explicitly provided as "quarantined", allow it. No behavior change for existing callers.

3. **`context_briefing`**: Internally calls lookup (which defaults to Active) and search (which will now filter Quarantined). No direct changes to briefing logic.

4. **`context_get`**: No change. Direct ID access returns any entry regardless of status. This is critical for forensic investigation of quarantined entries.

5. **`context_correct`**: Already checks `status == Deprecated` and rejects. Add check for `Quarantined` -- cannot correct a quarantined entry (it must be restored first).

**Implementation note**: The search filter happens in the server layer (after getting results from the vector store), not in the vector store itself. Quarantined entries remain in the HNSW index. Removing them from HNSW would require a rebuild. The post-search filter is cheap (one status comparison per result).

### C3: Quarantine Tool (`unimatrix-server`)

**Scope**: New `context_quarantine` MCP tool for Admin-level status transitions.

**Parameters**:
```
QuarantineParams {
    id: i64,                  // required: entry ID
    reason: Option<String>,   // optional: reason for quarantine/restore
    action: Option<String>,   // optional: "quarantine" (default) or "restore"
    agent_id: Option<String>, // standard identity param
    format: Option<String>,   // standard format param
}
```

**Execution flow**:
1. Identity -> require_capability(Admin)
2. Validate params (id > 0, action in {quarantine, restore})
3. Read entry (verify exists)
4. Action dispatch:
   - **quarantine**: If already Quarantined, return success (idempotent). Otherwise, use `quarantine_with_audit()` in `server.rs` -- atomic status transition + audit in single write transaction.
   - **restore**: If not Quarantined, return error. Otherwise, use `restore_with_audit()` -- transition to Active + audit.
5. Recompute confidence (quarantined gets base_score 0.1; restored gets base_score 0.5)
6. Format response

**New server methods**:
- `UnimatrixServer::quarantine_with_audit(id, reason)` -- single write txn: update status to Quarantined, update STATUS_INDEX (remove old, insert new), update COUNTERS (decrement old status counter, increment total_quarantined), write audit event.
- `UnimatrixServer::restore_with_audit(id, reason)` -- same pattern, status back to Active.

These follow the same combined-transaction pattern as `deprecate_with_audit()` in vnc-003.

### C4: Contradiction Detection (`unimatrix-server`)

**Scope**: New module `contradiction.rs` with the scanning logic.

**Key types**:
```rust
/// A detected contradiction between two entries.
pub struct ContradictionPair {
    pub entry_id_a: u64,
    pub entry_id_b: u64,
    pub title_a: String,
    pub title_b: String,
    pub similarity: f32,
    pub conflict_score: f32,
    pub explanation: String,
}

/// An entry with an inconsistent embedding.
pub struct EmbeddingInconsistency {
    pub entry_id: u64,
    pub title: String,
    pub expected_similarity: f32, // similarity between stored and re-computed
}

/// Configuration for the contradiction scanner.
pub struct ContradictionConfig {
    pub similarity_threshold: f32,    // default: 0.85
    pub conflict_sensitivity: f32,    // default: 0.5
    pub neighbors_per_entry: usize,   // default: 10
    pub embedding_consistency_threshold: f32, // default: 0.99
}
```

**Contradiction scan algorithm**:

```
scan_contradictions(store, vector_index, config) -> Vec<ContradictionPair>:
  1. Read all active entries from ENTRIES table
  2. seen_pairs = HashSet<(min(a,b), max(a,b))>  // dedup
  3. For each active entry E:
     a. Get E's embedding: search HNSW with entry's own data, or re-embed
        DECISION: Re-embed from text (see ADR-002 for rationale)
     b. Search HNSW for top-K neighbors of E (K=neighbors_per_entry)
     c. For each neighbor N with similarity > similarity_threshold:
        - Skip if N == E (self-match)
        - Skip if N is not Active
        - Skip if (min(E.id, N.id), max(E.id, N.id)) already in seen_pairs
        - Run conflict_heuristic(E.content, N.content, config.conflict_sensitivity)
        - If conflict_score > 0: add to results, mark pair as seen
  4. Sort results by conflict_score descending
  5. Return
```

**Why re-embed instead of retrieving stored embeddings**: hnsw_rs `get_point_data` takes a `PointId (layer, position)`, not a data_id. Our `VectorIndex` maps entry_id -> data_id, but not data_id -> PointId. Adding that mapping would require tracking PointIds assigned by hnsw_rs during insertion, which is not currently captured. Re-embedding from text is simpler, uses the existing `EmbedService`, and as a side effect also validates embedding consistency (the re-computed embedding should match what was originally stored). See ADR-002.

**Conflict heuristic**:

```
conflict_heuristic(content_a, content_b, sensitivity) -> (score, explanation):
  signals = []

  // Signal 1: Negation opposition (weight: 0.6)
  // Look for patterns where one entry affirms and the other negates
  // e.g., "use X" vs "avoid X", "always X" vs "never X"
  neg_score = check_negation_opposition(content_a, content_b)
  if neg_score > 0: signals.push(("negation", neg_score * 0.6))

  // Signal 2: Incompatible directives (weight: 0.3)
  // Both entries prescribe specific choices for the same decision
  // e.g., "use library A for X" vs "use library B for X"
  dir_score = check_incompatible_directives(content_a, content_b)
  if dir_score > 0: signals.push(("directive", dir_score * 0.3))

  // Signal 3: Opposing sentiment on same subject (weight: 0.1)
  // One positive, one negative framing
  sent_score = check_opposing_sentiment(content_a, content_b)
  if sent_score > 0: signals.push(("sentiment", sent_score * 0.1))

  total_score = sum of weighted signal scores, clamped to [0.0, 1.0]

  // Apply sensitivity threshold
  if total_score < (1.0 - sensitivity): return (0.0, "")

  explanation = join signal descriptions
  return (total_score, explanation)
```

**Negation pattern detection**: Extract directive phrases from each entry (sentences containing "use", "always", "never", "avoid", "prefer", "do not", "should", "must"). Normalize the phrases. Check if one entry's directive negates another's. This is regex-based with tokenization. See ADR-003 for pattern details.

**Embedding consistency check**:

```
check_embedding_consistency(store, vector_index, embed_service, config) -> Vec<EmbeddingInconsistency>:
  1. Read all active entries from ENTRIES table
  2. For each entry E:
     a. Re-embed E's title+content using embed_service
     b. Search HNSW with the re-computed embedding, K=1
     c. If top result is E itself with similarity >= threshold: consistent
     d. If top result is NOT E, or similarity < threshold: flag as inconsistent
  3. Return flagged entries
```

This is elegant: if an entry's re-computed embedding matches what is stored in HNSW, the entry should be its own nearest neighbor with similarity ~1.0. If it is not, either the stored embedding was tampered with, the content was modified post-embedding, or the embedding model changed.

### C5: StatusReport Extension (`unimatrix-server`)

**Scope**: Extend `StatusReport` and `context_status` tool to include quarantine counts, contradictions, and embedding consistency results.

**StatusReport changes**:
```rust
pub struct StatusReport {
    // existing fields...
    pub total_quarantined: u64,                                  // new
    pub contradictions: Vec<ContradictionPair>,                  // new
    pub contradiction_count: usize,                              // new (convenience)
    pub embedding_inconsistencies: Vec<EmbeddingInconsistency>,  // new
    pub contradiction_scan_performed: bool,                      // new
    pub embedding_check_performed: bool,                         // new
}
```

**StatusParams changes**:
```rust
pub struct StatusParams {
    // existing fields...
    pub check_embeddings: Option<bool>,  // new: opt-in, default false
}
```

**context_status changes**:
1. Read `total_quarantined` counter (alongside existing active/deprecated/proposed)
2. After building the basic report, if embed service is ready:
   - Run contradiction scan (default ON)
   - If `check_embeddings == true`: run embedding consistency check
3. Add results to StatusReport
4. Format response with new sections

**Response formatting**:
- Summary: "Active: X | Deprecated: Y | Proposed: Z | Quarantined: Q | Contradictions: C"
- Markdown: Add "## Quarantine" and "## Contradictions" and "## Embedding Integrity" sections
- JSON: Add `quarantined`, `contradictions`, `embedding_inconsistencies` objects

## Data Flow

```
context_quarantine(id, reason, action)
  |
  v
server.rs::quarantine_with_audit() / restore_with_audit()
  |-- update STATUS_INDEX
  |-- update COUNTERS
  |-- write AUDIT_LOG
  |-- recompute confidence (C2 integration with crt-002)
  v
Entry status = Quarantined / Active

context_status(check_embeddings?)
  |
  v
1. Read counters (including total_quarantined)
2. Scan indexes (existing logic)
3. If embed_service ready:
   |-- contradiction::scan_contradictions(store, vector_index, embed_service, config)
   |     |-- For each active entry: re-embed, search HNSW for neighbors
   |     |-- For each high-similarity pair: run conflict_heuristic
   |     v
   |   Vec<ContradictionPair>
   |
   |-- if check_embeddings:
   |     contradiction::check_embedding_consistency(store, vector_index, embed_service, config)
   |     v
   |   Vec<EmbeddingInconsistency>
4. Build StatusReport with new fields
5. Format response

context_search / context_lookup / context_briefing
  |
  v
(existing flow, but with quarantine filtering added)
  |-- After getting results, filter out entries where status == Quarantined
```

## Component Dependencies

```
C1 (Status Extension)
 |
 +-- C2 (Retrieval Filtering) -- depends on C1
 |
 +-- C3 (Quarantine Tool) -- depends on C1, C2
 |
 +-- C5 (StatusReport Extension) -- depends on C1, C4
 |
C4 (Contradiction Detection) -- independent module, depends on existing vector/embed infrastructure
```

**Implementation order**: C1 -> C2 (parallel with C4) -> C3 -> C5

## Integration Points

### With crt-002 (Confidence)
- `base_score()` in `confidence.rs` must handle `Status::Quarantined` -> returns 0.1
- After quarantine/restore, confidence is recomputed via `update_confidence()`
- Contradiction detection does NOT modify confidence. Contradicted entries keep their confidence scores. This is a deliberate non-goal (see SCOPE.md).

### With vnc-003 (context_status)
- `context_status` handler gains contradiction scanning logic
- `StatusReport` struct gains new fields
- `format_status_report` gains new sections
- `StatusParams` gains `check_embeddings` parameter

### With vnc-002 (content scanning / validation)
- `context_quarantine` uses existing validation infrastructure
- No new content scanning needed (quarantine is a status transition, not a content operation)

### With nxs-002 (vector index)
- Contradiction scanning uses existing `VectorStore::search()` -- no API changes
- Quarantined entries remain in HNSW (not removed). Post-search filter is the approach.
- Embedding consistency check uses `VectorStore::search()` for self-match verification

### With nxs-003 (embedding)
- Contradiction scanning and embedding consistency checks require `EmbedService::embed_entry()`
- Both degrade gracefully when embed service is not ready (lazy loading)

## Technology Decisions

See ADR documents for detailed rationale:
- ADR-001: Quarantined base_score value
- ADR-002: Re-embed vs retrieve stored embeddings for contradiction scanning
- ADR-003: Conflict heuristic design and pattern library
