# Specification: crt-001 Usage Tracking

## Objective

Add usage tracking to Unimatrix's retrieval pipeline so that every knowledge access is recorded with deduplication and vote correction, enabling downstream features (confidence evolution, co-access boosting, feature lifecycle) to consume gaming-resistant usage signals from EntryRecord fields and a new FEATURE_ENTRIES table. FEATURE_ENTRIES writes are gated by agent trust level to preserve the read-only trust model for Restricted agents.

## Functional Requirements

### FR-01: EntryRecord Schema Extension
The system shall add two new fields to `EntryRecord`, appended after `trust_source` in positional order:
- `helpful_count: u32` (serde default: 0)
- `unhelpful_count: u32` (serde default: 0)

### FR-02: Schema Migration V1 to V2
On `Store::open()`, if `schema_version < 2`, the system shall scan all existing entries, deserialize with the V1 (24-field) layout, construct new EntryRecord with `helpful_count = 0` and `unhelpful_count = 0`, and rewrite. The schema_version counter shall be set to 2 after migration completes.

### FR-03: FEATURE_ENTRIES Table
The system shall create a new `FEATURE_ENTRIES` multimap table (`MultimapTableDefinition<&str, u64>`) on `Store::open()`. This table maps feature identifiers to sets of entry IDs.

### FR-04: Access Count Increment
On every successful retrieval (context_search, context_lookup, context_get, context_briefing), the system shall increment `access_count` by 1 for each returned entry, subject to session deduplication (FR-08).

### FR-05: Last Accessed Timestamp Update
On every successful retrieval, the system shall set `last_accessed_at` to the current unix timestamp (seconds) for each returned entry. This update is NOT subject to deduplication -- it always occurs.

### FR-06: Helpful Count Increment
When a retrieval tool is called with `helpful = true` and no prior vote exists for this (agent_id, entry_id) this session, the system shall increment `helpful_count` by 1 for each returned entry.

### FR-07: Unhelpful Count Increment
When a retrieval tool is called with `helpful = false` and no prior vote exists for this (agent_id, entry_id) this session, the system shall increment `unhelpful_count` by 1 for each returned entry. When `helpful` is omitted (None), neither counter changes.

### FR-08: Access Count Deduplication
The system shall maintain an in-memory set of `(agent_id, entry_id)` pairs where `access_count` has been incremented. For any pair already in the set, subsequent retrievals shall NOT increment `access_count` (but `last_accessed_at` is still updated per FR-05).

### FR-09: Vote Tracking with Last-Vote-Wins Correction
The system shall maintain an in-memory map of `(agent_id, entry_id) -> bool` tracking the most recent vote value per agent per entry per session. When a vote is submitted:
- **No prior vote**: Record the vote and increment the appropriate counter (helpful_count or unhelpful_count).
- **Prior vote with same value**: No-op (no counter changes).
- **Prior vote with different value**: Decrement the old counter (saturating at 0), increment the new counter, and update the recorded vote value. The decrement and increment happen in the same write transaction for atomicity (R-16).

### FR-10: Deduplication Session Scope
The deduplication state (FR-08, FR-09) shall be held in memory only, not persisted. Server restart clears all dedup state. A new server session constitutes a new legitimate access.

### FR-11: Feature-Entry Linking with Trust Gating
When a retrieval tool is called with a `feature` parameter AND the requesting agent's trust level is Internal or higher, the system shall insert `(feature, entry_id)` pairs into the FEATURE_ENTRIES multimap for each returned entry. Duplicate pairs are idempotent (no-ops in redb multimap). When the agent's trust level is Restricted, the `feature` parameter is silently ignored and no FEATURE_ENTRIES writes occur.

### FR-12: Briefing Deduplication
For `context_briefing`, which internally performs multiple retrievals (lookup + search), usage recording shall apply to entries in the final assembled result only. An entry appearing in both the lookup and search results shall be recorded once, not twice.

### FR-13: Tool Parameter Extensions
The system shall add two optional parameters to all four retrieval tools:
- `feature: Option<String>` -- work-context label for FEATURE_ENTRIES tracking
- `helpful: Option<bool>` -- helpfulness signal

Note: `context_briefing` already has a `feature` parameter (added in vnc-003). Only `helpful` is new for that tool.

