# Proposal A: Knowledge Oracle -- Database Design

## redb Table Layout

```
Database: ~/.unimatrix/{project_hash}/unimatrix.redb

ENTRIES:          TableDefinition<u64, &[u8]>
  key:   entry_id (auto-increment from COUNTERS)
  value: bincode-serialized EntryRecord

TOPIC_INDEX:      TableDefinition<(&str, u64), ()>
  key:   (topic, entry_id)
  value: unit -- existence is the index
  usage: range scan on topic prefix for context_lookup

CATEGORY_INDEX:   TableDefinition<(&str, u64), ()>
  key:   (category, entry_id)
  value: unit
  usage: range scan on category prefix for context_lookup

TAG_INDEX:        MultimapTableDefinition<&str, u64>
  key:   tag string
  value: set of entry_ids
  usage: intersection of tag sets for multi-tag filter

TIME_INDEX:       TableDefinition<(u64, u64), ()>
  key:   (unix_timestamp_secs, entry_id)
  value: unit
  usage: temporal ordering, staleness detection

STATUS_INDEX:     TableDefinition<(u8, u64), ()>
  key:   (status_enum_byte, entry_id)
  value: unit
  usage: filter by lifecycle status (0=active, 1=deprecated, 2=proposed)

VECTOR_MAP:       TableDefinition<u64, usize>
  key:   entry_id
  value: hnsw_rs data_id (the d_id for this entry's embedding)
  usage: bridge between redb metadata and hnsw_rs vector index

COUNTERS:         TableDefinition<&str, u64>
  keys:  "next_entry_id", "total_active", "total_deprecated"
  usage: ID generation, fast stats without full scans
```

## EntryRecord Schema

```rust
struct EntryRecord {
    id: u64,
    title: String,               // short title (auto-generated if not provided)
    content: String,             // full markdown content
    topic: String,               // primary topic
    category: String,            // knowledge type
    tags: Vec<String>,           // cross-cutting labels
    source: String,              // provenance ("agent:ndp-architect", "user")
    status: Status,              // Active | Deprecated | Proposed
    confidence: f32,             // computed on read, stored as cache
    created_at: u64,             // unix timestamp
    updated_at: u64,             // unix timestamp
    last_accessed_at: u64,       // for freshness decay
    access_count: u32,           // for Wilson score
    supersedes: Option<u64>,     // correction chain: what this replaces
    superseded_by: Option<u64>,  // correction chain: what replaced this
    correction_count: u32,       // times this specific entry was corrected
    embedding_dim: u16,          // 384 for all-MiniLM-L6-v2
}
```

Serialized via `bincode`. Typical entry ~500-2000 bytes. At 100K entries: ~100-200 MB in redb.

## hnsw_rs Vector Index

```
Files: ~/.unimatrix/{project_hash}/vectors.hnsw.data
       ~/.unimatrix/{project_hash}/vectors.hnsw.graph

Config:
  max_nb_connection: 16   (M parameter -- edges per node)
  ef_construction: 200    (build quality -- higher = slower insert, better recall)
  distance: DistDot       (2-3x faster than cosine for pre-normalized vectors)
  dimension: 384          (all-MiniLM-L6-v2 output; v0.1 may use text-embedding-3-small at 384d)

Search params:
  ef_search: 32           (query-time quality -- higher = slower, better recall)
  k: from tool param      (default 5)
  filter: FilterT closure checking redb status + topic/category/tags
```

## Query Model Mapping

The generic `{ topic, category, query, tags }` maps to the backend as follows:

```
context_lookup(topic: "auth", category: "convention", tags: ["jwt"]):
  1. Range scan TOPIC_INDEX for prefix "auth" -> Set A of entry_ids
  2. Range scan CATEGORY_INDEX for prefix "convention" -> Set B
  3. Lookup TAG_INDEX for "jwt" -> Set C
  4. Intersect: A & B & C -> candidate entry_ids
  5. Batch fetch from ENTRIES, filter status=active
  6. Sort by confidence desc, created_at desc
  7. Return (no hnsw_rs involved)

context_search(query: "how to handle auth tokens", topic: "auth"):
  1. Embed query -> 384d vector
  2. If topic filter: scan TOPIC_INDEX "auth" -> Set A of entry_ids
  3. Map entry_ids to hnsw data_ids via VECTOR_MAP -> build FilterT closure
  4. hnsw_rs.search_filter(embedding, k, ef=32, filter_closure)
  5. Map result d_ids back to entry_ids via reverse VECTOR_MAP lookup
  6. Batch fetch metadata from ENTRIES
  7. Assemble response with similarity scores
```

## Confidence / Lifecycle

**Computed on every read** (not stored; the cached value is for context_status only):

```
confidence(entry) =
    base_confidence(entry.source)           // 0.7 agent, 0.85 user, 0.95 correction
  * usage_boost(entry.access_count)         // Wilson lower bound: (p + z^2/2n - z*sqrt(...)) / (1 + z^2/n)
  * freshness(days_since_last_access)       // 2^(-days/90) -- 90-day half-life, floor 0.1
  * correction_penalty(entry.correction_count) // 0.9^correction_count
```

**Status transitions:**
```
Proposed --[first retrieval]--> Active --[90 days unused]--> Active (low confidence)
Active --[context_correct]--> Deprecated (superseded_by set)
Active --[context_deprecate]--> Deprecated (no replacement)
```

No explicit "aging" status. Confidence decay handles staleness continuously. Deprecated entries are excluded from search results by default (filtered at hnsw_rs level and redb level).

## Dedup on Insert

```
context_store() pipeline:
  1. Embed new content -> 384d vector
  2. search_filter(embedding, k=1, ef=32, filter=active_entries_only)
  3. If top result similarity > 0.92:
       return advisory "near-duplicate detected" with existing entry
       (not isError -- let caller decide)
  4. If no near-duplicate or force=true:
       write_txn: insert ENTRIES, all indexes, VECTOR_MAP
       hnsw_rs.insert(embedding, new_data_id)
  5. Return entry_id + confirmation
```

## Periodic Maintenance

hnsw_rs has no deletion API. Deprecated entries accumulate as filtered-out nodes, increasing filter overhead.

**Rebuild trigger:** When `COUNTERS["total_deprecated"] / (total_active + total_deprecated) > 0.30`, the `context_status` response includes a rebuild recommendation. CLI command `unimatrix rebuild-index` performs: dump all active entries, create fresh hnsw_rs index, swap files, reset VECTOR_MAP.

**Persistence strategy:** After every N writes (configurable, default 100), dump hnsw_rs to temp files, rename into place. redb handles its own durability (fsync on commit).
