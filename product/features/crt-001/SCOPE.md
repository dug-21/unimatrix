# crt-001: Usage Tracking

## Problem Statement

Unimatrix is a working knowledge engine that agents actively read from and write to. But it has no memory of *how* knowledge is used. The `access_count` and `last_accessed_at` fields on EntryRecord exist (pre-seeded in nxs-001) but are never populated -- every entry shows 0 accesses regardless of how many times it has been retrieved. There is no record of which features drive retrieval or whether retrieved entries were helpful. Without usage data, the system cannot:

- Distinguish heavily-used entries from dead weight (crt-002 confidence evolution needs `access_count` and `helpful_count`)
- Track which entries are associated with which features (feature-scoped context for col-004)
- Fade unused entries or boost helpful ones (crt-004 co-access boosting)

This is the bridge feature from passive knowledge accumulation (Milestones 1-2) to active learning (Milestone 4). Every downstream Cortical and Collective feature depends on the usage data crt-001 produces.

## Goals

1. **Populate EntryRecord usage fields** -- On every retrieval, update `access_count += 1` and `last_accessed_at = now()` on each returned entry, subject to session deduplication. These fields already exist but are always 0.

2. **Add helpfulness tracking fields** -- Add `helpful_count: u32` and `unhelpful_count: u32` to EntryRecord (new schema fields, serde default 0). The two-counter design enables proper statistical treatment (Wilson score interval) in crt-002.

3. **Record helpfulness signal with vote correction** -- Add an optional `helpful` boolean parameter to retrieval tools (`context_search`, `context_lookup`, `context_get`, `context_briefing`). When `helpful = true`, increment `helpful_count`; when `helpful = false`, increment `unhelpful_count`. When not provided, neither counter changes. Subject to session deduplication with last-vote-wins correction: if an agent changes its vote on an entry within a session, the old counter is decremented and the new counter is incremented.

4. **Session-based deduplication** -- In-memory deduplication prevents the same agent from inflating `access_count` or helpful/unhelpful votes by retrieving the same entry repeatedly within a session. Blocks the most trivial gaming vectors (loop-to-boost, helpful-flag stuffing) at near-zero cost. Vote dedup uses last-vote-wins semantics: an agent can correct an earlier vote in the same session without inflating totals.

5. **FEATURE_ENTRIES multimap** -- New redb table linking features to entries used during that feature's lifecycle. Populated on retrieval when a `feature` parameter is provided by an agent with Internal or higher trust level. Restricted agents' `feature` parameters are silently ignored -- they have no legitimate feature-lifecycle context and allowing writes would violate the read-only trust model. Enables feature-scoped queries (col-004) and cross-feature usage analysis.

6. **Write rate tracking query** -- Add a query method on AuditLog to count write operations by agent_id in a time window. Uses existing AUDIT_LOG data (writes are already audited). Enables future rate limiting enforcement.

7. **EntryStore trait extension** -- Add `record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError>` method to the EntryStore trait for batch access_count/last_accessed_at updates.

## Non-Goals

- **No confidence computation.** The confidence formula is crt-002. crt-001 provides the raw data (`access_count`, `helpful_count`, `unhelpful_count`) but does not compute or update the `confidence` field.
- **No rate limiting enforcement.** crt-001 provides the query infrastructure. The enforcement policy (reject writes above threshold) is a separate concern.
- **No anomaly detection.** Behavioral baselines require accumulated usage data over time. Detection logic is a future crt feature. The AUDIT_LOG already captures the raw data needed.
- **No co-access tracking.** Tracking entries retrieved together is crt-004. The raw data is already available in AUDIT_LOG's `target_ids`. crt-004 derives co-access pairs from there.
- **No usage-based staleness metrics.** `context_status` will not report stale entries based on usage data. That requires policy decisions about staleness thresholds.
- **No UI or CLI for usage analytics.** Raw data only. Visualization is mtx-002 (Knowledge Explorer).
- **No retroactive usage logging.** Only retrievals after crt-001 deployment are tracked.
- **No batched/async usage writes.** Each retrieval writes synchronously. If this becomes a performance bottleneck under future multi-agent concurrency, async batching can be layered on without schema changes.
- **No `helpful` feedback as a separate tool.** The signal is embedded in retrieval tool parameters.
- **No confidence formula design.** The additive weighted composite formula, log transforms, and Wilson score computation are crt-002 concerns. crt-001 collects the raw inputs.
- **No implicit outcome correlation.** Inferring helpfulness from retrieval-then-successful-outcome patterns in AUDIT_LOG is a future enhancement (Layer 3 in the gaming resistance strategy). crt-001 does not need schema changes to support it -- AUDIT_LOG already has the raw data.

