# crt-003: Contradiction Detection

## Problem Statement

Unimatrix accumulates knowledge across feature cycles. Entries stored months apart by different agents can contain contradictory advice on the same topic. Today, there is no mechanism to detect or surface these contradictions. A "convention" entry stating "always use serde for config parsing" and another stating "use TOML raw parsing for config, avoid serde overhead" can coexist indefinitely, both discoverable via semantic search, both returned to agents with equal confidence -- and agents will arbitrarily follow whichever one they retrieve first.

This is not hypothetical. The product vision identifies semantic poisoning as the highest-severity knowledge integrity risk (see `product/research/mcp-security/RESEARCH-knowledge-integrity.md`). Demonstrated attacks (PoisonedRAG, ADMIT, MemoryGraft) achieve 86-90% success rates by injecting a handful of semantically plausible but misleading entries. In Unimatrix's architecture, a single poisoned "convention" or "decision" entry propagates across every future feature cycle through `context_briefing` and `context_search`.

crt-003 is the primary defense against this risk class. It detects entries with high embedding similarity but potentially conflicting content, surfaces them through `context_status`, and introduces entry quarantine as an isolation mechanism for suspicious entries. It also adds embedding consistency checks to detect relevance hijacking -- entries whose embeddings have been crafted to appear near high-value queries despite unrelated content.

Without crt-003, the M4 milestone goal ("contradictions surface") remains unmet, and the auditable knowledge lifecycle value proposition has a critical gap: knowledge is hash-chained and trust-tiered but never cross-validated.

## Goals

1. **Detect contradictions between semantically similar entries** -- For active entries, identify pairs with embedding similarity above a configurable threshold (default 0.85) that appear to contain conflicting content. Use a lightweight content-level conflict signal (NLI-inspired heuristic) to distinguish genuine contradictions from legitimate related entries (e.g., two complementary conventions on the same topic).

2. **Surface contradictions through `context_status`** -- Extend the existing `StatusReport` to include a contradiction section: count of detected contradiction pairs, and the pairs themselves (entry IDs, similarity score, conflict signal). This gives human operators visibility into knowledge base integrity.

3. **Add `Quarantined` status to entry lifecycle** -- Introduce a new `Status::Quarantined` variant that excludes entries from retrieval (search, lookup, briefing) but preserves them for forensic analysis. Quarantined entries remain in the ENTRIES table and are visible via `context_get` (by ID) and `context_status`. This is the isolation mechanism for entries flagged by contradiction detection or manual review.

4. **Embedding consistency checks** -- Detect relevance hijacking by re-embedding an entry's content and comparing the result to its stored embedding. Entries whose stored embedding diverges significantly from the re-computed embedding are flagged as inconsistent. This catches entries that were inserted with manipulated embeddings (or whose content was modified after embedding).

5. **Quarantine tool** -- Add a `context_quarantine` tool that allows Admin-level agents to quarantine a specific entry, providing a reason. Quarantined entries get a `Quarantined` status and are excluded from normal retrieval, but can still be viewed by ID and appear in status reports.

6. **Filter quarantined entries from retrieval** -- Modify existing retrieval tools (`context_search`, `context_lookup`, `context_briefing`) to exclude entries with `Quarantined` status, consistent with how `Deprecated` entries are handled via `STATUS_INDEX`.

## Non-Goals

