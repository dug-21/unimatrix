# Implementation Brief: crt-001 Usage Tracking

## Source Documents

| Document | Path |
|----------|------|
| Scope | product/features/crt-001/SCOPE.md |
| Architecture | product/features/crt-001/architecture/ARCHITECTURE.md |
| Specification | product/features/crt-001/specification/SPECIFICATION.md |
| Risk Strategy | product/features/crt-001/RISK-TEST-STRATEGY.md |
| Alignment Report | product/features/crt-001/ALIGNMENT-REPORT.md |

## Component Map

| Component | Pseudocode | Test Plan |
|-----------|-----------|-----------|
| schema-extension | pseudocode/schema-extension.md | test-plan/schema-extension.md |
| schema-migration | pseudocode/schema-migration.md | test-plan/schema-migration.md |
| store-usage | pseudocode/store-usage.md | test-plan/store-usage.md |
| trait-extension | pseudocode/trait-extension.md | test-plan/trait-extension.md |
| usage-dedup | pseudocode/usage-dedup.md | test-plan/usage-dedup.md |
| server-integration | pseudocode/server-integration.md | test-plan/server-integration.md |
| audit-query | pseudocode/audit-query.md | test-plan/audit-query.md |

## Goal

Add usage tracking to Unimatrix's retrieval pipeline so that every knowledge access updates EntryRecord fields (access_count, last_accessed_at, helpful_count, unhelpful_count) with session-based deduplication and last-vote-wins correction, links features to entries via a trust-gated FEATURE_ENTRIES multimap table, and provides write rate tracking queries on the existing AUDIT_LOG. This is the bridge from passive knowledge accumulation to active learning -- downstream features (crt-002 confidence, crt-004 co-access) depend on the gaming-resistant usage signals this feature produces.

## Resolved Decisions

| Decision | Resolution | Source | ADR File |
|----------|-----------|--------|----------|
| Two-transaction retrieval | Read in read txn, usage write in separate write txn. Usage is analytics, not critical. | SCOPE Decision #3 | architecture/ADR-001-two-transaction-retrieval.md |
| Server-layer deduplication with vote correction | Dedup at server layer (UsageDedup), not store layer. Store applies writes unconditionally. Vote tracking uses HashMap with last-vote-wins. | SCOPE Decision #9, #10 | architecture/ADR-002-server-layer-dedup.md |
| Schema migration v1->v2 | Scan-and-rewrite following nxs-004 pattern. V1EntryRecord for legacy deserialization. | SCOPE Constraints | architecture/ADR-003-schema-migration-v1-v2.md |
| Fire-and-forget usage recording | Usage errors logged, not propagated. Tool callers never see usage failures. | SCOPE Decision #3 | architecture/ADR-004-fire-and-forget-usage.md |
| Audit log reverse scan for rate tracking | Reverse iteration of AUDIT_LOG, stop when timestamp < since. No secondary index. | Architecture C7 | architecture/ADR-005-audit-log-scan-rate-tracking.md |
| Vote correction atomicity | Decrement-old + increment-new in the same write transaction via extended record_usage signature. Saturating subtraction prevents underflow. | SCOPE Decision #10 | architecture/ADR-006-vote-correction-atomicity.md |
| FEATURE_ENTRIES trust-level gating | Only Internal+ agents write FEATURE_ENTRIES. Restricted agents' feature params silently ignored. Single enforcement point in record_usage_for_entries. | SCOPE Decision #11 | architecture/ADR-007-feature-entries-trust-gating.md |
| Reuse access_count, no rename | Existing field serves as usage_count. No rename avoids migration complexity. | SCOPE Decision #1 | SCOPE.md |
| Drop USAGE_LOG table | AUDIT_LOG + EntryRecord fields cover all downstream needs. | SCOPE Decision #2 | SCOPE.md |
| Two-counter helpfulness | helpful_count + unhelpful_count enables Wilson score in crt-002. | SCOPE Decision #6 | SCOPE.md |
| last_accessed_at always updated | No dedup on recency signal. Non-gameable input for crt-002. | SCOPE Decision #8 | SCOPE.md |
| context_briefing logs per-entry in final result | One usage update per entry in assembled response, not per internal query. | SCOPE Decision #4 | SCOPE.md |

