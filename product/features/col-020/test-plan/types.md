# Test Plan: C2 -- types

Module: `crates/unimatrix-observe/src/types.rs`

## Unit Tests

### SessionSummary serde

#### test_session_summary_serde_roundtrip
- **Input**: Fully populated SessionSummary
- **Assert**: Serialize to JSON -> deserialize -> all fields match original
- **AC**: AC-01 (struct exists and is serializable)

#### test_session_summary_outcome_none_omitted
- **Input**: SessionSummary with outcome = None
- **Assert**: Serialized JSON does not contain "outcome" key (skip_serializing_if)

### KnowledgeReuse serde

#### test_knowledge_reuse_serde_roundtrip
- **Input**: KnowledgeReuse with tier1_reuse_count=5, by_category={"convention": 3, "pattern": 2}, category_gaps=["procedure"]
- **Assert**: Round-trip preserves all fields

### AttributionMetadata serde

#### test_attribution_metadata_serde_roundtrip
- **Input**: AttributionMetadata { attributed_session_count: 7, total_session_count: 10 }
- **Assert**: Round-trip preserves values

### RetrospectiveReport backward compatibility

#### test_retrospective_report_deserialize_pre_col020
- **Input**: JSON string representing a pre-col-020 RetrospectiveReport (no session_summaries, knowledge_reuse, rework_session_count, context_reload_pct, attribution fields)
- **Assert**: Deserializes successfully. All new fields are None.
- **Risks**: R-09
- **AC**: AC-11

#### test_retrospective_report_serialize_none_fields_omitted
- **Input**: RetrospectiveReport with all new fields as None
- **Assert**: Serialized JSON does not contain keys "session_summaries", "knowledge_reuse", "rework_session_count", "context_reload_pct", "attribution"
- **Risks**: R-09
- **AC**: AC-11

#### test_retrospective_report_roundtrip_with_new_fields
- **Input**: RetrospectiveReport with all new fields populated
- **Assert**: Serialize -> deserialize -> all new fields match
- **Risks**: R-09

#### test_retrospective_report_partial_new_fields
- **Input**: JSON with only session_summaries present (other new fields absent)
- **Assert**: Deserializes successfully. session_summaries populated, others None.
- **Risks**: R-09
