# nan-002: format-types -- Pseudocode

## Purpose

Define shared typed deserialization structs for the JSONL format_version 1 contract in a new `format.rs` module. These types are the single source of truth for the import format and provide compile-time documentation of the export contract (ADR-001).

## File Created

- `crates/unimatrix-server/src/format.rs`

## Module Structure

```
format.rs
  ExportHeader          -- header line deserialization
  ExportRow             -- tagged enum for data lines
  CounterRow            -- counters table
  EntryRow              -- entries table (26 fields)
  EntryTagRow           -- entry_tags table
  CoAccessRow           -- co_access table
  FeatureEntryRow       -- feature_entries table
  OutcomeIndexRow       -- outcome_index table
  AgentRegistryRow      -- agent_registry table
  AuditLogRow           -- audit_log table
```

## ExportHeader

```
#[derive(Deserialize, Debug)]
pub struct ExportHeader {
    pub _header: bool,
    pub schema_version: i64,
    pub exported_at: i64,
    pub entry_count: i64,
    pub format_version: i64,
}
```

All fields required. The `_header` field must be `true` (validated by import pipeline, not by serde).

## ExportRow

```
#[derive(Deserialize, Debug)]
#[serde(tag = "_table")]
pub enum ExportRow {
    #[serde(rename = "counters")]
    Counter(CounterRow),

    #[serde(rename = "entries")]
    Entry(EntryRow),

    #[serde(rename = "entry_tags")]
    EntryTag(EntryTagRow),

    #[serde(rename = "co_access")]
    CoAccess(CoAccessRow),

    #[serde(rename = "feature_entries")]
    FeatureEntry(FeatureEntryRow),

    #[serde(rename = "outcome_index")]
    OutcomeIndex(OutcomeIndexRow),

    #[serde(rename = "agent_registry")]
    AgentRegistry(AgentRegistryRow),

    #[serde(rename = "audit_log")]
    AuditLog(AuditLogRow),
}
```

Uses serde internally-tagged enum. The `_table` field in JSON selects the variant. Unknown `_table` values produce a serde deserialization error (the import pipeline wraps this with the line number for actionable messaging).

## CounterRow

```
#[derive(Deserialize, Debug)]
pub struct CounterRow {
    pub name: String,
    pub value: i64,
}
```

## EntryRow (26 fields -- ground truth from DDL)

```
#[derive(Deserialize, Debug)]
pub struct EntryRow {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub topic: String,
    pub category: String,
    pub source: String,
    pub status: i64,
    pub confidence: f64,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_accessed_at: i64,
    pub access_count: i64,
    pub supersedes: Option<i64>,
    pub superseded_by: Option<i64>,
    pub correction_count: i64,
    pub embedding_dim: i64,
    pub created_by: String,
    pub modified_by: String,
    pub content_hash: String,
    pub previous_hash: String,
    pub version: i64,
    pub feature_cycle: String,
    pub trust_source: String,
    pub helpful_count: i64,
    pub unhelpful_count: i64,
    pub pre_quarantine_status: Option<i64>,
}
```

CRITICAL: This list matches the entries DDL exactly. The Specification's FR-06 erroneously included `allowed_topics`, `allowed_categories`, `target_ids` (those belong to `agent_registry`/`audit_log`). Use this list.

Nullable columns (`supersedes`, `superseded_by`, `pre_quarantine_status`) use `Option<i64>`. JSON `null` maps to `None`.

`confidence` is `f64` matching the DDL `REAL` type. serde_json preserves f64 precision to 15+ significant digits.

## EntryTagRow

```
#[derive(Deserialize, Debug)]
pub struct EntryTagRow {
    pub entry_id: i64,
    pub tag: String,
}
```

## CoAccessRow

```
#[derive(Deserialize, Debug)]
pub struct CoAccessRow {
    pub entry_id_a: i64,
    pub entry_id_b: i64,
    pub count: i64,
    pub last_updated: i64,
}
```

## FeatureEntryRow

```
#[derive(Deserialize, Debug)]
pub struct FeatureEntryRow {
    pub feature_id: String,    // DDL column: feature_id, export JSON key: feature_id
    pub entry_id: i64,
}
```

CRITICAL: The field is `feature_id`, NOT `feature_cycle`. The Architecture's Integration Surface incorrectly names this `feature_cycle`. Verified against DDL (`CREATE TABLE feature_entries (feature_id TEXT NOT NULL, ...)`) and export code (`map.insert("feature_id".into(), ...)`).

## OutcomeIndexRow

```
#[derive(Deserialize, Debug)]
pub struct OutcomeIndexRow {
    pub feature_cycle: String,  // DDL column: feature_cycle, export JSON key: feature_cycle
    pub entry_id: i64,
}
```

This one IS `feature_cycle` -- different table, different column name from `feature_entries`.

## AgentRegistryRow

```
#[derive(Deserialize, Debug)]
pub struct AgentRegistryRow {
    pub agent_id: String,
    pub trust_level: i64,
    pub capabilities: String,              // JSON-in-TEXT, preserved as raw string
    pub allowed_topics: Option<String>,     // JSON-in-TEXT, nullable
    pub allowed_categories: Option<String>, // JSON-in-TEXT, nullable
    pub enrolled_at: i64,
    pub last_seen_at: i64,
    pub active: i64,
}
```

`capabilities` is a JSON array serialized as a string (e.g., `"[\"admin\",\"read\"]"`). Serde deserializes this as a plain `String` -- it must NOT be re-parsed as JSON. Same for `allowed_topics` and `allowed_categories`.

## AuditLogRow

```
#[derive(Deserialize, Debug)]
pub struct AuditLogRow {
    pub event_id: i64,
    pub timestamp: i64,
    pub session_id: String,
    pub agent_id: String,
    pub operation: String,
    pub target_ids: String,    // JSON-in-TEXT, preserved as raw string
    pub outcome: i64,
    pub detail: String,
}
```

`target_ids` is a JSON array serialized as a string. Same treatment as `capabilities`.

## Design Notes

- All structs derive `Deserialize` and `Debug`. No `Serialize` -- export uses `serde_json::Value` (nan-001 ADR-002).
- No `Clone` needed -- rows are consumed once during INSERT.
- Field names match JSON keys exactly (serde default field naming).
- No `#[serde(default)]` on any field -- all fields are required in the export format. Missing fields cause a deserialization error caught by the import pipeline.

## Key Test Scenarios

1. Deserialize a valid entry JSON line with all 26 fields into `EntryRow`. Verify all field values.
2. Deserialize entry with `supersedes: null`, `superseded_by: null`, `pre_quarantine_status: null` -- Option fields are None.
3. Deserialize entry with empty strings for `previous_hash`, `feature_cycle`, `trust_source` -- preserved as empty strings.
4. Deserialize entry with unicode content (CJK, emoji) -- round-trips correctly.
5. Deserialize entry with `confidence: 0.8723456789012345` -- f64 precision preserved.
6. Deserialize entry with `access_count: i64::MAX` -- no overflow.
7. Deserialize `AgentRegistryRow` with `capabilities: "[\"admin\"]"` and `allowed_topics: null` -- string preserved, nullable is None.
8. Deserialize unknown `_table` value produces serde error (not panic).
9. Deserialize `FeatureEntryRow` with JSON key `feature_id` -- field matches.
10. Deserialize `OutcomeIndexRow` with JSON key `feature_cycle` -- field matches.
11. Deserialize `ExportHeader` with all required fields. Missing `_header` or `format_version` produces error.
12. Each of the 8 row types has at least one edge-case deserialization test.
