# Gate 3b Report: Code Review Validation

**Feature**: crt-001 Usage Tracking
**Gate**: 3b (Code Review)
**Result**: PASS

## Validation Checklist

### C1: Schema Extension (schema.rs, db.rs, lib.rs)

| Check | Result |
|-------|--------|
| helpful_count: u32 with #[serde(default)] appended after trust_source | PASS |
| unhelpful_count: u32 with #[serde(default)] appended after trust_source | PASS |
| FEATURE_ENTRIES MultimapTableDefinition<&str, u64> defined | PASS |
| db.rs creates FEATURE_ENTRIES table on open (11 total) | PASS |
| lib.rs re-exports FEATURE_ENTRIES | PASS |
| Test helpers (make_test_record) updated | PASS |
| Roundtrip tests updated with new fields | PASS |

### C2: Schema Migration (migration.rs)

| Check | Result |
|-------|--------|
| CURRENT_SCHEMA_VERSION = 2 | PASS |
| V1EntryRecord with 24 fields defined | PASS |
| migrate_v1_to_v2 function follows v0->v1 pattern | PASS |
| Backfills helpful_count=0, unhelpful_count=0 | PASS |
| v0->v1 migration also adds new fields (chain migration) | PASS |
| 5 new migration tests | PASS |

### C3: Store Usage Methods (write.rs)

| Check | Result |
|-------|--------|
| record_usage() accepts 6 params (all_ids, access_ids, helpful_ids, unhelpful_ids, dec_helpful, dec_unhelpful) | PASS |
| Single write transaction for atomicity | PASS |
| HashSet-based membership checks for each category | PASS |
| Saturating subtraction for decrements | PASS |
| all_ids always updates last_accessed_at | PASS |
| access_ids increments access_count | PASS |
| helpful_ids/unhelpful_ids increment respective counters | PASS |
| record_feature_entries() with multimap insert | PASS |
| insert() sets helpful_count=0, unhelpful_count=0 | PASS |

### C4: EntryStore Trait Extension (traits.rs, adapters.rs, async_wrappers.rs)

| Check | Result |
|-------|--------|
| record_access(&self, &[u64]) added to EntryStore trait | PASS |
| StoreAdapter delegates to record_usage with empty vote params | PASS |
| AsyncEntryStore::record_access uses spawn_blocking | PASS |
| Trait remains object-safe (Send + Sync, &self) | PASS |

### C5: Usage Dedup (usage_dedup.rs - new file)

| Check | Result |
|-------|--------|
| VoteAction enum: NewVote, CorrectedVote, NoOp | PASS |
| UsageDedup with Mutex<DedupState> | PASS |
| filter_access: HashSet<(String, u64)> for access dedup | PASS |
| check_votes: HashMap<(String, u64), bool> for vote tracking | PASS |
| Last-vote-wins semantics | PASS |
| 14 unit tests covering R-03 and R-16 | PASS |
| Poison recovery (unwrap_or_else on lock) | PASS |

### C6: Server Integration (server.rs, tools.rs, validation.rs, lib.rs)

| Check | Result |
|-------|--------|
| pub mod usage_dedup in lib.rs | PASS |
| usage_dedup: Arc<UsageDedup> field on UnimatrixServer | PASS |
| Constructor initializes usage_dedup | PASS |
| record_usage_for_entries() async method implemented | PASS |
| Fire-and-forget: tracing::warn! on errors, never propagated | PASS |
| Trust-level gating via matches! macro | PASS |
| feature + helpful params on SearchParams | PASS |
| feature + helpful params on LookupParams | PASS |
| feature + helpful params on GetParams | PASS |
| helpful param on BriefingParams (feature already exists) | PASS |
| validate_feature() and validate_helpful() in validation.rs | PASS |
| Usage recording call in context_search after audit | PASS |
| Usage recording call in context_lookup after audit | PASS |
| Usage recording call in context_get after audit | PASS |
| Usage recording call in context_briefing with deduped IDs | PASS |
| context_briefing collects entry IDs from conventions+duties+context | PASS |
| helpful_count: 0, unhelpful_count: 0 in insert_with_audit EntryRecord | PASS |
| helpful_count: 0, unhelpful_count: 0 in correct_with_audit EntryRecord | PASS |