## Background Research

### Why No Separate USAGE_LOG Table

The product vision specifies a USAGE_LOG table. During design, analysis of AUDIT_LOG (implemented in vnc-001) revealed significant overlap:

**What AUDIT_LOG already captures per retrieval tool call:**
- `agent_id` -- who made the request
- `operation` -- which tool (context_search, context_lookup, etc.)
- `target_ids: Vec<u64>` -- which entry IDs were returned
- `timestamp` -- when it happened
- `outcome` -- success/failure

**What downstream features actually need:**

| Consumer | Needs | Source |
|----------|-------|--------|
| crt-002 (confidence) | `access_count`, `helpful_count`, `unhelpful_count` per entry | EntryRecord fields |
| crt-004 (co-access) | which entries appeared in the same retrieval | AUDIT_LOG `target_ids` |
| col-002 (retrospective) | outcome entries by feature | FEATURE_ENTRIES + entry category |
| Security (rate limiting) | per-agent write counts in time window | AUDIT_LOG |
| Security (behavioral baselines) | per-agent access patterns | AUDIT_LOG |

No downstream feature requires per-entry-per-retrieval event granularity beyond what AUDIT_LOG + EntryRecord fields already provide. A separate USAGE_LOG would duplicate data that AUDIT_LOG already records.

**Decision:** Drop USAGE_LOG. Store aggregate usage on EntryRecord (inline updates with dedup). Feature-entry links go in FEATURE_ENTRIES. Co-access data is derivable from AUDIT_LOG `target_ids`. Write rate tracking queries AUDIT_LOG.

### Gaming Resistance Analysis

A research spike (`product/research/ass-008/USAGE-TRACKING-RESEARCH.md`) identified that naive usage counters are trivially gameable and feed directly into crt-002's confidence scoring. The analysis evaluated 12 approaches and recommends a 3-layer defense:

**Layer 1 (crt-001 — recording-time):** Session dedup + two-counter helpfulness. Blocks the cheapest attacks.
**Layer 2 (crt-002 — computation-time):** Log transforms + Wilson score + additive weighted composite. Makes remaining attacks ineffective.
**Layer 3 (future):** Implicit outcome correlation + agent diversity + anomaly detection. Adds non-gameable signals.

crt-001 implements Layer 1. The key insight: **collect gaming-resistant raw data now so crt-002 has proper inputs.** Two counters (helpful + unhelpful) enable Wilson score. Session dedup prevents count inflation. `last_accessed_at` (always updated, no dedup) provides a non-gameable recency signal.

**Gaming analysis with all three layers applied:**
- Loop-to-boost: capped at 1 access per session, log-transformed, 15% weight → marginal impact
- Helpful-flag stuffing: capped at 1 vote per session, Wilson-scored → marginal impact
- Combined worst-case: +22.5% confidence improvement vs +100% in the naive design

### Existing Schema (EntryRecord)

The `access_count: u32` and `last_accessed_at: u64` fields exist on EntryRecord with `#[serde(default)]`. They are initialized to 0 on insert and never written to by any retrieval code path. The vision specifies `usage_count` -- since `access_count` already exists and serves the identical purpose, we reuse it (no rename, avoids unnecessary migration).

Field order (last fields of EntryRecord):
```
... content_hash, previous_hash, version, feature_cycle, trust_source
                                                          ^ helpful_count appended here
                                                            ^ unhelpful_count appended here
```

### Existing Table Structure (10 tables)

Current tables: ENTRIES, TOPIC_INDEX, CATEGORY_INDEX, TAG_INDEX, TIME_INDEX, STATUS_INDEX, VECTOR_MAP, COUNTERS, AGENT_REGISTRY, AUDIT_LOG.

crt-001 adds 1 new table:
- **FEATURE_ENTRIES**: `MultimapTableDefinition<&str, u64>` (feature_id -> set of entry_ids). Same pattern as TAG_INDEX.

### Retrieval Code Paths

Four tools perform retrieval:
1. `context_search` -- semantic search, returns top-k entries with similarity scores
2. `context_lookup` -- deterministic metadata query, returns filtered entries
3. `context_get` -- single entry by ID
4. `context_briefing` -- compiled orientation (internally does lookup + search)

