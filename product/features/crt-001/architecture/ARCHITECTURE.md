# Architecture: crt-001 Usage Tracking

## System Overview

crt-001 adds usage tracking to Unimatrix's existing retrieval pipeline. Every time an agent retrieves knowledge via `context_search`, `context_lookup`, `context_get`, or `context_briefing`, the system now records: which entries were accessed, when, and whether the agent found them helpful. This data flows into EntryRecord fields (`access_count`, `last_accessed_at`, `helpful_count`, `unhelpful_count`) that downstream features (crt-002 confidence, crt-004 co-access) consume.

The feature touches three crates but introduces no new crates:
- **unimatrix-store**: New FEATURE_ENTRIES table, schema migration v1->v2, new `record_usage()` method with decrement support for vote correction
- **unimatrix-core**: EntryStore trait extension, async wrapper for `record_access`
- **unimatrix-server**: UsageDedup module (HashMap-based vote tracking with last-vote-wins correction), tool parameter additions, recording integration with trust-level gating, AuditLog query method

The architecture follows three key principles: (1) deduplication at the server layer, not the store layer -- the store applies writes, the server decides what to write; (2) two-transaction retrieval -- the read is a read transaction, the usage write is a separate write transaction; (3) trust-level gating at the server layer -- the server checks agent trust before writing FEATURE_ENTRIES.

```
Claude Code / MCP Client
        |
        | stdio (JSON-RPC 2.0)
        v
unimatrix-server (binary)
  |-- ServerHandler (rmcp)          -- unchanged
  |-- ToolRouter                    -- EXTENDED: 4 retrieval tools gain feature+helpful params
  |     |-- tools.rs                -- EXTENDED: +2 optional params on 4 existing tools
  |     |-- validation.rs           -- EXTENDED: +validate_feature, +validate_helpful
  |     |-- response.rs             -- unchanged (same response formats)
  |     '-- scanning.rs             -- unchanged
  |
  |-- UnimatrixServer               -- EXTENDED: +record_usage_for_entries() async method (trust-gated)
  |-- UsageDedup (NEW)              -- In-memory session dedup (access + vote tracking with correction)
  |-- AuditLog                      -- EXTENDED: +write_count_since() query method
  |-- AgentRegistry                 -- consumed (identity resolution + trust level check)
  |
  v
unimatrix-core (traits + async wrappers)
  |-- EntryStore trait              -- EXTENDED: +record_access(&self, entry_ids) method
  |-- AsyncEntryStore               -- EXTENDED: +record_access async wrapper
  |         |          |
  v         v          v
store    vector      embed
  |
  |-- schema.rs                     -- EXTENDED: +helpful_count, +unhelpful_count, +FEATURE_ENTRIES
  |-- migration.rs                  -- EXTENDED: +migrate_v1_to_v2
  |-- write.rs                      -- EXTENDED: +record_usage(), +record_feature_entries()
  |-- db.rs                         -- EXTENDED: open creates FEATURE_ENTRIES table (11 total)
```

## Component Breakdown

### C1: Schema Extension (`crates/unimatrix-store/src/schema.rs`) -- EXTENDED

Add two new fields to EntryRecord and define the FEATURE_ENTRIES table.

**EntryRecord additions (appended after `trust_source`):**
```rust
/// Times this entry was marked helpful by agents (deduped per session).
#[serde(default)]
pub helpful_count: u32,
/// Times this entry was marked unhelpful by agents (deduped per session).
#[serde(default)]
pub unhelpful_count: u32,
```

**New table definition:**
```rust
/// Feature-entry multimap: feature_id -> set of entry_ids.
/// Populated when retrieval tools include a `feature` parameter AND agent trust >= Internal.
pub const FEATURE_ENTRIES: MultimapTableDefinition<&str, u64> =
    MultimapTableDefinition::new("feature_entries");
```

This brings the total table count to 11. The table follows the same pattern as TAG_INDEX (multimap, string key, u64 value set).

### C2: Schema Migration (`crates/unimatrix-store/src/migration.rs`) -- EXTENDED