- **No automated quarantine.** crt-003 detects and surfaces contradictions. It does not automatically quarantine entries. The decision to quarantine is a human or Admin action. Automated quarantine creates a denial-of-service vector: an attacker could inject entries designed to trigger false positives, quarantining legitimate knowledge.
- **No NLI model integration.** Full natural language inference (textual entailment detection) requires a second ONNX model and significant compute. crt-003 uses a lightweight heuristic approach to conflict detection, not a trained NLI classifier.
- **No pairwise all-to-all comparison.** O(n^2) pairwise similarity scans are prohibitive. crt-003 uses the existing HNSW index to find high-similarity neighbors efficiently (per-entry nearest-neighbor queries), not brute-force pairwise comparison.
- **No real-time contradiction blocking on insert.** Checking every new entry against all existing entries at insert time would add latency to `context_store`. Contradiction detection runs as part of `context_status` (on-demand scan), not inline on every write.
- **No contradiction resolution workflow.** Choosing which of two contradicting entries is correct requires human judgment. crt-003 flags contradictions; it does not resolve them. Resolution is a human action (correct, deprecate, or quarantine one of the entries).
- **No cross-project contradiction detection.** Single-project scope only (consistent with dsn-002 project isolation).
- **No confidence impact from contradictions.** Contradicted entries do not have their confidence score reduced automatically. This would require defining how much contradiction should penalize confidence, which is a policy decision better left to a future feature.
- **No UI for contradiction management.** Visualization and interactive management is mtx-002 (Knowledge Explorer).
- **No background/scheduled scanning.** The server has no scheduler. Contradiction scanning runs on-demand via `context_status`. A future feature could add periodic scanning.

## Background Research

### Existing Infrastructure

**Embedding similarity search (nxs-002):** `VectorIndex::search()` returns `SearchResult { entry_id, similarity }` where similarity = `1.0 - distance` (DistDot on L2-normalized vectors, equivalent to cosine similarity). The existing `search` method finds the k nearest neighbors for a query embedding. For contradiction detection, we query each entry's embedding against the index to find its nearest neighbors -- this is the same operation, just using an existing entry's embedding as the query instead of a user query.

**Status lifecycle (nxs-001):** The `Status` enum currently has three variants: `Active(0)`, `Deprecated(1)`, `Proposed(2)`. The `STATUS_INDEX` table maps `(status_byte, entry_id) -> ()` for efficient status-filtered queries. Adding `Quarantined(3)` requires:
- New enum variant with `#[repr(u8)]` value 3
- New counter key `"total_quarantined"` in COUNTERS
- STATUS_INDEX entries for quarantined items
- No schema migration needed (the Status field is stored as part of EntryRecord; the variant byte maps directly)

**Retrieval filtering:** `context_search` currently retrieves all entries from HNSW and then filters by metadata. It does NOT pre-filter by status. However, `context_lookup` uses `QueryFilter` which defaults to `Status::Active` when no status is specified. For search, the filtering happens at the response level -- search returns candidates from the full HNSW index, and then the server filters results. To exclude quarantined entries from search, we need to add status filtering to the search result processing (same approach used for metadata filtering).

**Embedding regeneration (nxs-003):** `EmbedService::embed_entry(title, content)` generates a fresh embedding from text. For consistency checks, we call this with the stored entry's title+content and compare the result to the stored embedding (looked up via VECTOR_MAP -> HNSW data point).

**Content scanning (vnc-002):** The existing `ContentScanner` uses regex patterns. While contradiction detection is semantic (not pattern-based), the scanning infrastructure demonstrates the pattern of analyzing content at tool handler level.

### Contradiction Detection Approaches

The product vision references "ReasoningBank's contradiction pipeline." The general approach in the literature for lightweight contradiction detection in knowledge bases:

1. **Embedding similarity threshold:** Entries with cosine similarity > 0.85 are semantically related enough to potentially conflict. Below this threshold, entries are on different topics and cannot meaningfully contradict each other.

2. **Content-level conflict signals:** Once high-similarity pairs are identified, analyze content for conflict indicators:
   - **Negation patterns:** One entry contains "always X" and the other "never X", or "do X" vs "do not X", "use X" vs "avoid X".
   - **Incompatible directives:** Both entries claim authority on the same topic but prescribe different actions.
   - **Temporal supersession without correction chain:** Entry B was created after Entry A on the same topic, but does not reference A via `supersedes`. This suggests unintentional contradiction rather than deliberate correction.