All four must update `access_count`/`last_accessed_at` on returned entries (subject to dedup). The briefing tool is special because it performs multiple internal retrievals -- it updates access fields once per entry in the final assembled result, not per internal query.

### Combined Transaction Pattern

vnc-002/vnc-003 established the combined transaction pattern where mutating operations happen in a single redb WriteTransaction. Usage tracking on retrieval is different -- the primary operation is a *read*, but we need a *write* for the usage updates. This means retrieval tools that currently use read transactions need a follow-up write transaction for usage recording.

**Two-transaction approach:** Read in read txn, then open write txn for usage updates. Usage data is analytics, not critical -- a crash between the two transactions loses usage events but causes no data corruption or inconsistency.

Performance: redb write transactions are serialized. Adding a write txn to every retrieval could bottleneck under concurrent access. However, Unimatrix's current deployment is single-agent stdio MCP. If multi-agent concurrency becomes a concern, async batching can be layered on without schema changes (the EntryRecord fields remain the same either way).

### Schema Migration

Adding `helpful_count: u32` and `unhelpful_count: u32` to EntryRecord requires schema migration (scan-and-rewrite). The infrastructure from nxs-004 handles this: `schema_version` counter in COUNTERS triggers `migrate_if_needed()` on Store::open(). This is the v1 -> v2 migration. Because bincode v2 uses positional encoding, existing serialized records cannot be deserialized with the new struct -- the scan-and-rewrite reads all entries with a LegacyEntryRecord (v1 layout) and writes them back with the new layout (v2).

### Security Alignment

The vision ties crt-001 to security: "Enables write rate limiting per agent and behavioral baseline establishment for anomaly detection."

- **Write rate limiting:** AUDIT_LOG already records all write operations with `agent_id` and `timestamp`. crt-001 adds a query method to count writes per agent in a time window. The enforcement policy (reject above threshold) is out of scope.
- **Behavioral baselines:** AUDIT_LOG records all operations per agent. Future anomaly detection can establish baselines from this data. No additional tables needed.
- **Read-path side effects:** crt-001 introduces write side effects on read operations (access_count/helpful_count updates). This blurs the read/write security boundary. Session deduplication mitigates the most obvious abuse vector (read-to-boost). The trust model constraint is documented: Restricted agents' reads now cause writes, but those writes are analytics (deduped counters), not knowledge mutations.

## Proposed Approach

### EntryRecord Changes

- **Reuse `access_count: u32`** -- already exists, serves as usage count, just never populated. No rename.
- **Add `helpful_count: u32`** with `#[serde(default)]` after `trust_source`.
- **Add `unhelpful_count: u32`** with `#[serde(default)]` after `helpful_count`.
- **Schema migration v1 -> v2**: scan-and-rewrite all entries to include both new fields. Same pattern as nxs-004 v0 -> v1.

### Table Design