Add v1->v2 migration following the established pattern from nxs-004's v0->v1 migration.

**V1EntryRecord** -- legacy struct matching the current 24-field layout (without `helpful_count` and `unhelpful_count`). Used only for deserialization during migration.

**migrate_v1_to_v2** -- scan all entries, deserialize with V1EntryRecord, construct new EntryRecord with `helpful_count: 0` and `unhelpful_count: 0`, serialize and overwrite. Same pattern as `migrate_v0_to_v1`.

**CURRENT_SCHEMA_VERSION** bumped from 1 to 2.

**migrate_if_needed** chain extended:
```rust
if current_version < 1 { migrate_v0_to_v1(&txn)?; }
if current_version < 2 { migrate_v1_to_v2(&txn)?; }
```

### C3: Store Usage Methods (`crates/unimatrix-store/src/write.rs`) -- EXTENDED

Two new methods on `Store`:

**`record_usage`** -- batch update access_count, last_accessed_at, helpful_count, unhelpful_count for a set of entries in a single write transaction. Supports both increment and decrement operations for vote correction.

```rust
impl Store {
    /// Record usage for a batch of entries in a single write transaction.
    ///
    /// For each entry_id in `all_ids`, updates `last_accessed_at` to `now`.
    /// For each entry_id in `access_ids`, increments `access_count`.
    /// For each entry_id in `helpful_ids`, increments `helpful_count`.
    /// For each entry_id in `unhelpful_ids`, increments `unhelpful_count`.
    /// For each entry_id in `decrement_helpful_ids`, decrements `helpful_count` (saturating at 0).
    /// For each entry_id in `decrement_unhelpful_ids`, decrements `unhelpful_count` (saturating at 0).
    ///
    /// The caller (server layer) determines which IDs appear in which set
    /// based on deduplication and vote correction. The store applies updates unconditionally.
    ///
    /// Vote correction (decrement old + increment new) happens atomically within
    /// the same write transaction, preventing count drift (R-16).
    pub fn record_usage(
        &self,
        all_ids: &[u64],
        access_ids: &[u64],
        helpful_ids: &[u64],
        unhelpful_ids: &[u64],
        decrement_helpful_ids: &[u64],
        decrement_unhelpful_ids: &[u64],
    ) -> Result<()>
```

**`record_feature_entries`** -- batch insert into FEATURE_ENTRIES multimap.

```rust
impl Store {
    /// Link a set of entry IDs to a feature in FEATURE_ENTRIES.
    /// Idempotent: duplicate (feature, entry_id) pairs are no-ops.
    pub fn record_feature_entries(&self, feature: &str, entry_ids: &[u64]) -> Result<()>
```

Both methods use a single write transaction internally. They are called from the server layer after the retrieval read completes.

### C4: EntryStore Trait Extension (`crates/unimatrix-core/src/traits.rs`) -- EXTENDED

Add one new method to the EntryStore trait:

```rust
pub trait EntryStore: Send + Sync {
    // ... existing methods ...

    /// Record access for a batch of entry IDs.
    /// Updates access_count and last_accessed_at for each entry.
    fn record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError>;
}
```

This method is simpler than `record_usage` -- it only handles the access_count/last_accessed_at portion. The full `record_usage` (with helpful/unhelpful and decrement support) is called directly on the raw Store from the server layer, since the server needs to pass dedup decisions and vote correction actions that the trait method cannot express without bloating the trait.

**StoreAdapter** in `crates/unimatrix-core/src/adapters.rs` delegates to `Store::record_usage(entry_ids, entry_ids, &[], &[], &[], &[])` -- all entries get both access_count and last_accessed_at updates (no dedup at the trait level; dedup happens in the server layer before calling the raw store method).

**AsyncEntryStore** in `crates/unimatrix-core/src/async_wrappers.rs` gets a new `record_access` async wrapper using `spawn_blocking`.

### C5: Usage Deduplication (`crates/unimatrix-server/src/usage_dedup.rs`) -- NEW

