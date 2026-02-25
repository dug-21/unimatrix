# Pseudocode Overview: crt-004 Co-Access Boosting

## Component Interaction

```
C1 (co-access-storage)           C2 (session-dedup)
  |  CO_ACCESS table               |  UsageDedup extension
  |  CoAccessRecord type            |  filter_co_access_pairs()
  |  read/write methods             |
  v                                 v
C3 (co-access-recording)  <----  uses C1 + C2
  |  Plugs into record_usage_for_entries
  |  generate_pairs() -> dedup -> store.record_co_access_pairs()
  v
C4 (co-access-boost)      <----  uses C1
  |  compute_search_boost()
  |  compute_briefing_boost()
  v
C5 (confidence-extension)  <---- uses C4
  |  Weight redistribution (0.92 stored)
  |  co_access_affinity() (0.08 query-time)
  v
C6 (tool-integration)      <---- uses C3, C4, C5
  |  context_search: step 9c co-access boost
  |  context_briefing: co-access boost
  |  context_status: co-access stats + cleanup
```

## Shared Types

```rust
// unimatrix-store/schema.rs
pub const CO_ACCESS: TableDefinition<(u64, u64), &[u8]>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CoAccessRecord {
    pub count: u32,
    pub last_updated: u64,
}

pub fn co_access_key(a: u64, b: u64) -> (u64, u64);
pub fn serialize_co_access(record: &CoAccessRecord) -> Result<Vec<u8>>;
pub fn deserialize_co_access(bytes: &[u8]) -> Result<CoAccessRecord>;

// unimatrix-server/response.rs
pub struct CoAccessClusterEntry {
    pub entry_id_a: u64,
    pub entry_id_b: u64,
    pub title_a: String,
    pub title_b: String,
    pub count: u32,
    pub last_updated: u64,
}
```

## Data Flow: Recording

```
Tool returns entry_ids [A, B, C]
  -> server.rs::record_usage_for_entries()
     -> Step 5: co-access recording
        -> coaccess::generate_pairs([A,B,C], 10) -> [(A,B), (A,C), (B,C)]
        -> usage_dedup.filter_co_access_pairs(pairs) -> new_pairs
        -> spawn_blocking: store.record_co_access_pairs(new_pairs)
```

## Data Flow: Boosting

```
context_search returns ranked results [(E1, score1), (E2, score2), ...]
  -> Step 9c: co-access boost
     -> anchors = [E1.id, E2.id, E3.id]  (top 3)
     -> result_ids = [E1.id, ..., Ek.id]
     -> boost_map = coaccess::compute_search_boost(anchors, result_ids, store, staleness)
        -> for each anchor: store.get_co_access_partners(anchor, staleness)
        -> for each result in partner set: compute log-transform boost
     -> for each result: final_score = rerank_score + boost_map[id]
     -> re-sort by final_score
```

## Implementation Order

Wave 1: C1 + C2 (parallel, no dependencies)
Wave 2: C3 + C4 (parallel, depend on C1/C2)
Wave 3: C5 (depends on C4 for constants)
Wave 4: C6 (depends on C3, C4, C5)