### C7: Audit Log Query (audit.rs)

| Check | Result |
|-------|--------|
| deserialize_audit_event promoted from #[cfg(test)] to pub(crate) | PASS |
| write_count_since() method on AuditLog | PASS |
| Forward scan of AUDIT_LOG table | PASS |
| Filters by agent_id and timestamp >= since | PASS |
| is_write_operation() checks context_store and context_correct | PASS |

## Architecture Compliance

| ADR | Requirement | Status |
|-----|-------------|--------|
| ADR-001 | Two-transaction retrieval (read then write) | PASS - read via async wrappers, write via spawn_blocking |
| ADR-002 | Server-layer dedup (store is unconditional) | PASS - UsageDedup at server, record_usage applies all |
| ADR-003 | HashMap for vote tracking (last-vote-wins) | PASS - HashMap<(String, u64), bool> |
| ADR-004 | FEATURE_ENTRIES multimap with trust gating | PASS - matches! for TrustLevel check |
| ADR-005 | AuditLog query via forward scan | PASS - iter() with timestamp filter |
| ADR-006 | Fire-and-forget with tracing::warn! | PASS - all usage errors logged, not propagated |
| ADR-007 | Bincode v2 positional: fields appended at end | PASS - helpful_count after trust_source with serde(default) |

## Specification Compliance

| FR | Requirement | Status |
|----|-------------|--------|
| FR-01 | access_count incremented at most once per agent per entry per session | PASS |
| FR-02 | last_accessed_at updated on every retrieval (no dedup) | PASS |
| FR-03 | helpful_count incremented on helpful=true (with dedup) | PASS |
| FR-04 | unhelpful_count incremented on helpful=false (with dedup) | PASS |
| FR-05 | Vote correction: decrement old, increment new | PASS |
| FR-06 | FEATURE_ENTRIES populated with trust-level gating | PASS |
| FR-07 | Restricted agents' feature param silently ignored | PASS |
| FR-08 | Vote correction: saturating subtraction | PASS |
| FR-09 | context_briefing deduplicates entry IDs | PASS |
| FR-10 | record_usage atomic in single write transaction | PASS |

## Compilation and Test Results

- **Build**: Clean compilation, no errors, no warnings (workspace)
- **Tests**: 587 passed, 0 failed, 18 ignored (model-dependent embed tests)
- **New tests**: 14 (usage_dedup) + existing migration tests updated

## Files Modified

- `crates/unimatrix-store/src/schema.rs` - +helpful_count, +unhelpful_count, +FEATURE_ENTRIES
- `crates/unimatrix-store/src/db.rs` - 11 tables, create FEATURE_ENTRIES
- `crates/unimatrix-store/src/lib.rs` - re-export FEATURE_ENTRIES
- `crates/unimatrix-store/src/migration.rs` - v1->v2 migration, V1EntryRecord
- `crates/unimatrix-store/src/write.rs` - record_usage(), record_feature_entries()
- `crates/unimatrix-core/src/traits.rs` - record_access on EntryStore
- `crates/unimatrix-core/src/adapters.rs` - StoreAdapter::record_access
- `crates/unimatrix-core/src/async_wrappers.rs` - AsyncEntryStore::record_access
- `crates/unimatrix-server/src/usage_dedup.rs` - NEW: UsageDedup, VoteAction
- `crates/unimatrix-server/src/lib.rs` - pub mod usage_dedup
- `crates/unimatrix-server/src/server.rs` - usage_dedup field, record_usage_for_entries()
- `crates/unimatrix-server/src/tools.rs` - feature/helpful params, usage recording calls
- `crates/unimatrix-server/src/validation.rs` - validate_feature(), validate_helpful()
- `crates/unimatrix-server/src/audit.rs` - write_count_since(), promoted deserialize
- `crates/unimatrix-server/src/response.rs` - test helper update
