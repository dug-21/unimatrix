# Pseudocode Overview: crt-001 Usage Tracking

## Component Interaction Flow

```
Tool Handler (context_search/lookup/get/briefing)
    |
    v
1. Execute query (READ txn -- unchanged)
    |
    v
2. Format response (unchanged)
    |
    v
3. Audit logging (unchanged)
    |
    v
4. Usage recording (NEW -- fire-and-forget):
    |
    +---> UsageDedup.filter_access(agent_id, entry_ids) -> access_ids
    +---> UsageDedup.check_votes(agent_id, entry_ids, helpful) -> [(id, VoteAction)]
    |
    +---> Partition vote actions into 4 buckets:
    |     helpful_ids, unhelpful_ids, decrement_helpful_ids, decrement_unhelpful_ids
    |
    +---> Store.record_usage(all_ids, access_ids, helpful_ids, unhelpful_ids,
    |                        decrement_helpful_ids, decrement_unhelpful_ids)
    |     [single WRITE txn: last_accessed_at, access_count, helpful/unhelpful counts]
    |
    +---> if feature.is_some() AND trust_level >= Internal:
          Store.record_feature_entries(feature, entry_ids)
          [second WRITE txn: FEATURE_ENTRIES multimap]
```

## Data Flow

```
EntryRecord (existing, extended):
  - access_count: u32        (incremented by record_usage, deduped)
  - last_accessed_at: u64    (set by record_usage, always updated)
  - helpful_count: u32       (NEW, incremented/decremented by record_usage)
  - unhelpful_count: u32     (NEW, incremented/decremented by record_usage)

FEATURE_ENTRIES (new table):
  MultimapTableDefinition<&str, u64>  -- feature -> {entry_ids}

UsageDedup (in-memory, per-session):
  access_counted: HashSet<(String, u64)>    -- prevents access inflation
  vote_recorded: HashMap<(String, u64), bool> -- enables last-vote-wins correction
```

## Implementation Order

1. C1: Schema Extension -- Add fields + table definition (no deps)
2. C2: Schema Migration -- V1EntryRecord + migrate_v1_to_v2 (depends on C1)
3. C3: Store Usage Methods -- record_usage + record_feature_entries (depends on C1)
4. C4: EntryStore Trait Extension -- record_access method (depends on C3)
5. C5: Usage Dedup -- UsageDedup struct (no store deps)
6. C6: Server Integration -- record_usage_for_entries + tool mods (depends on C3, C5)
7. C7: Audit Log Query -- write_count_since (no other crt-001 deps)

## Shared Types

```
// VoteAction (C5, consumed by C6)
enum VoteAction {
    NewVote,
    CorrectedVote,
    NoOp,
}

// V1EntryRecord (C2 only -- migration struct)
struct V1EntryRecord { /* 24 fields matching schema v1 */ }
```

## Cross-Component Contracts

- C1 defines the schema that C2, C3 consume
- C3 provides record_usage() that C4 (trait) and C6 (server) call
- C5 provides dedup decisions that C6 uses to build the 6 ID sets for C3
- C6 ties together C3 + C5 at the server layer
- C7 is independent (reads existing AUDIT_LOG)
