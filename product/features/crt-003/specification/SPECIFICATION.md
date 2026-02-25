# Specification: crt-003 Contradiction Detection

## 1. Functional Requirements

### FR-01: Status Enum Extension

**FR-01a**: The `Status` enum gains a `Quarantined` variant with `#[repr(u8)]` value `3`.

**FR-01b**: `TryFrom<u8>` for Status accepts value 3 and returns `Status::Quarantined`. Values 0-2 are unchanged. Values 4-255 remain errors.

**FR-01c**: `Display` for `Status::Quarantined` returns `"Quarantined"`.

**FR-01d**: `status_counter_key(Status::Quarantined)` returns `"total_quarantined"`.

**FR-01e**: The `total_quarantined` counter is initialized to 0 on Store::open() if not present, consistent with existing status counters.

### FR-02: Quarantine Tool

**FR-02a**: `context_quarantine` is a new MCP tool registered on the server.

**FR-02b**: Parameters:

| Parameter | Type | Required | Default | Description |
|-----------|------|----------|---------|-------------|
| `id` | i64 | Yes | -- | Entry ID to quarantine or restore |
| `reason` | String | No | -- | Reason for the action |
| `action` | String | No | "quarantine" | One of: "quarantine", "restore" |
| `agent_id` | String | No | -- | Agent identity |
| `format` | String | No | "summary" | Response format |

**FR-02c**: Capability requirement: Admin. Agents lacking Admin receive MCP error -32003 (InsufficientCapability).

**FR-02d**: Quarantine action:
- If entry does not exist: return `EntryNotFound` error
- If entry status is already Quarantined: return success (idempotent, no-op)
- Otherwise: transition status to Quarantined, decrement old status counter, increment `total_quarantined` counter, update STATUS_INDEX, write audit event, recompute confidence

**FR-02e**: Restore action:
- If entry does not exist: return `EntryNotFound` error
- If entry status is not Quarantined: return error ("entry is not quarantined")
- Otherwise: transition status to Active, decrement `total_quarantined` counter, increment `total_active` counter, update STATUS_INDEX, write audit event, recompute confidence

**FR-02f**: Both quarantine and restore operations are atomic (single write transaction).

**FR-02g**: Audit event for quarantine includes: operation "context_quarantine", target_ids [entry_id], detail includes action and reason.

### FR-03: Retrieval Filtering

**FR-03a**: `context_search` excludes entries with `Status::Quarantined` from results. The filter is applied after HNSW search returns candidates and after metadata filtering.

**FR-03b**: `context_lookup` excludes entries with `Status::Quarantined` from results when the `status` parameter is not provided (default behavior returns Active only, unchanged). When `status` parameter is explicitly "quarantined", entries with Quarantined status are returned.

**FR-03c**: `context_briefing` excludes entries with `Status::Quarantined` from both the lookup and search phases.

**FR-03d**: `context_get` returns entries regardless of status, including Quarantined. No filtering.

**FR-03e**: `context_correct` rejects correction of Quarantined entries with an error. A quarantined entry must be restored before it can be corrected.

### FR-04: Contradiction Detection

**FR-04a**: A `scan_contradictions` function accepts a store reference, vector index reference, embed service reference, and configuration. It returns a list of `ContradictionPair` structs.

**FR-04b**: The scan iterates all active entries, re-embeds each one, searches HNSW for top-K neighbors (K configurable, default 10), and runs the conflict heuristic on each pair where similarity exceeds the threshold (configurable, default 0.85).

**FR-04c**: Contradiction pairs are deduplicated: for entries A and B, only (min(A.id, B.id), max(A.id, B.id)) is reported, never both (A,B) and (B,A).

**FR-04d**: The conflict heuristic uses three weighted signals:
- Negation opposition (weight 0.6): detects "use X" vs "avoid X" patterns
- Incompatible directives (weight 0.3): detects "use A" vs "use B" for the same purpose
- Opposing sentiment (weight 0.1): detects positive vs negative framing