3. **Lightweight heuristic vs. NLI model:** A full NLI model (e.g., bart-large-mnli) would classify pairs as entailment/neutral/contradiction with ~90% accuracy. However, this requires loading a second ONNX model (~1.5GB), adding significant memory and latency. The lightweight alternative uses pattern matching and structural analysis -- less accurate but zero additional dependencies. Given Unimatrix's scale (hundreds to low thousands of entries per project), false positives from the heuristic are manageable and can be reviewed by humans.

### Embedding Consistency Checks

The MCP security research identifies "relevance hijacking" as a key attack vector (Section 6.1 of `RESEARCH-knowledge-integrity.md`): an attacker crafts a document whose embedding is deceptively close to high-value queries despite unrelated content. Detection: re-embed the entry's text and compare with the stored embedding. If the cosine similarity between the stored embedding and the re-computed embedding is below a threshold (e.g., 0.99), the entry's embedding may have been tampered with or the entry was inserted with a pre-computed adversarial embedding.

In Unimatrix's current architecture, `VectorIndex::insert()` embeds the entry's text at insert time. There is no API to insert an entry with a pre-computed embedding. This means embedding consistency violations should not normally occur -- they would indicate either a code bug or a modified vector index file. However, the check is cheap to implement and provides defense-in-depth.

### Quarantine Mechanics

The product vision (Section 8.3 of the security research) identifies quarantine as a key recovery mechanism: "Flag suspicious entries as quarantined rather than deleting them. Quarantined entries are excluded from retrieval but preserved for forensic analysis."

Key design considerations:
- Quarantine is a status transition like deprecation, but semantically different: deprecated means "no longer relevant," quarantined means "potentially harmful, under review."
- Quarantined entries should be excluded from `context_search`, `context_lookup`, and `context_briefing` results.
- Quarantined entries should still be retrievable by `context_get` (by ID) for investigation.
- `context_status` should report quarantine counts and list quarantined entries.
- Unquarantining (restoring to Active) should be possible via `context_quarantine` with an explicit action.

### Scale Considerations

Contradiction scanning is per-entry nearest-neighbor search: for each active entry, find its top-k neighbors above the similarity threshold and check for conflicts. With n active entries and k neighbors per entry:
- HNSW queries: O(n) searches, each O(log n) via HNSW
- Conflict analysis: O(n * k) content comparisons
- Total: O(n * log n) + O(n * k)

At Unimatrix's expected scale (100-2000 entries per project), with k=5, this is 500-10000 conflict checks. Each check is string pattern matching -- microseconds per check. The HNSW searches dominate at ~1-5ms each. Total scan time: 0.1-10 seconds. Acceptable for an on-demand `context_status` call. At larger scales, incremental scanning (only check entries modified since last scan) would be needed.

## Proposed Approach

### Status Enum Extension

Add `Quarantined = 3` to the `Status` enum in `unimatrix-store/src/schema.rs`. This requires:
- New `#[repr(u8)]` variant
- `TryFrom<u8>` match arm
- `Display` impl match arm
- New counter key `"total_quarantined"` in COUNTERS
- No schema migration (bincode serializes Status as u8, existing entries will never contain value 3)

### Quarantine Tool

New tool `context_quarantine` in `tools.rs`:
- Parameters: `id` (required), `reason` (optional), `action` (optional: "quarantine" default, "restore"), `agent_id`, `format`
- Capability: Admin
- Quarantine action: transition entry status to `Quarantined`, log audit event with reason
- Restore action: transition entry status to `Active`, log audit event
- Atomic status transition + audit in single write transaction

### Retrieval Filtering

Modify retrieval paths to exclude `Quarantined` entries:
- `context_search`: Add status check to result filtering (after HNSW search, before response formatting). Currently filters by metadata; add status != Quarantined check.
- `context_lookup`: `QueryFilter` defaults to `Status::Active`. When status is explicitly provided, allow searching for Quarantined entries. When not provided (default), exclude Quarantined.
- `context_briefing`: Uses lookup + search internally. Both will inherit the filtering.
- `context_get`: No change -- entries are retrievable by ID regardless of status, for forensic investigation.

