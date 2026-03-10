# C4: Type Renames

## Purpose

Rename fields and types for semantic clarity with serde backward compatibility. All renames use `serde(alias)` for deserialization; new fields use `serde(default)`.

## File: `crates/unimatrix-observe/src/types.rs`

### Change 1: SessionSummary field renames + addition (lines 169-190)

Current struct:
```rust
pub struct SessionSummary {
    ...
    pub knowledge_in: u64,
    pub knowledge_out: u64,
    ...
}
```

New pseudocode:
```
pub struct SessionSummary {
    pub session_id: String,
    pub started_at: u64,
    pub duration_secs: u64,
    pub tool_distribution: HashMap<String, u64>,
    pub top_file_zones: Vec<(String, u64)>,
    pub agents_spawned: Vec<String>,

    #[serde(alias = "knowledge_in")]
    pub knowledge_served: u64,

    #[serde(alias = "knowledge_out")]
    pub knowledge_stored: u64,

    #[serde(default)]
    pub knowledge_curated: u64,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}
```

Update the doc comments:
- `knowledge_served`: "Count of knowledge retrieval tool calls (context_search, context_lookup, context_get)."
- `knowledge_stored`: "Count of knowledge creation tool calls (context_store)."
- `knowledge_curated`: "Count of knowledge curation tool calls (context_correct, context_deprecate, context_quarantine)."

### Change 2: KnowledgeReuse -> FeatureKnowledgeReuse (lines 193-201)

Current struct:
```rust
pub struct KnowledgeReuse {
    pub tier1_reuse_count: u64,
    pub by_category: HashMap<String, u64>,
    pub category_gaps: Vec<String>,
}
```

New pseudocode:
```
/// Feature-scoped knowledge delivery measurement (col-020b).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureKnowledgeReuse {
    /// Total unique entry IDs delivered to agents for this feature.
    #[serde(alias = "tier1_reuse_count")]
    pub delivery_count: u64,

    /// Entries appearing in 2+ distinct sessions (sub-metric of delivery_count).
    #[serde(default)]
    pub cross_session_count: u64,

    /// Delivery counts grouped by entry category.
    pub by_category: HashMap<String, u64>,

    /// Categories with active entries but zero delivery.
    pub category_gaps: Vec<String>,
}
```

### Change 3: RetrospectiveReport field rename (lines 244-246)

Current:
```rust
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knowledge_reuse: Option<KnowledgeReuse>,
```

New:
```
    #[serde(default, skip_serializing_if = "Option::is_none", alias = "knowledge_reuse")]
    pub feature_knowledge_reuse: Option<FeatureKnowledgeReuse>,
```

Note: The `alias` is added to the END of the existing serde attribute list. The `default` and `skip_serializing_if` remain.

## Existing Test Updates

### test_session_summary_serde_roundtrip (line 330)
- Change struct literal: `knowledge_in: 7` -> `knowledge_served: 7`, `knowledge_out: 2` -> `knowledge_stored: 2`, add `knowledge_curated: 0`
- Change assertions: `back.knowledge_in` -> `back.knowledge_served`, `back.knowledge_out` -> `back.knowledge_stored`
- Add: `assert_eq!(back.knowledge_curated, 0);`

### test_session_summary_outcome_none_omitted (line 367)
- Change struct literal: `knowledge_in: 0` -> `knowledge_served: 0`, `knowledge_out: 0` -> `knowledge_stored: 0`, add `knowledge_curated: 0`

### test_knowledge_reuse_serde_roundtrip (line 388)
- Change `KnowledgeReuse` -> `FeatureKnowledgeReuse`
- Change `tier1_reuse_count: 5` -> `delivery_count: 5`, add `cross_session_count: 0`
- Change assertion: `back.tier1_reuse_count` -> `back.delivery_count`
- Add: `assert_eq!(back.cross_session_count, 0);`

### test_retrospective_report_deserialize_pre_col020 (line 423)
- Change assertion: `report.knowledge_reuse` -> `report.feature_knowledge_reuse`

### test_retrospective_report_serialize_none_fields_omitted (line 447)
- Change struct literal: `knowledge_reuse: None` -> `feature_knowledge_reuse: None`
- Update all SessionSummary literals to use renamed fields + add `knowledge_curated: 0`
- Change assertion string: `"knowledge_reuse"` -> `"feature_knowledge_reuse"`

### test_retrospective_report_roundtrip_with_new_fields (line 475)
- Change struct literal SessionSummary: `knowledge_in: 1` -> `knowledge_served: 1`, `knowledge_out: 0` -> `knowledge_stored: 0`, add `knowledge_curated: 0`
- Change `KnowledgeReuse` -> `FeatureKnowledgeReuse`, `tier1_reuse_count: 3` -> `delivery_count: 3`, add `cross_session_count: 0`
- Change `knowledge_reuse: Some(...)` -> `feature_knowledge_reuse: Some(...)`
- Update assertions accordingly

### test_retrospective_report_partial_new_fields (line 535)
- JSON string has `"knowledge_in": 0, "knowledge_out": 0` -- these should still deserialize correctly via alias
- Change assertion: `report.knowledge_reuse` -> `report.feature_knowledge_reuse`

## New Tests

### test_session_summary_deserialize_pre_col020b
```
Deserialize JSON: {"session_id":"s1","started_at":0,"duration_secs":0,
  "tool_distribution":{},"top_file_zones":[],"agents_spawned":[],
  "knowledge_in":5,"knowledge_out":3}
Assert: .knowledge_served == 5  (alias mapping)
Assert: .knowledge_stored == 3  (alias mapping)
Assert: .knowledge_curated == 0 (serde default)
```

### test_feature_knowledge_reuse_deserialize_from_old
```
Deserialize JSON: {"tier1_reuse_count":7,"by_category":{},"category_gaps":[]}
Assert: .delivery_count == 7    (alias mapping)
Assert: .cross_session_count == 0 (serde default)
```

### test_retrospective_report_deserialize_old_knowledge_reuse_field
```
Deserialize JSON with "knowledge_reuse": {"tier1_reuse_count":3,"by_category":{},"category_gaps":[]}
Assert: .feature_knowledge_reuse is Some
Assert: .feature_knowledge_reuse.delivery_count == 3
```

## Error Handling

No error paths. Serde aliases are compile-time annotations. Missing new fields default to 0 via `serde(default)`.