In-memory session deduplication prevents the same agent from inflating counters by repeatedly retrieving the same entry. Vote tracking uses **last-vote-wins** semantics with a `HashMap` to enable vote correction.

```rust
use std::collections::{HashSet, HashMap};
use std::sync::Mutex;

/// The action to take for a vote on a specific entry.
pub enum VoteAction {
    /// First vote for this (agent, entry) pair. Increment the appropriate counter.
    NewVote,
    /// Agent is changing their vote. Decrement the old counter, increment the new one.
    CorrectedVote,
    /// Same vote value as before, or already voted in this session. No-op.
    NoOp,
}

/// Session-scoped deduplication for usage tracking.
///
/// Tracks (agent_id, entry_id) pairs to ensure:
/// - access_count increments at most once per agent per entry per session
/// - helpful/unhelpful votes use last-vote-wins: an agent can change its vote,
///   and the old counter is decremented while the new counter is incremented
///
/// In-memory only. Cleared on server restart. Not persisted.
pub struct UsageDedup {
    inner: Mutex<DedupState>,
}

struct DedupState {
    /// (agent_id, entry_id) pairs where access_count has been incremented.
    access_counted: HashSet<(String, u64)>,
    /// (agent_id, entry_id) -> last vote value (true = helpful, false = unhelpful).
    /// Tracks the most recent vote per agent per entry. Enables last-vote-wins correction.
    vote_recorded: HashMap<(String, u64), bool>,
}

impl UsageDedup {
    pub fn new() -> Self;

    /// Check which entry IDs should have access_count incremented.
    /// Returns the subset of `entry_ids` not yet counted for this agent.
    /// Marks all returned IDs as counted.
    pub fn filter_access(&self, agent_id: &str, entry_ids: &[u64]) -> Vec<u64>;

    /// Determine the vote action for each entry ID given the new vote value.
    /// Returns a Vec of (entry_id, VoteAction) pairs.
    ///
    /// For each entry_id:
    /// - No prior vote: returns NewVote, records the vote
    /// - Prior vote with same value: returns NoOp
    /// - Prior vote with different value: returns CorrectedVote, updates the recorded vote
    pub fn check_votes(
        &self,
        agent_id: &str,
        entry_ids: &[u64],
        helpful: bool,
    ) -> Vec<(u64, VoteAction)>;
}
```

**Thread safety**: `Mutex<DedupState>` -- low contention in single-agent stdio. The Mutex is held only for HashMap/HashSet lookups and inserts (microseconds).

**Memory**: ~72 bytes per unique (agent_id, entry_id) pair in the HashMap (slightly larger than HashSet due to the bool value). At 1000 unique pairs: ~72 KB. Well within budget for a single-session server.

### C6: Server Integration (`crates/unimatrix-server/src/server.rs` + `tools.rs`) -- EXTENDED

**UnimatrixServer state addition:**
```rust
pub struct UnimatrixServer {
    // ... existing fields ...
    /// Session-scoped usage deduplication.
    pub(crate) usage_dedup: Arc<UsageDedup>,
}
```

**New async method on UnimatrixServer:**
```rust
impl UnimatrixServer {
    /// Record usage for a set of retrieved entries.
    ///
    /// Called after every successful retrieval. Applies dedup, updates
    /// EntryRecord fields, and writes FEATURE_ENTRIES if feature is provided
    /// AND the agent's trust level is Internal or higher.
    ///
    /// For vote correction: if an agent changes its vote on an entry within
    /// a session, the old counter is decremented and the new counter is
    /// incremented in the same write transaction.
    pub(crate) async fn record_usage_for_entries(
        &self,
        agent_id: &str,
        trust_level: TrustLevel,
        entry_ids: &[u64],
        helpful: Option<bool>,
        feature: Option<&str>,
    ) -> Result<(), ServerError>
}
```