## Files to Create/Modify

### New Files

| File | Description |
|------|-------------|
| `crates/unimatrix-server/src/usage_dedup.rs` | In-memory session deduplication struct (UsageDedup) with VoteAction enum and HashMap-based vote tracking |

### Modified Files

| File | Description |
|------|-------------|
| `crates/unimatrix-store/src/schema.rs` | Add helpful_count, unhelpful_count to EntryRecord; add FEATURE_ENTRIES table definition |
| `crates/unimatrix-store/src/migration.rs` | Add V1EntryRecord, migrate_v1_to_v2; bump CURRENT_SCHEMA_VERSION to 2 |
| `crates/unimatrix-store/src/write.rs` | Add Store::record_usage() (6-param with decrement support) and Store::record_feature_entries() methods |
| `crates/unimatrix-store/src/db.rs` | Create FEATURE_ENTRIES table on Store::open() (11 total tables) |
| `crates/unimatrix-store/src/lib.rs` | Re-export FEATURE_ENTRIES from schema |
| `crates/unimatrix-core/src/traits.rs` | Add EntryStore::record_access method |
| `crates/unimatrix-core/src/adapters.rs` | Implement record_access on StoreAdapter (delegates with empty vote slices) |
| `crates/unimatrix-core/src/async_wrappers.rs` | Add record_access async wrapper on AsyncEntryStore |
| `crates/unimatrix-server/src/server.rs` | Add usage_dedup field to UnimatrixServer; add record_usage_for_entries() async method (with trust_level param); update new() constructor |
| `crates/unimatrix-server/src/tools.rs` | Add feature+helpful params to 4 retrieval tool structs; add usage recording calls (passing trust_level) after each retrieval |
| `crates/unimatrix-server/src/audit.rs` | Add write_count_since() method; promote deserialize_audit_event to pub(crate) |
| `crates/unimatrix-server/src/validation.rs` | Add validate_feature() and validate_helpful() functions |
| `crates/unimatrix-server/src/lib.rs` | Declare usage_dedup module |

## Data Structures

### EntryRecord (Extended)

```rust
// Appended after trust_source (position 24, 25 -- zero-indexed 23, 24):
#[serde(default)]
pub helpful_count: u32,    // field index 23 (after trust_source at 22)
#[serde(default)]
pub unhelpful_count: u32,  // field index 24
```

### V1EntryRecord (Migration-Only)

```rust
// 24-field struct matching current schema (nxs-004, schema version 1)
struct V1EntryRecord {
    id: u64, title: String, content: String, topic: String,
    category: String, tags: Vec<String>, source: String,
    status: Status, confidence: f32, created_at: u64, updated_at: u64,
    last_accessed_at: u64, access_count: u32,
    supersedes: Option<u64>, superseded_by: Option<u64>,
    correction_count: u32, embedding_dim: u16,
    created_by: String, modified_by: String,
    content_hash: String, previous_hash: String,
    version: u32, feature_cycle: String, trust_source: String,
}
```

### FEATURE_ENTRIES Table

```rust
pub const FEATURE_ENTRIES: MultimapTableDefinition<&str, u64> =
    MultimapTableDefinition::new("feature_entries");
```

### UsageDedup

```rust
/// The action to take for a vote on a specific entry.
pub enum VoteAction {
    /// First vote for this (agent, entry) pair. Increment the appropriate counter.
    NewVote,
    /// Agent is changing their vote. Decrement the old counter, increment the new one.
    CorrectedVote,
    /// Same vote value as before. No-op.
    NoOp,
}

pub struct UsageDedup {
    inner: Mutex<DedupState>,
}

struct DedupState {
    /// (agent_id, entry_id) pairs where access_count has been incremented.
    access_counted: HashSet<(String, u64)>,
    /// (agent_id, entry_id) -> last vote value (true = helpful, false = unhelpful).
    /// Enables last-vote-wins correction.
    vote_recorded: HashMap<(String, u64), bool>,
}
```

## Function Signatures