### FR-14: Store Record Usage Method
The system shall provide `Store::record_usage(all_ids, access_ids, helpful_ids, unhelpful_ids, decrement_helpful_ids, decrement_unhelpful_ids)` that atomically updates multiple entries in a single write transaction:
- For entries in `all_ids`: set `last_accessed_at` to current timestamp
- For entries in `access_ids`: increment `access_count` by 1
- For entries in `helpful_ids`: increment `helpful_count` by 1
- For entries in `unhelpful_ids`: increment `unhelpful_count` by 1
- For entries in `decrement_helpful_ids`: decrement `helpful_count` by 1 (saturating at 0)
- For entries in `decrement_unhelpful_ids`: decrement `unhelpful_count` by 1 (saturating at 0)

### FR-15: Store Record Feature Entries Method
The system shall provide `Store::record_feature_entries(feature, entry_ids)` that inserts (feature, entry_id) pairs into FEATURE_ENTRIES in a single write transaction.

### FR-16: EntryStore Trait Extension
The `EntryStore` trait shall include a `record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError>` method. The `StoreAdapter` implementation delegates to `Store::record_usage` with all IDs as both `all_ids` and `access_ids` (no dedup at trait level, empty slices for vote parameters).

### FR-17: Async Wrapper
`AsyncEntryStore` shall include a `record_access` async wrapper using `spawn_blocking`, matching the pattern of other async wrappers.

### FR-18: Write Rate Tracking Query
The system shall provide `AuditLog::write_count_since(agent_id, since)` that returns the count of write operations (`context_store`, `context_correct`) by the specified agent since the given timestamp. Uses a reverse scan of existing AUDIT_LOG data.

### FR-19: Fire-and-Forget Recording
Usage recording errors (write transaction failures) shall be logged via `tracing::warn!` but shall not propagate to the tool caller. The retrieval result is always returned regardless of usage recording success.

### FR-20: Vote Correction Atomicity
When a vote correction occurs (agent changes their vote on an entry), the decrement of the old counter and the increment of the new counter shall happen within the same write transaction in `Store::record_usage`. This prevents count drift where total votes (helpful + unhelpful) exceed actual voting events.

### FR-21: FEATURE_ENTRIES Trust-Level Check
The system shall check the requesting agent's trust level before writing to FEATURE_ENTRIES. Only agents with trust level Internal or higher (Internal, Privileged, System) shall trigger FEATURE_ENTRIES writes. Restricted agents' `feature` parameters shall be silently ignored without affecting retrieval results.

## Non-Functional Requirements

### NFR-01: Write Transaction Performance
Usage recording adds at most two write transactions per retrieval (one for record_usage, one for record_feature_entries). Each write transaction must complete within 10ms for batches up to 50 entries. At current scale (single-agent stdio), this is well within budget.

### NFR-02: Dedup Memory Footprint
The in-memory dedup structures shall use approximately 72 bytes per unique (agent_id, entry_id) pair (HashMap slightly larger than HashSet). At 10,000 unique pairs, this is approximately 720 KB -- well within a single-session server's memory budget.

### NFR-03: Migration Performance
Schema migration v1->v2 shall complete within 1 second for databases with up to 10,000 entries.

### NFR-04: Backward Compatibility
All existing retrieval tool calls (without `feature` or `helpful` parameters) shall continue to work identically. The new parameters are optional with no default side effects.

### NFR-05: Thread Safety
`UsageDedup` shall be safe to access from multiple async tasks (`Send + Sync` via `Mutex<DedupState>`). At current single-agent concurrency, contention is negligible.

## Acceptance Criteria