This method:
1. Calls `usage_dedup.filter_access(agent_id, entry_ids)` to get non-deduped access IDs
2. If `helpful` is `Some(value)`, calls `usage_dedup.check_votes(agent_id, entry_ids, value)` to get vote actions per entry
3. Partitions vote actions into: `helpful_ids` (NewVote where helpful=true), `unhelpful_ids` (NewVote where helpful=false), `decrement_helpful_ids` (CorrectedVote where old vote was true), `decrement_unhelpful_ids` (CorrectedVote where old vote was false)
4. Calls `Store::record_usage()` via `spawn_blocking` with all six ID sets -- the decrement and increment happen in the same write transaction (R-16 atomicity)
5. If `feature` is `Some(f)` AND `trust_level >= Internal`, calls `Store::record_feature_entries(f, entry_ids)` via `spawn_blocking`. If `trust_level < Internal` (i.e., Restricted), the feature parameter is silently ignored (AC-17)

**Tool parameter additions** (to all 4 retrieval tool param structs):
```rust
/// Feature context for usage tracking. Links returned entries to this feature.
pub feature: Option<String>,
/// Whether the returned entries were helpful (true/false/omit).
pub helpful: Option<bool>,
```

**Tool handler modifications**: After each successful retrieval and response formatting, call `self.record_usage_for_entries(...)` with the resolved agent's trust level. Usage recording errors are logged but do not fail the tool call (usage is analytics, not critical). The trust level is obtained from the agent identity resolution that already happens in each tool handler.

### C7: Audit Log Query (`crates/unimatrix-server/src/audit.rs`) -- EXTENDED

Add a query method for write rate tracking:

```rust
impl AuditLog {
    /// Count write operations by a specific agent since a given timestamp.
    ///
    /// Scans AUDIT_LOG for entries where `agent_id` matches and `operation`
    /// is a write tool (context_store, context_correct) with `timestamp >= since`.
    pub fn write_count_since(&self, agent_id: &str, since: u64) -> Result<u64, ServerError>
}
```

This scans the AUDIT_LOG table (which stores events keyed by monotonic event_id) and deserializes each event to check agent_id, operation, and timestamp. For the current scale (hundreds to low thousands of events per session), a full scan is acceptable. If this becomes a bottleneck at scale, an agent-indexed secondary table can be added in a future feature.

## Component Interactions

### Retrieval Flow (Modified)

```
Agent calls context_search(query, feature="crt-001", helpful=true)
    |
    v
1. Identity resolution (unchanged) -> agent_id + trust_level
2. Capability check (unchanged)
3. Validation (EXTENDED: validate feature + helpful params)
4. Business logic: execute search (READ transaction, unchanged)
5. Response formatting (unchanged)
6. Audit logging (unchanged -- AUDIT_LOG records target_ids as before)
7. Usage recording (NEW):
   a. UsageDedup.filter_access(agent_id, entry_ids) -> non_deduped_access_ids
   b. UsageDedup.check_votes(agent_id, entry_ids, true) -> vote_actions per entry
   c. Partition vote_actions into helpful_ids, decrement_unhelpful_ids (for corrections)
   d. Store.record_usage(
        all_ids = entry_ids,                  // last_accessed_at for all
        access_ids = non_deduped_ids,         // access_count for non-deduped only
        helpful_ids = new_helpful_ids,        // new helpful votes
        unhelpful_ids = [],                   // helpful=true, not false
        decrement_helpful_ids = [],           // no corrections from helpful
        decrement_unhelpful_ids = corrected,  // entries that flipped from unhelpful to helpful
      )  -- single WRITE transaction (atomicity for vote correction)
   e. Trust check: agent trust_level >= Internal?
      YES -> Store.record_feature_entries("crt-001", entry_ids)  -- WRITE transaction
      NO  -> silently skip (Restricted agents don't write FEATURE_ENTRIES)
8. Return response to agent
```

Key: Step 7 happens AFTER the response is formatted and the audit event is logged. Usage recording is fire-and-forget -- errors are logged but don't affect the tool result.

### context_briefing Special Handling

`context_briefing` performs multiple internal retrievals (lookup by role + search by task). The usage recording must apply to the **final assembled result** only, not to intermediate queries. This means:

1. Briefing internally collects entries from lookup and search
2. Deduplicates them (an entry may appear in both lookup and search results)
3. Assembles the final briefing response
4. Records usage for the unique set of entries in the final response

### Transaction Model

```
Retrieval request
    |
    +--> READ transaction (query/get/search)
    |         |
    |         v
    |    Results (Vec<EntryRecord>)
    |
    +--> WRITE transaction 1 (record_usage)
    |    - For each entry: update last_accessed_at (always)
    |    - For non-deduped: increment access_count
    |    - For new votes: increment helpful_count or unhelpful_count
    |    - For corrected votes: decrement old counter + increment new counter (atomic)
    |    - Single txn for the whole batch (R-16 atomicity guarantee)
    |
    +--> WRITE transaction 2 (record_feature_entries) [if feature provided AND trust >= Internal]
         - Insert (feature, entry_id) pairs into multimap
         - Single txn for the whole batch
```

Two separate write transactions (usage + feature_entries) rather than combining them, because:
1. `record_feature_entries` is conditional (only when feature is provided AND agent has sufficient trust)
2. Either can fail independently without affecting the other
3. Both are analytics -- neither failure corrupts knowledge data

### Data Flow for crt-002 (Downstream Consumer)

```
EntryRecord fields populated by crt-001:
    access_count       -> crt-002 usage factor (log-transformed)
    last_accessed_at   -> crt-002 freshness factor (time decay)
    helpful_count      -> crt-002 helpfulness factor (Wilson score numerator)
    unhelpful_count    -> crt-002 helpfulness factor (Wilson score denominator)
    confidence (0.0)   -> crt-002 WRITES this field (crt-001 does NOT touch it)
```

Vote correction ensures these counters accurately reflect agent assessments. Without correction, an early incorrect vote (e.g., `helpful=false` on speculative retrieval) would permanently degrade an entry's Wilson score even if the agent later determined the entry was helpful.

## Technology Decisions

### ADR-001: Two-Transaction Retrieval Pattern
See `architecture/ADR-001-two-transaction-retrieval.md`.

### ADR-002: Server-Layer Deduplication with Vote Correction
See `architecture/ADR-002-server-layer-dedup.md`.

### ADR-003: Schema Migration V1 to V2
See `architecture/ADR-003-schema-migration-v1-v2.md`.

### ADR-004: Fire-and-Forget Usage Recording
See `architecture/ADR-004-fire-and-forget-usage.md`.

### ADR-005: Audit Log Full Scan for Rate Tracking
See `architecture/ADR-005-audit-log-scan-rate-tracking.md`.

### ADR-006: Vote Correction Atomicity
See `architecture/ADR-006-vote-correction-atomicity.md`.

### ADR-007: FEATURE_ENTRIES Trust-Level Gating
See `architecture/ADR-007-feature-entries-trust-gating.md`.

## Integration Points

### Upstream Dependencies (Consumed)
- **unimatrix-store v1 schema** -- current EntryRecord with 24 fields (trust_source is last)
- **unimatrix-store migration infrastructure** -- `migrate_if_needed()`, LegacyEntryRecord pattern
- **unimatrix-store COUNTERS table** -- `schema_version` counter for migration gating
- **unimatrix-server tools.rs** -- existing 4 retrieval tool handlers
- **unimatrix-server audit.rs** -- AuditEvent struct, AUDIT_LOG table
- **unimatrix-server identity.rs** -- `resolve_agent()` for agent_id extraction + trust level
- **unimatrix-server agent_registry.rs** -- TrustLevel enum, agent trust resolution
- **unimatrix-core traits.rs** -- EntryStore trait, StoreAdapter, AsyncEntryStore