**FR-04e**: The conflict heuristic returns a score in [0.0, 1.0] and an explanation string. Pairs are flagged when `conflict_score >= (1.0 - conflict_sensitivity)`.

**FR-04f**: `ContradictionPair` contains: entry_id_a, entry_id_b, title_a, title_b, similarity, conflict_score, explanation.

**FR-04g**: Results are sorted by conflict_score descending.

**FR-04h**: Deprecated and Quarantined entries are excluded from the scan (only Active entries are scanned).

### FR-05: Embedding Consistency Check

**FR-05a**: A `check_embedding_consistency` function accepts a store reference, vector index reference, embed service reference, and configuration. It returns a list of `EmbeddingInconsistency` structs.

**FR-05b**: For each active entry, the function re-embeds the title+content, searches HNSW with the re-computed embedding for top-1, and checks whether the entry itself is the top result with similarity >= threshold (configurable, default 0.99).

**FR-05c**: Entries where the self-match similarity is below threshold, or the entry is not the top-1 result, are flagged as inconsistent.

**FR-05d**: `EmbeddingInconsistency` contains: entry_id, title, expected_similarity (the actual similarity observed).

### FR-06: StatusReport Extension

**FR-06a**: `StatusReport` gains these fields:
- `total_quarantined: u64`
- `contradictions: Vec<ContradictionPair>`
- `contradiction_count: usize`
- `embedding_inconsistencies: Vec<EmbeddingInconsistency>`
- `contradiction_scan_performed: bool`
- `embedding_check_performed: bool`

**FR-06b**: `StatusParams` gains `check_embeddings: Option<bool>` (default false).

**FR-06c**: `context_status` reads `total_quarantined` from the COUNTERS table.

**FR-06d**: If the embed service is ready, `context_status` performs contradiction scanning by default. The `contradiction_scan_performed` field indicates whether the scan ran.

**FR-06e**: If `check_embeddings` is true AND the embed service is ready, `context_status` performs the embedding consistency check. The `embedding_check_performed` field indicates whether the check ran.

**FR-06f**: If the embed service is not ready, both scans are skipped. The report includes quarantine counts but no contradictions or embedding inconsistencies.

### FR-07: Response Formatting

**FR-07a**: `format_status_report` includes quarantine count in all three formats (summary, markdown, json).

**FR-07b**: In markdown format, contradictions appear under "## Contradictions" with a table of pairs (IDs, titles, similarity, conflict score, explanation).

**FR-07c**: In markdown format, embedding inconsistencies appear under "## Embedding Integrity" with a table of flagged entries.

**FR-07d**: In JSON format, `contradictions` is an array of objects. `embedding_inconsistencies` is an array of objects.

**FR-07e**: `format_quarantine_success` and `format_restore_success` provide responses for the quarantine tool in all three formats.

## 2. Non-Functional Requirements

**NFR-01: Performance**: Contradiction scanning completes within 30 seconds for up to 2000 active entries. The primary cost is embedding generation (~5ms/entry) and HNSW search (~1ms/search).

**NFR-02: Backward compatibility**: All existing tool behavior is unchanged. No existing parameters are removed or reinterpreted. No schema migration is needed.

**NFR-03: Graceful degradation**: When the embed service is not ready, contradiction scanning and embedding consistency checks are silently skipped. The rest of `context_status` still works.

**NFR-04: Memory**: The contradiction scanner's working set is proportional to the number of active entries (one embedding per entry = 384 floats * 4 bytes = 1.5KB per entry). At 2000 entries: ~3MB working set.

**NFR-05: Idempotency**: Quarantining an already-quarantined entry is a no-op. Scanning the same knowledge base multiple times produces the same results (deterministic embedding model).

## 3. Domain Model

### Status Lifecycle (extended)