| AC-ID | Criterion | Verification Method |
|-------|-----------|-------------------|
| AC-01 | FEATURE_ENTRIES multimap table exists and is created on Store::open() | Unit test: open store, verify table is accessible |
| AC-02 | `helpful_count: u32` and `unhelpful_count: u32` fields added to EntryRecord with serde(default), appended after `trust_source` | Unit test: serialize/deserialize roundtrip with new fields |
| AC-03 | Schema migration v1->v2 runs on Store::open(), backfilling helpful_count=0 and unhelpful_count=0 | Integration test: create v1 database, open with crt-001 code, verify fields |
| AC-04 | access_count is incremented at most once per (agent_id, entry_id) per session | Integration test: retrieve same entry twice, verify access_count=1 |
| AC-05 | last_accessed_at is set to current timestamp on every retrieval (no dedup) | Integration test: retrieve same entry twice, verify last_accessed_at updated both times |
| AC-06 | helpful=true increments helpful_count (once per session per agent per entry) | Integration test: retrieve with helpful=true, verify helpful_count=1 |
| AC-07 | helpful=false increments unhelpful_count; helpful=None changes neither | Integration test: three cases, verify counts |
| AC-08 | FEATURE_ENTRIES populated when `feature` param provided AND agent trust >= Internal | Integration test: retrieve with feature as Internal agent, verify multimap entry exists |
| AC-09 | FEATURE_ENTRIES does not create duplicate pairs | Integration test: retrieve twice with same feature, verify single pair |
| AC-10 | context_briefing updates usage once per entry in final result (no double-counting) | Integration test: entry appearing in both lookup and search counted once |
| AC-11 | EntryStore trait extended with record_access method (object-safe, batch) | Compile test: `fn _check(_: &dyn EntryStore) {}` |
| AC-12 | AuditLog::write_count_since returns correct count | Unit test: log events, query, verify count |
| AC-13 | Existing retrieval behavior unchanged | Regression test: existing tool tests pass without modification |
| AC-14 | All usage updates atomic within a single write transaction | Unit test: verify batch update atomicity via Store::record_usage |
| AC-15 | UsageDedup tracks (agent_id, entry_id) per session, not persisted | Unit test: filter_access returns correct subsets; check_votes returns correct VoteActions; state not in redb |
| AC-16 | Vote correction: when an agent changes its vote on an entry within a session, the old counter is decremented (saturating at 0) and the new counter is incremented. Net effect is a vote flip, not inflation. | Integration test: vote helpful=false then helpful=true, verify helpful_count=1, unhelpful_count=0 |
| AC-17 | Restricted agents' `feature` parameters are silently ignored. Only Internal or higher trust levels write to FEATURE_ENTRIES. | Integration test: Restricted agent retrieves with feature param, verify no FEATURE_ENTRIES written; Internal agent retrieves with feature param, verify FEATURE_ENTRIES written |
| AC-18 | All new code has unit tests; integration tests verify end-to-end retrieval-to-usage-update flow including dedup and vote correction behavior | Test count and coverage verification |

## Domain Models

### UsageDedup
In-memory struct held by UnimatrixServer. Contains:
- `access_counted: HashSet<(String, u64)>` -- binary tracking for access dedup
- `vote_recorded: HashMap<(String, u64), bool>` -- maps (agent_id, entry_id) to last vote value for last-vote-wins correction

Session-scoped (not persisted). The HashMap-based vote tracking enables vote correction: when `check_votes` detects a vote change, it returns `CorrectedVote` so the server can arrange the decrement-old + increment-new in a single transaction.

### VoteAction
Enum returned by `UsageDedup::check_votes` for each entry:
- `NewVote` -- first vote for this (agent, entry) pair. Increment the appropriate counter.
- `CorrectedVote` -- agent is changing their vote. Decrement the old counter, increment the new one.
- `NoOp` -- same vote value as before. No counter changes.

### EntryRecord (Extended)
Existing domain entity gaining two new fields:
- `helpful_count: u32` -- times marked helpful by agents (deduped per session, correctable)
- `unhelpful_count: u32` -- times marked unhelpful by agents (deduped per session, correctable)

Existing fields now populated:
- `access_count: u32` -- incremented on retrieval (deduped per session)
- `last_accessed_at: u64` -- updated to current timestamp on every retrieval (no dedup)

### FEATURE_ENTRIES
Multimap table linking feature identifiers (opaque strings) to entry IDs. Populated by any retrieval tool call that includes a `feature` parameter, subject to trust-level gating: only Internal or higher trust level agents trigger writes. The feature string is a portable work-context label -- it could be a feature ID ("crt-001"), sprint name, issue number, or any grouping.

### Write Rate Query
A read-only query on the existing AUDIT_LOG table. Returns count of write operations by a specific agent since a given timestamp. Does not modify any data.

## User Workflows

### Workflow 1: Agent Retrieves Knowledge (Typical)
1. Agent calls `context_search(query="error handling patterns", feature="crt-001")`
2. Server resolves agent identity -> agent_id + trust_level
3. Server executes search (read transaction) -- returns 5 entries
4. Server checks UsageDedup for agent_id + each entry_id
5. Server calls Store::record_usage for non-deduped entries (write transaction)
6. Server checks trust_level >= Internal -> YES -> calls Store::record_feature_entries("crt-001", [5 entry_ids]) (write transaction)
7. Agent receives search results (unchanged format)