### Downstream Consumers (Served)
- **crt-002** -- reads `access_count`, `helpful_count`, `unhelpful_count`, `last_accessed_at` from EntryRecord
- **crt-004** -- reads AUDIT_LOG `target_ids` for co-access derivation; reads FEATURE_ENTRIES for feature grouping
- **col-002** -- reads FEATURE_ENTRIES for feature-scoped entry aggregation
- **col-004** -- reads FEATURE_ENTRIES for feature lifecycle tracking
- **Future rate limiter** -- calls `AuditLog::write_count_since()` for per-agent write counts

## Integration Surface

| Integration Point | Type/Signature | Source |
|-------------------|---------------|--------|
| `EntryRecord::helpful_count` | `pub helpful_count: u32` | `crates/unimatrix-store/src/schema.rs` |
| `EntryRecord::unhelpful_count` | `pub unhelpful_count: u32` | `crates/unimatrix-store/src/schema.rs` |
| `FEATURE_ENTRIES` | `MultimapTableDefinition<&str, u64>` | `crates/unimatrix-store/src/schema.rs` |
| `Store::record_usage()` | `fn record_usage(&self, all_ids: &[u64], access_ids: &[u64], helpful_ids: &[u64], unhelpful_ids: &[u64], decrement_helpful_ids: &[u64], decrement_unhelpful_ids: &[u64]) -> Result<()>` | `crates/unimatrix-store/src/write.rs` |
| `Store::record_feature_entries()` | `fn record_feature_entries(&self, feature: &str, entry_ids: &[u64]) -> Result<()>` | `crates/unimatrix-store/src/write.rs` |
| `EntryStore::record_access()` | `fn record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError>` | `crates/unimatrix-core/src/traits.rs` |
| `VoteAction` | `pub enum VoteAction { NewVote, CorrectedVote, NoOp }` | `crates/unimatrix-server/src/usage_dedup.rs` |
| `UsageDedup::new()` | `pub fn new() -> Self` | `crates/unimatrix-server/src/usage_dedup.rs` |
| `UsageDedup::filter_access()` | `pub fn filter_access(&self, agent_id: &str, entry_ids: &[u64]) -> Vec<u64>` | `crates/unimatrix-server/src/usage_dedup.rs` |
| `UsageDedup::check_votes()` | `pub fn check_votes(&self, agent_id: &str, entry_ids: &[u64], helpful: bool) -> Vec<(u64, VoteAction)>` | `crates/unimatrix-server/src/usage_dedup.rs` |
| `UnimatrixServer::record_usage_for_entries()` | `pub(crate) async fn record_usage_for_entries(&self, agent_id: &str, trust_level: TrustLevel, entry_ids: &[u64], helpful: Option<bool>, feature: Option<&str>) -> Result<(), ServerError>` | `crates/unimatrix-server/src/server.rs` |
| `AuditLog::write_count_since()` | `pub fn write_count_since(&self, agent_id: &str, since: u64) -> Result<u64, ServerError>` | `crates/unimatrix-server/src/audit.rs` |
| `SearchParams::feature` | `pub feature: Option<String>` | `crates/unimatrix-server/src/tools.rs` |
| `SearchParams::helpful` | `pub helpful: Option<bool>` | `crates/unimatrix-server/src/tools.rs` |
| `LookupParams::feature` | `pub feature: Option<String>` | `crates/unimatrix-server/src/tools.rs` |
| `LookupParams::helpful` | `pub helpful: Option<bool>` | `crates/unimatrix-server/src/tools.rs` |
| `GetParams::feature` | `pub feature: Option<String>` | `crates/unimatrix-server/src/tools.rs` |
| `GetParams::helpful` | `pub helpful: Option<bool>` | `crates/unimatrix-server/src/tools.rs` |
| `BriefingParams::helpful` | `pub helpful: Option<bool>` | `crates/unimatrix-server/src/tools.rs` |
| `CURRENT_SCHEMA_VERSION` | `pub(crate) const CURRENT_SCHEMA_VERSION: u64 = 2` | `crates/unimatrix-store/src/migration.rs` |

**Note**: `BriefingParams` already has a `feature` field (added in vnc-003). crt-001 reuses it for FEATURE_ENTRIES tracking -- no new parameter needed on context_briefing for the feature link.