```
    +--------+     context_store      +---------+
    | (none) | ----create-----------> | Active  |
    +--------+                        +---------+
                                     /  |   |   \
                       context_correct  |   |    context_quarantine
                       (deprecated      |   |    (quarantine action)
                        original)       |   |            |
                              |         |   |            v
                              v         |   |      +-------------+
                        +-----------+   |   |      | Quarantined |
                        | Deprecated|   |   |      +-------------+
                        +-----------+   |   |            |
                              ^         |   |    context_quarantine
                              |         |   |    (restore action)
                     context_deprecate  |   |            |
                                        |   +<-----------+
                                        |
                                        | context_correct
                                        | (creates new Active entry)
                                        v
                                   +---------+
                                   | Active  | (correction entry)
                                   +---------+
```

**Transition rules**:
- Active -> Quarantined (via context_quarantine, quarantine action)
- Quarantined -> Active (via context_quarantine, restore action)
- Active -> Deprecated (via context_deprecate or context_correct)
- Quarantined -> Deprecated: NOT allowed (must restore first, then deprecate)
- Quarantined -> correction target: NOT allowed (must restore first)
- Any status -> Quarantined: ONLY Active entries can be quarantined (prevents double-status confusion)

**Correction**: Only Active entries can be quarantined. Proposed entries cannot (they have not been ratified). Deprecated entries cannot (they are already inactive). This simplifies the state machine and prevents confusion.

### Contradiction Pair

A contradiction pair represents two active entries that:
1. Have embedding similarity above a threshold (semantically related)
2. Contain content that the conflict heuristic flags as potentially contradictory

A contradiction pair is informational -- it is surfaced in `context_status` for human review. It does not trigger any automatic action.

## 4. Acceptance Criteria Verification Methods

| AC | Verification Method |
|----|-------------------|
| AC-01 | Unit test: `Status::try_from(3u8) == Ok(Status::Quarantined)` |
| AC-02 | Integration test: context_quarantine tool handler with valid/invalid params |
| AC-03 | Integration test: verify STATUS_INDEX and COUNTERS after quarantine |
| AC-04 | Integration test: verify STATUS_INDEX and COUNTERS after restore |
| AC-05 | Integration test: quarantine already-quarantined entry returns success |
| AC-06 | Integration test: restore non-quarantined entry returns error |
| AC-07 | Integration test: context_search with quarantined entries, verify exclusion |
| AC-08 | Integration test: context_lookup with/without status param, verify exclusion |
| AC-09 | Integration test: context_briefing with quarantined entries, verify exclusion |
| AC-10 | Integration test: context_get returns quarantined entry |
| AC-11 | Integration test: scan_contradictions finds known contradictory pair |
| AC-12 | Unit test: deduplication -- (A,B) and (B,A) produce single result |
| AC-13 | Unit test: conflict heuristic detects "use X" vs "avoid X" patterns |
| AC-14 | Integration test: context_status includes total_quarantined |
| AC-15 | Integration test: context_status includes contradictions when embed ready |
| AC-16 | Integration test: embedding consistency check detects modified entry |
| AC-17 | Integration test: context_status with check_embeddings=true |
| AC-18 | Unit test: scan uses HNSW search (mock vector store) |
| AC-19 | Unit test: StatusReport has all new fields |
| AC-20 | All test types present; CI passes |
| AC-21 | Existing tests pass without modification (or with minimal Status match updates) |
| AC-22 | Integration test: context_quarantine with non-existent ID returns error |
| AC-23 | Integration test: confidence changes after quarantine/restore |
| AC-24 | Integration test: all new response formats produce valid output |

## 5. Constraints

- No new crate dependencies
- `#![forbid(unsafe_code)]`, edition 2024, MSRV 1.89
- Object-safe traits preserved
- Cumulative test infrastructure
- No schema migration (Status is u8, new variant is just a new value)
- Quarantined entries remain in HNSW index (not removed)
- Contradiction scanning requires EmbedService (graceful degradation when unavailable)