### Contradiction Detection Module

New module `contradiction.rs` in `unimatrix-server`:

**Contradiction scan flow:**
1. Iterate all active entries from ENTRIES table
2. For each entry, retrieve its embedding from the HNSW index (via VECTOR_MAP entry_id -> data_id -> embedding)
3. Search HNSW for top-k neighbors (k=10) with similarity > threshold (0.85)
4. For each high-similarity pair, run the conflict heuristic on their content
5. Collect pairs flagged as contradictions: `(entry_id_a, entry_id_b, similarity, conflict_signal)`
6. Deduplicate symmetric pairs: only report (A, B) not both (A, B) and (B, A)

**Conflict heuristic:**
A lightweight rule-based classifier that looks for signals of content conflict between two entries:
- Negation opposition: one entry says "do X" and the other says "don't X" or "avoid X" or "never X"
- Incompatible value directives: both entries address the same technical choice but prescribe different options (e.g., "use library A" vs "use library B" for the same purpose)
- Categorical conflict: entries in the same category and topic but with opposing sentiment (positive vs negative framing of the same practice)

The heuristic returns a conflict score in [0.0, 1.0] and a brief explanation string. Pairs with conflict score above a configurable sensitivity threshold (default 0.5) are flagged. Lower threshold = more sensitive (more flags, more false positives); higher threshold = more specific (fewer flags, may miss subtle contradictions).

**Embedding consistency check:**
1. For each entry, re-embed its title+content using `EmbedService`
2. Retrieve the stored embedding from HNSW (via data point lookup)
3. Compute cosine similarity between stored and re-computed embeddings
4. Flag entries where similarity < 0.99 (configurable threshold)

This check is opt-in via a `check_embeddings: Option<bool>` parameter on `context_status` (default false).

### StatusReport Extension

Extend the existing `StatusReport` struct with:
- `total_quarantined: u64` -- count of quarantined entries
- `contradictions: Vec<ContradictionPair>` -- detected contradiction pairs
- `embedding_inconsistencies: Vec<EmbeddingInconsistency>` -- entries with embedding drift
- `contradiction_scan_performed: bool` -- whether the scan ran (requires embed service to be ready)

### Response Formatting Extension

Extend `format_status_report` to include:
- Quarantine counts alongside existing status counts
- Contradiction section with pairs listed (entry IDs, titles, similarity, conflict signal)
- Embedding inconsistency section if the check was performed

## Acceptance Criteria

