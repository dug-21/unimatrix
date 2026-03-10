# Test Plan: C4 — Type Renames

**File:** `crates/unimatrix-observe/src/types.rs`
**Risks:** R-02 (serde alias), R-03 (serde default), R-13 (new field names)

## Unit Test Expectations

All tests in `types.rs::tests`.

### test_session_summary_serde_roundtrip (UPDATE EXISTING)

Update field references from `knowledge_in`/`knowledge_out` to `knowledge_served`/`knowledge_stored`. Add `knowledge_curated` field.

```
Arrange: SessionSummary with knowledge_served=7, knowledge_stored=2, knowledge_curated=1
Act:     serialize -> deserialize
Assert:  back.knowledge_served == 7
         back.knowledge_stored == 2
         back.knowledge_curated == 1
```

### test_session_summary_deserialize_pre_col020b (NEW)

Deserialize col-020 era JSON with old field names.

```
Arrange: JSON string with "knowledge_in": 5, "knowledge_out": 3 (no knowledge_curated)
Act:     serde_json::from_str::<SessionSummary>(json)
Assert:  result.knowledge_served == 5   (via serde alias)
         result.knowledge_stored == 3   (via serde alias)
         result.knowledge_curated == 0  (via serde default)
```

### test_session_summary_knowledge_curated_default (NEW)

New field defaults to 0 when absent.

```
Arrange: JSON string with knowledge_served and knowledge_stored but NO knowledge_curated
Act:     serde_json::from_str::<SessionSummary>(json)
Assert:  result.knowledge_curated == 0
```

### test_session_summary_knowledge_curated_present (NEW)

When knowledge_curated is present in JSON, the value is preserved.

```
Arrange: JSON string with "knowledge_curated": 5
Act:     serde_json::from_str::<SessionSummary>(json)
Assert:  result.knowledge_curated == 5
```

### test_knowledge_reuse_serde_roundtrip (UPDATE EXISTING -> RENAME TO test_feature_knowledge_reuse_serde_roundtrip)

Update to `FeatureKnowledgeReuse` type with renamed fields.

```
Arrange: FeatureKnowledgeReuse with delivery_count=5, cross_session_count=2,
         by_category={"convention": 3, "pattern": 2}, category_gaps=["procedure"]
Act:     serialize -> deserialize
Assert:  back.delivery_count == 5
         back.cross_session_count == 2
         back.by_category preserved
         back.category_gaps preserved
```

### test_feature_knowledge_reuse_deserialize_from_old (NEW)

Deserialize col-020 era JSON with `tier1_reuse_count` (no `cross_session_count`).

```
Arrange: JSON with "tier1_reuse_count": 7, "by_category": {...}, "category_gaps": [...]
Act:     serde_json::from_str::<FeatureKnowledgeReuse>(json)
Assert:  result.delivery_count == 7     (via serde alias)
         result.cross_session_count == 0 (via serde default)
```

### test_retrospective_report_deserialize_old_knowledge_reuse_field (NEW)

Deserialize col-020 era JSON where the field is named `knowledge_reuse`.

```
Arrange: JSON with "knowledge_reuse": {"tier1_reuse_count": 3, "by_category": {}, "category_gaps": []}
Act:     serde_json::from_str::<RetrospectiveReport>(json)
Assert:  result.feature_knowledge_reuse.is_some()
         result.feature_knowledge_reuse.unwrap().delivery_count == 3
```

### test_retrospective_report_roundtrip_with_new_fields (UPDATE EXISTING)

Update all field references to new names. Verify serialized JSON contains new field names.

```
Arrange: RetrospectiveReport with feature_knowledge_reuse: Some(FeatureKnowledgeReuse {...})
Act:     serialize -> deserialize
Assert:  round-trip preserves all values
         serialized JSON contains "feature_knowledge_reuse" (not "knowledge_reuse")
         serialized JSON contains "delivery_count" (not "tier1_reuse_count")
         serialized JSON contains "knowledge_served" (not "knowledge_in")
```

### test_retrospective_report_serialize_none_fields_omitted (UPDATE EXISTING)

Update field name from `knowledge_reuse` to `feature_knowledge_reuse` in struct construction and assertion.

```
Arrange: RetrospectiveReport with feature_knowledge_reuse: None
Act:     serialize
Assert:  JSON does not contain "feature_knowledge_reuse"
```

### test_retrospective_report_deserialize_pre_col020 (UPDATE EXISTING)

Update assertion from `.knowledge_reuse` to `.feature_knowledge_reuse`.

```
Assert:  report.feature_knowledge_reuse.is_none()
```

### test_session_summary_outcome_none_omitted (UPDATE EXISTING)

Update struct construction to use new field names.

## Risk Coverage

- R-02: `test_session_summary_deserialize_pre_col020b`, `test_feature_knowledge_reuse_deserialize_from_old`, `test_retrospective_report_deserialize_old_knowledge_reuse_field` each verify alias deserialization for renamed fields.
- R-03: `test_session_summary_knowledge_curated_default` and the deserialization tests for `cross_session_count` verify serde default behavior.
- R-13: `test_retrospective_report_roundtrip_with_new_fields` asserts serialized JSON uses new field names.