### Store Methods (unimatrix-store/src/write.rs)

```rust
impl Store {
    /// Record usage with vote correction support.
    /// Decrement slices enable atomic vote flips (R-16).
    pub fn record_usage(
        &self,
        all_ids: &[u64],
        access_ids: &[u64],
        helpful_ids: &[u64],
        unhelpful_ids: &[u64],
        decrement_helpful_ids: &[u64],
        decrement_unhelpful_ids: &[u64],
    ) -> Result<()>;

    pub fn record_feature_entries(
        &self,
        feature: &str,
        entry_ids: &[u64],
    ) -> Result<()>;
}
```

### EntryStore Trait (unimatrix-core/src/traits.rs)

```rust
fn record_access(&self, entry_ids: &[u64]) -> Result<(), CoreError>;
```

### UsageDedup (unimatrix-server/src/usage_dedup.rs)

```rust
impl UsageDedup {
    pub fn new() -> Self;

    /// Returns entry IDs not yet access-counted for this agent.
    pub fn filter_access(&self, agent_id: &str, entry_ids: &[u64]) -> Vec<u64>;

    /// Returns (entry_id, VoteAction) for each entry given a new vote value.
    /// Uses HashMap to track prior votes and detect corrections.
    pub fn check_votes(
        &self,
        agent_id: &str,
        entry_ids: &[u64],
        helpful: bool,
    ) -> Vec<(u64, VoteAction)>;
}
```

### Server Methods (unimatrix-server/src/server.rs)

```rust
impl UnimatrixServer {
    /// Record usage with dedup, vote correction, and trust-gated FEATURE_ENTRIES.
    pub(crate) async fn record_usage_for_entries(
        &self,
        agent_id: &str,
        trust_level: TrustLevel,
        entry_ids: &[u64],
        helpful: Option<bool>,
        feature: Option<&str>,
    ) -> Result<(), ServerError>;
}
```

### AuditLog Query (unimatrix-server/src/audit.rs)

```rust
impl AuditLog {
    pub fn write_count_since(
        &self,
        agent_id: &str,
        since: u64,
    ) -> Result<u64, ServerError>;
}
```

## Constraints

- bincode v2 positional encoding: fields appended after trust_source, no reordering
- Schema migration v1->v2 required (scan-and-rewrite)
- Synchronous writes in read path (acceptable for single-agent stdio)
- AuditEvent struct NOT modified (would break existing AUDIT_LOG deserialization)
- EntryStore trait must remain object-safe (no &mut self, no generics in methods)
- No async in unimatrix-store crate
- `#![forbid(unsafe_code)]` on all crates
- Vote correction decrement uses saturating subtraction (floor at 0)
- FEATURE_ENTRIES writes require agent trust level >= Internal

## Dependencies

- **redb** v3.1.x -- MultimapTableDefinition for FEATURE_ENTRIES
- **bincode** v2 -- serde-compatible path for serialization
- **serde** -- Serialize/Deserialize with `#[serde(default)]`
- **std::collections::HashSet** -- UsageDedup access tracking
- **std::collections::HashMap** -- UsageDedup vote tracking (last-vote-wins)
- **std::sync::Mutex** -- UsageDedup thread safety
- **tracing** -- warn! for fire-and-forget error logging
- **tokio** -- spawn_blocking for async wrappers

## NOT in Scope

- Confidence computation (crt-002)
- Rate limiting enforcement (future security feature)
- Anomaly detection (future crt feature)
- Co-access pair tracking (crt-004)
- Usage-based staleness in context_status
- UI/CLI for usage analytics (mtx-002)
- Retroactive usage logging
- Batched/async usage writes
- Separate context_feedback tool
- USAGE_LOG table
- AuditEvent struct modifications
- Dedup state persistence
- Active suppression detection (deferred to crt-002 minimum sample size guards)

## Alignment Status

**7 PASS, 1 WARN, 0 VARIANCE, 0 FAIL**

- WARN (W1): EntryStore trait extension adds record_access method not in vision's crt-001 description. Accepted -- explicitly requested in approved SCOPE.md Goal #7, follows established trait extension pattern.

No variances requiring human approval.