**FEATURE_ENTRIES** (new table, table #11):
```
MultimapTableDefinition<&str, u64>  -- feature_id -> {entry_id, ...}
```

Populated on any retrieval that includes a `feature` parameter. Idempotent -- inserting a duplicate (feature_id, entry_id) pair is a no-op in redb multimap.

### Tool Parameter Changes

Add optional parameters to all four retrieval tools:
- `feature`: `Option<String>` -- work-context label for FEATURE_ENTRIES tracking
- `helpful`: `Option<bool>` -- helpfulness signal for the returned entries

These are optional, backward-compatible additions. Existing tool calls without these parameters continue to work unchanged.

### Session Deduplication

In-memory `UsageDedup` struct held by the server:

```rust
struct UsageDedup {
    /// (agent_id, entry_id) pairs where access_count has been incremented this session.
    access_counted: HashSet<(String, u64)>,
    /// (agent_id, entry_id) -> last vote value. Tracks the most recent vote per agent per entry.
    /// Enables last-vote-wins correction: if an agent changes its mind, the old counter is
    /// decremented and the new counter is incremented.
    vote_recorded: HashMap<(String, u64), bool>,
}
```

- **In-memory only, not persisted.** Cleared on server restart. A new session = a new legitimate access.
- **Why not persisted:** Persisting the dedup set would require a new table and add write overhead. The purpose is to block intra-session loops, not cross-session access. An agent that accesses an entry once per session across 100 sessions is genuine usage.
- **Thread safety:** `Mutex<UsageDedup>` or `RwLock<UsageDedup>` — low contention in single-agent stdio.
- **Memory cost:** ~72 bytes per unique (agent_id, entry_id) pair (HashMap entry slightly larger than HashSet). At 1000 entries: ~72 KB. Well within budget.
- **Last-vote-wins:** An agent that initially votes `helpful=false` (speculative retrieval) and later votes `helpful=true` (after confirming the entry was useful) gets the correction applied: unhelpful_count decremented, helpful_count incremented. This prevents early incorrect assessments from permanently degrading entry quality.

### Recording Flow

On every successful retrieval:
1. Execute the query (read transaction, unchanged)
2. Resolve agent_id from identity pipeline
3. Check dedup set for each returned entry_id:
   a. If (agent_id, entry_id) NOT in `access_counted`: mark it, include in access update batch
   b. If `helpful` is `Some(value)`: check `vote_recorded` for (agent_id, entry_id):
      - No prior vote: record it, include in helpful or unhelpful batch
      - Prior vote with same value: no-op
      - Prior vote with different value: include in new vote batch AND in correction batch (decrement old counter)
4. Open a write transaction for usage updates:
   a. For entries in the access batch: increment `access_count`, set `last_accessed_at` to now
   b. For entries in the vote batch: increment `helpful_count` or `unhelpful_count` as appropriate
   c. For entries in the correction batch: decrement the old counter (saturating subtraction, floor at 0)
   d. If `feature` is provided AND agent trust level >= Internal: insert (feature_id, entry_id) into FEATURE_ENTRIES
   e. `last_accessed_at` is ALWAYS updated for all returned entries (even deduped) -- recency is not a gameable signal
5. Commit the write transaction

Steps 4a-4d happen in a single write transaction for the entire entry batch.

For `context_briefing`: dedup and usage updates apply to entries in the final assembled result only, not to intermediate internal queries. This avoids double-counting entries that appear in both the lookup and search phases.

### Module Structure

**In `unimatrix-store`** (extend existing modules):
- `Store::record_usage(entry_ids: &[u64], helpful: Option<bool>, deduped_access_ids: &[u64], deduped_vote_ids: &[u64])` -- batch update on multiple entries in one write transaction. The caller (server layer) determines which IDs are deduped; the store just applies the updates.
- `Store::record_feature_entries(feature: &str, entry_ids: &[u64])` -- batch insert into FEATURE_ENTRIES
- FEATURE_ENTRIES table definition in `tables.rs`
- Schema migration v1 -> v2 in `migration.rs`

**In `unimatrix-core`** (trait extension):
- `EntryStore::record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError>` -- trait method for access recording
- Async wrapper under `async` feature gate

**In `unimatrix-server`** (tool modifications + new module):
- `usage_dedup.rs` -- `UsageDedup` struct with `check_access` and `check_vote` methods
- Add `feature` and `helpful` parameters to retrieval tool handlers
- Call dedup check + `record_usage` + `record_feature_entries` after each successful retrieval
- Add `AuditLog::write_count_since(agent_id: &str, since: u64) -> Result<u64, ServerError>` for rate tracking

### Write Rate Tracking

New method on `AuditLog`:
```rust
pub fn write_count_since(&self, agent_id: &str, since: u64) -> Result<u64, ServerError>
```

Scans AUDIT_LOG for entries where `operation` is a write tool (`context_store`, `context_correct`) and `agent_id` matches, with `timestamp >= since`. Returns the count. This uses existing AUDIT_LOG data -- no new tables.

## Acceptance Criteria

- AC-01: FEATURE_ENTRIES multimap table exists and is created on Store::open()
- AC-02: `helpful_count: u32` and `unhelpful_count: u32` fields added to EntryRecord with serde(default), appended after `trust_source` in positional order
- AC-03: Schema migration v1 -> v2 runs on Store::open(), backfilling helpful_count = 0 and unhelpful_count = 0 for all existing entries
- AC-04: EntryRecord.access_count is incremented at most once per (agent_id, entry_id) per server session. Subsequent retrievals of the same entry by the same agent in the same session do not increment access_count.
- AC-05: EntryRecord.last_accessed_at is set to current unix timestamp on every successful retrieval (always updated, not subject to deduplication)
- AC-06: When `helpful = Some(true)` is provided and no prior vote exists for this (agent_id, entry_id) this session, `helpful_count` is incremented on each returned entry
- AC-07: When `helpful = Some(false)` is provided and no prior vote exists for this (agent_id, entry_id) this session, `unhelpful_count` is incremented on each returned entry. When `helpful` is `None`, neither counter changes.
- AC-08: When `feature` parameter is provided AND the agent's trust level is Internal or higher, (feature_id, entry_id) pairs are inserted into FEATURE_ENTRIES. Restricted agents' `feature` parameters are silently ignored.
- AC-09: FEATURE_ENTRIES does not create duplicate pairs (multimap idempotency)
- AC-10: context_briefing updates usage fields once per entry in the final assembled result (no double-counting from internal lookup + search)
- AC-11: EntryStore trait extended with `record_access` method (object-safe, &self, batch entry_ids)
- AC-12: AuditLog::write_count_since returns count of write operations by agent_id since a timestamp
- AC-13: Existing retrieval tool behavior unchanged (same results, same response formats) -- usage tracking is a side effect only
- AC-14: All usage updates (access_count, last_accessed_at, helpful_count, unhelpful_count, FEATURE_ENTRIES) are atomic within a single write transaction per retrieval
- AC-15: In-memory UsageDedup tracks (agent_id, entry_id) for access counting and vote recording per server session. Dedup state is cleared on server restart and is not persisted.
- AC-16: Vote correction: when an agent changes its vote on an entry within a session (e.g., from unhelpful to helpful), the old counter is decremented (saturating at 0) and the new counter is incremented. Net effect is a vote flip, not an inflation.
- AC-17: Restricted agents' `feature` parameters are silently ignored. Only Internal or higher trust levels write to FEATURE_ENTRIES.
- AC-18: All new code has unit tests; integration tests verify end-to-end retrieval-to-usage-update flow including dedup and vote correction behavior

## Constraints

- **bincode v2 positional encoding**: `helpful_count` and `unhelpful_count` must be appended after `trust_source` (last field). No field reordering.
- **Schema migration required**: Adding both fields requires scan-and-rewrite (same pattern as nxs-004 v0 -> v1, this is v1 -> v2).
- **Synchronous writes in read path**: Usage updates add a write transaction to every retrieval. Acceptable for single-agent stdio deployment. Can be replaced with async batching later without schema changes.
- **AuditEvent struct not modified**: Adding fields to AuditEvent would break bincode deserialization of existing AUDIT_LOG entries. The `feature` context is captured in FEATURE_ENTRIES, not AUDIT_LOG.
- **EntryStore trait is object-safe**: New methods must maintain object safety (no `&mut self`, no generics).
- **No async in unimatrix-store**: The store crate is synchronous. Usage recording methods are synchronous. Dedup logic lives in the server layer (async-capable).
- **redb table count**: Going from 10 to 11 tables is fine. No hard limit concern.

## Decisions

1. **Reuse `access_count`, don't rename to `usage_count`** -- the field exists, serves the same purpose, renaming adds migration complexity with zero value.
2. **Drop USAGE_LOG table** -- AUDIT_LOG + EntryRecord fields cover all downstream consumer needs. See "Why No Separate USAGE_LOG Table" above.
3. **Two-transaction approach for retrieval** -- read in read txn, then write txn for usage updates. Usage data is analytics; lost events on crash are acceptable.
4. **context_briefing logs per-entry in final result** -- one usage update per entry in the assembled response, not per internal query.
5. **`feature` as a portable work-context label** -- opaque string, not tied to any specific workflow. Could be a feature ID, sprint name, issue number, or any grouping the caller chooses.
6. **Two-counter helpfulness (helpful + unhelpful)** -- enables proper Wilson score interval in crt-002. A single helpful_count cannot distinguish "no one voted" from "everyone voted unhelpful."
7. **In-memory session deduplication** -- blocks loop-to-boost and helpful-flag stuffing at near-zero cost. Not persisted; cleared on restart. A new session is a legitimate new access.
8. **`last_accessed_at` always updated (no dedup)** -- recency is not a gameable signal. It provides a non-manipulable freshness input for crt-002.
9. **Dedup at server layer, not store layer** -- the store applies updates; the server decides what to update. This keeps the store crate simple and synchronous, and puts the dedup logic where the agent_id context is available.
10. **Last-vote-wins with correction** -- an agent can change its vote within a session. The old counter is decremented, the new counter is incremented. This prevents early incorrect assessments from permanently degrading entry quality. A knowledge base that gets smarter should allow self-correction, not lock in first impressions.
11. **FEATURE_ENTRIES gated by trust level** -- only Internal+ agents write to FEATURE_ENTRIES. Restricted agents (read-only, auto-enrolled unknowns) have no feature-lifecycle context and should not create analytics associations. This preserves the read-only trust model for Restricted agents.

## Tracking

https://github.com/dug-21/unimatrix/issues/25