### Workflow 2: Agent Provides Helpfulness Feedback
1. Agent calls `context_get(id=42, helpful=true)`
2. Server resolves agent identity -> agent_id + trust_level
3. Server executes get (read transaction) -- returns entry 42
4. Server calls UsageDedup.filter_access: not yet counted -> include in access batch
5. Server calls UsageDedup.check_votes(agent_id, [42], true): no prior vote -> NewVote -> include in helpful batch
6. Server calls Store::record_usage(all_ids=[42], access_ids=[42], helpful_ids=[42], unhelpful_ids=[], decrement_helpful_ids=[], decrement_unhelpful_ids=[])
7. Agent receives entry 42 (unchanged format)

### Workflow 3: Agent Corrects a Vote (Last-Vote-Wins)
1. Agent calls `context_get(id=42, helpful=false)` -- initial assessment: not useful
2. Server records: access_count=1, unhelpful_count=1 for entry 42
3. Agent calls `context_get(id=42, helpful=true)` -- later determines it was useful
4. Server calls UsageDedup.filter_access: already counted -> empty (access_count not incremented again)
5. Server calls UsageDedup.check_votes(agent_id, [42], true): prior vote was false, new is true -> CorrectedVote
6. Server calls Store::record_usage(all_ids=[42], access_ids=[], helpful_ids=[42], unhelpful_ids=[], decrement_helpful_ids=[], decrement_unhelpful_ids=[42])
7. Result: access_count=1, helpful_count=1, unhelpful_count=0 (corrected from 1 to 0)
8. Agent receives entry 42 (unchanged format)

### Workflow 4: Restricted Agent with Feature (Silently Ignored)
1. Restricted agent calls `context_search(query="patterns", feature="crt-001")`
2. Server resolves agent identity -> agent_id + trust_level=Restricted
3. Server executes search normally -- returns entries
4. Server records usage (access_count, helpful/unhelpful counters) normally
5. Server checks trust_level >= Internal -> NO (Restricted) -> skips record_feature_entries
6. Agent receives search results normally (unaware that feature param was ignored)

### Workflow 5: Rate Limiting Check (Future Consumer)
1. Rate limiter calls `audit.write_count_since("suspicious-agent", one_hour_ago)`
2. AuditLog reverse-scans AUDIT_LOG entries from newest to oldest
3. Stops when timestamp < one_hour_ago
4. Returns count of context_store/context_correct operations by that agent
5. Rate limiter decides whether to allow or deny the next write

## Constraints

- **bincode v2 positional encoding**: Fields must be appended after `trust_source`. No reordering.
- **Schema migration required**: v1->v2 scan-and-rewrite for helpful_count and unhelpful_count.
- **Synchronous writes in read path**: Acceptable for single-agent stdio. Async batching layerable later.
- **AuditEvent struct not modified**: Would break deserialization of existing AUDIT_LOG entries.
- **EntryStore trait is object-safe**: record_access takes `&self` and `&[u64]`, no generics.
- **No async in unimatrix-store**: Store methods are synchronous. Async wrappers in unimatrix-core.

## Dependencies

- **unimatrix-store** (modified): schema.rs, migration.rs, write.rs, db.rs
- **unimatrix-core** (modified): traits.rs, adapters.rs, async_wrappers.rs
- **unimatrix-server** (modified): tools.rs, server.rs, audit.rs; new: usage_dedup.rs
- **redb** v3.1.x: MultimapTableDefinition for FEATURE_ENTRIES
- **std::collections::HashSet**: for UsageDedup access tracking
- **std::collections::HashMap**: for UsageDedup vote tracking (last-vote-wins)
- **std::sync::Mutex**: for thread-safe UsageDedup access
- **tracing**: for logging usage recording errors (fire-and-forget)

## NOT in Scope

- Confidence computation (crt-002)
- Rate limiting enforcement policy
- Anomaly detection algorithms
- Co-access pair tracking (crt-004)
- Usage-based staleness metrics in context_status
- UI or CLI for usage analytics (mtx-002)
- Retroactive usage logging for pre-crt-001 retrievals
- Batched/async usage writes
- Separate context_feedback tool
- USAGE_LOG table (dropped -- AUDIT_LOG + EntryRecord fields suffice)
- Modification of AuditEvent struct
- Persistence of dedup state
- Suppression rate anomaly detection (crt-002 -- minimum sample size guards)