- AC-01: `Status::Quarantined` variant exists with `#[repr(u8)]` value 3, with TryFrom, Display, and counter key implementations
- AC-02: `context_quarantine` tool accepts `id` (required), optional `reason`, `action` ("quarantine" or "restore", default "quarantine"), `agent_id`, `format`. Requires Admin capability.
- AC-03: Quarantining an entry sets its status to `Quarantined` in a single atomic write transaction with audit event
- AC-04: Restoring a quarantined entry sets its status back to `Active` in a single atomic write transaction with audit event
- AC-05: Quarantining an already-quarantined entry is idempotent (returns success, no-op)
- AC-06: Restoring a non-quarantined entry returns an error (only quarantined entries can be restored)
- AC-07: `context_search` excludes entries with `Quarantined` status from results
- AC-08: `context_lookup` excludes entries with `Quarantined` status from results when no explicit status filter is provided
- AC-09: `context_briefing` excludes entries with `Quarantined` status from results
- AC-10: `context_get` returns quarantined entries (no filtering by status for direct ID access)
- AC-11: Contradiction detection scans all active entries, finding pairs with embedding similarity above a configurable threshold (default 0.85) and flagged by the conflict heuristic
- AC-12: Contradiction pairs are deduplicated (only (A,B) reported, not also (B,A))
- AC-13: The conflict heuristic detects negation opposition (e.g., "use X" vs "avoid X") and incompatible directives between entry contents, with a configurable sensitivity threshold (default 0.5) controlling the false-positive/miss trade-off
- AC-14: `context_status` reports include `total_quarantined` count alongside existing status counts
- AC-15: `context_status` performs contradiction scanning by default when the embed service is ready. Contradiction pairs are included in the report.
- AC-16: Embedding consistency checks re-embed entry content and compare to stored embedding, flagging entries where similarity < threshold (default 0.99)
- AC-17: Embedding consistency results are included in `context_status` when `check_embeddings` parameter is true (opt-in, default false)
- AC-18: Contradiction scanning uses HNSW nearest-neighbor search (not brute-force pairwise), keeping time complexity at O(n log n) for n entries
- AC-19: `StatusReport` struct extended with quarantine count, contradiction pairs, and embedding inconsistency fields
- AC-20: All new code has unit tests; integration tests verify quarantine lifecycle, retrieval filtering, and contradiction detection
- AC-21: Existing tests continue to pass (no regressions from Status enum extension)
- AC-22: `context_quarantine` returns `ServerError::Core(EntryNotFound)` when the ID does not exist
- AC-23: Confidence is recomputed when an entry is quarantined (quarantined entries receive base_score treatment consistent with their status)
- AC-24: All new tools and response formats support the `format` parameter (summary/markdown/json) consistent with existing tools

## Constraints

- **Status enum is `#[repr(u8)]` with exhaustive match.** Adding `Quarantined = 3` will cause compile errors in every unhandled match arm. This is intentional -- forces all status-handling code to be updated.
- **bincode positional encoding.** The `Status` field is serialized as part of `EntryRecord`. Since Status is serialized as a u8 (via `#[repr(u8)]`), adding a new variant does not change the serialization format -- existing entries with Status values 0, 1, 2 still deserialize correctly. No schema migration required.
- **HNSW index must be loaded.** Contradiction scanning and embedding consistency checks require the HNSW index and embedding model. If the embed service is not ready (lazy loading still in progress), the scan falls back to reporting quarantine counts only.
- **No new crate dependencies.** All needed functionality (regex, HNSW search, embedding generation) is already in the dependency tree.
- **Object-safe EntryStore trait.** If any trait methods are added (e.g., `query_by_status` for Quarantined), they must maintain object safety.
- **EntryStore trait already has `update_status` and `query_by_status`.** These methods need to handle the new `Quarantined` status variant. No trait signature changes needed -- `Status::Quarantined` is just a new value for the existing `Status` type.
- **Contradiction scan is on-demand, not continuous.** The server has no background scheduler. Scanning runs during `context_status` calls by default. At scale (>2000 entries), scanning time may impact `context_status` response latency.
- **`#![forbid(unsafe_code)]`**, edition 2024, MSRV 1.89 per workspace conventions.
- **Test infrastructure is cumulative.** Build on existing test fixtures and helpers.

## Resolved Questions

1. **Contradiction scanning defaults to ON in `context_status`.** `context_status` is a batch-oriented diagnostic function, not called on every read/write. Scanning by default is appropriate. No opt-out parameter needed -- callers who want fast counts can read counters directly.

2. **Conflict heuristic uses a tunable sensitivity threshold.** A configurable `conflict_sensitivity` parameter (0.0-1.0) with a moderate default (0.5) controls the trade-off between false positives and missed contradictions. Lower values = more flags (sensitive), higher values = fewer flags (specific). The threshold is a named constant that can be adjusted via future configuration (vnc-004).

3. **Embedding consistency checks are opt-in via `check_embeddings` parameter on `context_status`.** Default is `false`. Re-embedding all entries is expensive. Callers opt in when they want the integrity check. Not a separate tool -- integrated into the existing `context_status` flow.

## Tracking

https://github.com/dug-21/unimatrix/issues/33
