# C2: Schema Pseudocode

## Purpose

Define all data types and redb table definitions. This is the foundation -- every other component depends on these types.

## Module: schema.rs

### Imports

- serde::{Serialize, Deserialize}
- redb::{TableDefinition, MultimapTableDefinition}

### Status Enum

```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
enum Status {
    Active = 0,
    Deprecated = 1,
    Proposed = 2,
}
```

Implement `TryFrom<u8>` for Status:
- 0 -> Active, 1 -> Deprecated, 2 -> Proposed
- Other values -> StoreError::InvalidStatus(byte)

Implement `Display` for Status: "Active", "Deprecated", "Proposed".

### EntryRecord

```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct EntryRecord {
    id: u64,
    title: String,
    content: String,
    topic: String,
    category: String,
    tags: Vec<String>,
    source: String,
    status: Status,
    #[serde(default)]
    confidence: f32,
    created_at: u64,
    updated_at: u64,
    #[serde(default)]
    last_accessed_at: u64,
    #[serde(default)]
    access_count: u32,
    #[serde(default)]
    supersedes: Option<u64>,
    #[serde(default)]
    superseded_by: Option<u64>,
    #[serde(default)]
    correction_count: u32,
    #[serde(default)]
    embedding_dim: u16,
}
```

All fields with `#[serde(default)]` support zero-migration schema evolution (R4).
EntryRecord derives Serialize + Deserialize (serde), NOT Encode + Decode (bincode-native). This is the W1 alignment constraint.

### NewEntry

```
struct NewEntry {
    title: String,
    content: String,
    topic: String,
    category: String,
    tags: Vec<String>,
    source: String,
    status: Status,
}
```

Excludes engine-assigned fields: id, created_at, updated_at, and all serde(default) fields.

### QueryFilter

```
#[derive(Debug, Clone, Default)]
struct QueryFilter {
    topic: Option<String>,
    category: Option<String>,
    tags: Option<Vec<String>>,
    status: Option<Status>,
    time_range: Option<TimeRange>,
}
```

### TimeRange

```
#[derive(Debug, Clone, Copy)]
struct TimeRange {
    start: u64,  // inclusive
    end: u64,    // inclusive
}
```

### DatabaseConfig

```
#[derive(Debug, Clone)]
struct DatabaseConfig {
    cache_size: usize,  // bytes
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self { cache_size: 64 * 1024 * 1024 }  // 64 MiB
    }
}
```

### Table Definitions (8 constants)

```
const ENTRIES: TableDefinition<u64, &[u8]>
const TOPIC_INDEX: TableDefinition<(&str, u64), ()>
const CATEGORY_INDEX: TableDefinition<(&str, u64), ()>
const TAG_INDEX: MultimapTableDefinition<&str, u64>
const TIME_INDEX: TableDefinition<(u64, u64), ()>
const STATUS_INDEX: TableDefinition<(u8, u64), ()>
const VECTOR_MAP: TableDefinition<u64, u64>
const COUNTERS: TableDefinition<&str, u64>
```

Table names as strings: "entries", "topic_index", "category_index", "tag_index", "time_index", "status_index", "vector_map", "counters".

## Error Handling

- `TryFrom<u8>` for Status returns `StoreError::InvalidStatus` for unknown bytes.
- No panics. No unwrap.

## Key Test Scenarios

- AC-02: EntryRecord round-trip serialization (all field types, edge cases)
- AC-16: Schema evolution (serialize reduced struct, deserialize as full -- WRITE THIS FIRST per W1)
- R3: bincode round-trip fidelity for edge values
- R4: serde(default) verification with bincode v2 standard config
