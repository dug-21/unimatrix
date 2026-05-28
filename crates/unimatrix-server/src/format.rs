//! Shared typed deserialization structs for JSONL format_version 1-2.
//!
//! These types are the single source of truth for the import format contract.
//! Export continues to use `serde_json::Value` for serialization (nan-001 ADR-002).
//! Import deserializes via these structs, ensuring compile-time safety if the
//! format changes.

use serde::Deserialize;

/// Header line of a JSONL export file.
///
/// Parsed separately from data lines (it lacks a `_table` field).
/// The `_header` field must be `true` -- validated by the import pipeline,
/// not by serde.
#[derive(Deserialize, Debug)]
pub struct ExportHeader {
    pub _header: bool,
    pub schema_version: i64,
    pub exported_at: i64,
    pub entry_count: i64,
    pub format_version: i64,
}

/// Tagged enum for data lines in the JSONL export.
///
/// The `_table` field in JSON selects the variant. Unknown `_table` values
/// produce a serde deserialization error.
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

    #[serde(rename = "graph_edges")]
    GraphEdge(GraphEdgeRow),

    #[serde(rename = "observations")]
    Observation(ObservationRow),

    #[serde(rename = "cycle_events")]
    CycleEvent(CycleEventRow),
}

/// Row from the `counters` table.
#[derive(Deserialize, Debug)]
pub struct CounterRow {
    pub name: String,
    pub value: i64,
}

/// Row from the `entries` table (26 fields matching DDL).
///
/// Nullable columns (`supersedes`, `superseded_by`, `pre_quarantine_status`)
/// use `Option<i64>`. JSON `null` maps to `None`.
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

/// Row from the `entry_tags` table.
#[derive(Deserialize, Debug)]
pub struct EntryTagRow {
    pub entry_id: i64,
    pub tag: String,
}

/// Row from the `co_access` table.
#[derive(Deserialize, Debug)]
pub struct CoAccessRow {
    pub entry_id_a: i64,
    pub entry_id_b: i64,
    pub count: i64,
    pub last_updated: i64,
}

/// Row from the `feature_entries` table.
///
/// Field is `feature_id` per DDL, NOT `feature_cycle`.
#[derive(Deserialize, Debug)]
pub struct FeatureEntryRow {
    pub feature_id: String,
    pub entry_id: i64,
}

/// Row from the `outcome_index` table.
#[derive(Deserialize, Debug)]
pub struct OutcomeIndexRow {
    pub feature_cycle: String,
    pub entry_id: i64,
}

/// Row from the `agent_registry` table.
///
/// `capabilities`, `allowed_topics`, `allowed_categories` are JSON-in-TEXT
/// columns -- deserialized as plain strings, not re-parsed as JSON.
#[derive(Deserialize, Debug)]
pub struct AgentRegistryRow {
    pub agent_id: String,
    pub trust_level: i64,
    pub capabilities: String,
    pub allowed_topics: Option<String>,
    pub allowed_categories: Option<String>,
    pub enrolled_at: i64,
    pub last_seen_at: i64,
    pub active: i64,
}

/// Row from the `audit_log` table.
///
/// `target_ids` is JSON-in-TEXT -- deserialized as a plain string.
#[derive(Deserialize, Debug)]
pub struct AuditLogRow {
    pub event_id: i64,
    pub timestamp: i64,
    pub session_id: String,
    pub agent_id: String,
    pub operation: String,
    pub target_ids: String,
    pub outcome: i64,
    pub detail: String,
}

/// Row from the `graph_edges` table (9 fields -- no `id`, per ADR-005).
///
/// The synthetic AUTOINCREMENT `id` is omitted because it is unreferenced
/// by any other table. SQLite assigns fresh ids on import.
#[derive(Deserialize, Debug)]
pub struct GraphEdgeRow {
    pub source_id: i64,
    pub target_id: i64,
    pub relation_type: String,
    pub weight: f64,
    pub created_at: i64,
    pub created_by: String,
    pub source: String,
    pub bootstrap_only: i64,
    pub metadata: Option<String>,
}

/// Row from the `observations` table (10 fields -- includes `id`, per ADR-006).
///
/// `id` is preserved through export/import because it has watermark/ordering
/// significance.
#[derive(Deserialize, Debug)]
pub struct ObservationRow {
    pub id: i64,
    pub session_id: String,
    pub ts_millis: i64,
    pub hook: String,
    pub tool: Option<String>,
    pub input: Option<String>,
    pub response_size: Option<i64>,
    pub response_snippet: Option<String>,
    pub topic_signal: Option<String>,
    pub phase: Option<String>,
}

/// Row from the `cycle_events` table (9 fields -- excludes `goal_embedding`, per ADR-004).
///
/// `goal_embedding` is a BLOB (bincode Vec<f32>) tied to the ONNX model version.
/// It is excluded from export; the inserter binds NULL on import.
#[derive(Deserialize, Debug)]
pub struct CycleEventRow {
    pub id: i64,
    pub cycle_id: String,
    pub seq: i64,
    pub event_type: String,
    pub phase: Option<String>,
    pub outcome: Option<String>,
    pub next_phase: Option<String>,
    pub timestamp: i64,
    pub goal: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- ExportHeader ---

    #[test]
    fn test_header_deserialize_valid() {
        let json = r#"{"_header":true,"schema_version":11,"exported_at":1741234567,"entry_count":245,"format_version":1}"#;
        let header: ExportHeader = serde_json::from_str(json).unwrap();
        assert!(header._header);
        assert_eq!(header.schema_version, 11);
        assert_eq!(header.exported_at, 1741234567);
        assert_eq!(header.entry_count, 245);
        assert_eq!(header.format_version, 1);
    }

    #[test]
    fn test_header_deserialize_missing_field_errors() {
        let json =
            r#"{"_header":true,"schema_version":11,"exported_at":1741234567,"entry_count":245}"#;
        let result = serde_json::from_str::<ExportHeader>(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("format_version"),
            "error should mention missing field: {err}"
        );
    }

    // --- ExportRow tagged enum ---

    #[test]
    fn test_export_row_counter_dispatch() {
        let json = r#"{"_table":"counters","name":"next_entry_id","value":100}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::Counter(c) => {
                assert_eq!(c.name, "next_entry_id");
                assert_eq!(c.value, 100);
            }
            other => panic!("expected Counter, got {other:?}"),
        }
    }

    #[test]
    fn test_export_row_entry_dispatch() {
        let json = make_entry_json(None);
        let row: ExportRow = serde_json::from_str(&json).unwrap();
        match row {
            ExportRow::Entry(e) => {
                assert_eq!(e.id, 1);
                assert_eq!(e.title, "Test Entry");
                assert_eq!(e.source, "human");
                assert_eq!(e.correction_count, 0);
                assert_eq!(e.embedding_dim, 384);
            }
            other => panic!("expected Entry, got {other:?}"),
        }
    }

    #[test]
    fn test_export_row_unknown_table_errors() {
        let json = r#"{"_table":"unknown_table","foo":"bar"}"#;
        let result = serde_json::from_str::<ExportRow>(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown_table"),
            "error should mention the unknown table name: {err}"
        );
    }

    // --- EntryRow edge cases ---

    #[test]
    fn test_entry_row_null_optionals() {
        let json = make_entry_json(None);
        let entry: EntryRow = serde_json::from_str(&strip_table_field(&json)).unwrap();
        assert!(entry.supersedes.is_none());
        assert!(entry.superseded_by.is_none());
        assert!(entry.pre_quarantine_status.is_none());
    }

    #[test]
    fn test_entry_row_empty_strings() {
        let overrides = serde_json::json!({
            "previous_hash": "",
            "feature_cycle": "",
            "trust_source": ""
        });
        let json = make_entry_json(Some(&overrides));
        let entry: EntryRow = serde_json::from_str(&strip_table_field(&json)).unwrap();
        assert_eq!(entry.previous_hash, "");
        assert_eq!(entry.feature_cycle, "");
        assert_eq!(entry.trust_source, "");
    }

    #[test]
    fn test_entry_row_unicode_content() {
        let overrides = serde_json::json!({
            "title": "\u{4e16}\u{754c}\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}",
            "content": "Emoji: \u{1f600}\u{1f680} Combining: e\u{0301}"
        });
        let json = make_entry_json(Some(&overrides));
        let entry: EntryRow = serde_json::from_str(&strip_table_field(&json)).unwrap();
        assert_eq!(
            entry.title,
            "\u{4e16}\u{754c}\u{3053}\u{3093}\u{306b}\u{3061}\u{306f}"
        );
        assert_eq!(
            entry.content,
            "Emoji: \u{1f600}\u{1f680} Combining: e\u{0301}"
        );
    }

    #[test]
    fn test_entry_row_max_integers() {
        let overrides = serde_json::json!({
            "access_count": i64::MAX,
            "helpful_count": i64::MAX
        });
        let json = make_entry_json(Some(&overrides));
        let entry: EntryRow = serde_json::from_str(&strip_table_field(&json)).unwrap();
        assert_eq!(entry.access_count, i64::MAX);
        assert_eq!(entry.helpful_count, i64::MAX);
    }

    #[test]
    fn test_entry_row_all_26_fields_present() {
        let json = make_entry_json(None);
        let entry: EntryRow = serde_json::from_str(&strip_table_field(&json)).unwrap();
        // Verify every field maps correctly
        assert_eq!(entry.id, 1);
        assert_eq!(entry.title, "Test Entry");
        assert_eq!(entry.content, "Test content body");
        assert_eq!(entry.topic, "testing");
        assert_eq!(entry.category, "pattern");
        assert_eq!(entry.source, "human");
        assert_eq!(entry.status, 0);
        assert!((entry.confidence - 0.85).abs() < f64::EPSILON);
        assert_eq!(entry.created_at, 1741000000);
        assert_eq!(entry.updated_at, 1741000100);
        assert_eq!(entry.last_accessed_at, 1741000200);
        assert_eq!(entry.access_count, 5);
        assert!(entry.supersedes.is_none());
        assert!(entry.superseded_by.is_none());
        assert_eq!(entry.correction_count, 0);
        assert_eq!(entry.embedding_dim, 384);
        assert_eq!(entry.created_by, "human");
        assert_eq!(entry.modified_by, "human");
        assert_eq!(entry.content_hash, "abc123");
        assert_eq!(entry.previous_hash, "def456");
        assert_eq!(entry.version, 1);
        assert_eq!(entry.feature_cycle, "nxs-001");
        assert_eq!(entry.trust_source, "direct");
        assert_eq!(entry.helpful_count, 3);
        assert_eq!(entry.unhelpful_count, 1);
        assert!(entry.pre_quarantine_status.is_none());
    }

    // --- CounterRow ---

    #[test]
    fn test_counter_row_deserialize() {
        let json = r#"{"_table":"counters","name":"schema_version","value":11}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::Counter(c) => {
                assert_eq!(c.name, "schema_version");
                assert_eq!(c.value, 11);
            }
            other => panic!("expected Counter, got {other:?}"),
        }
    }

    // --- EntryTagRow ---

    #[test]
    fn test_entry_tag_row_deserialize() {
        let json = r#"{"_table":"entry_tags","entry_id":1,"tag":"architecture"}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::EntryTag(t) => {
                assert_eq!(t.entry_id, 1);
                assert_eq!(t.tag, "architecture");
            }
            other => panic!("expected EntryTag, got {other:?}"),
        }
    }

    #[test]
    fn test_entry_tag_row_unicode_tag() {
        let json = r#"{"_table":"entry_tags","entry_id":1,"tag":"\u00e9t\u00e9"}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::EntryTag(t) => {
                assert_eq!(t.tag, "\u{00e9}t\u{00e9}");
            }
            other => panic!("expected EntryTag, got {other:?}"),
        }
    }

    // --- CoAccessRow ---

    #[test]
    fn test_co_access_row_deserialize() {
        let json = r#"{"_table":"co_access","entry_id_a":1,"entry_id_b":2,"count":5,"last_updated":1741234567}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::CoAccess(c) => {
                assert_eq!(c.entry_id_a, 1);
                assert_eq!(c.entry_id_b, 2);
                assert_eq!(c.count, 5);
                assert_eq!(c.last_updated, 1741234567);
            }
            other => panic!("expected CoAccess, got {other:?}"),
        }
    }

    // --- FeatureEntryRow ---

    #[test]
    fn test_feature_entry_row_deserialize() {
        let json = r#"{"_table":"feature_entries","feature_id":"crt-005","entry_id":42}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::FeatureEntry(f) => {
                assert_eq!(f.feature_id, "crt-005");
                assert_eq!(f.entry_id, 42);
            }
            other => panic!("expected FeatureEntry, got {other:?}"),
        }
    }

    // --- OutcomeIndexRow ---

    #[test]
    fn test_outcome_index_row_deserialize() {
        let json = r#"{"_table":"outcome_index","feature_cycle":"col-001","entry_id":10}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::OutcomeIndex(o) => {
                assert_eq!(o.feature_cycle, "col-001");
                assert_eq!(o.entry_id, 10);
            }
            other => panic!("expected OutcomeIndex, got {other:?}"),
        }
    }

    // --- AgentRegistryRow ---

    #[test]
    fn test_agent_registry_row_deserialize() {
        let json = r#"{"_table":"agent_registry","agent_id":"system","trust_level":3,"capabilities":"[\"admin\",\"read\"]","allowed_topics":null,"allowed_categories":null,"enrolled_at":1741000000,"last_seen_at":1741000100,"active":1}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::AgentRegistry(a) => {
                assert_eq!(a.agent_id, "system");
                assert_eq!(a.trust_level, 3);
                assert_eq!(a.capabilities, r#"["admin","read"]"#);
                assert!(a.allowed_topics.is_none());
                assert!(a.allowed_categories.is_none());
                assert_eq!(a.enrolled_at, 1741000000);
                assert_eq!(a.last_seen_at, 1741000100);
                assert_eq!(a.active, 1);
            }
            other => panic!("expected AgentRegistry, got {other:?}"),
        }
    }

    #[test]
    fn test_agent_registry_row_with_topics() {
        let json = r#"{"_table":"agent_registry","agent_id":"worker","trust_level":1,"capabilities":"[\"read\"]","allowed_topics":"[\"testing\"]","allowed_categories":"[\"pattern\"]","enrolled_at":1741000000,"last_seen_at":1741000100,"active":1}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::AgentRegistry(a) => {
                assert_eq!(a.allowed_topics.as_deref(), Some(r#"["testing"]"#));
                assert_eq!(a.allowed_categories.as_deref(), Some(r#"["pattern"]"#));
            }
            other => panic!("expected AgentRegistry, got {other:?}"),
        }
    }

    // --- AuditLogRow ---

    #[test]
    fn test_audit_log_row_deserialize() {
        let json = r#"{"_table":"audit_log","event_id":1,"timestamp":1741234567,"session_id":"sess-001","agent_id":"system","operation":"store","target_ids":"[]","outcome":1,"detail":"stored entry 1"}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::AuditLog(a) => {
                assert_eq!(a.event_id, 1);
                assert_eq!(a.timestamp, 1741234567);
                assert_eq!(a.session_id, "sess-001");
                assert_eq!(a.agent_id, "system");
                assert_eq!(a.operation, "store");
                assert_eq!(a.target_ids, "[]");
                assert_eq!(a.outcome, 1);
                assert_eq!(a.detail, "stored entry 1");
            }
            other => panic!("expected AuditLog, got {other:?}"),
        }
    }

    // --- Floating-point fidelity ---

    #[test]
    fn test_entry_row_confidence_precision() {
        let overrides = serde_json::json!({
            "confidence": 0.8723456789012345_f64
        });
        let json = make_entry_json(Some(&overrides));
        let entry: EntryRow = serde_json::from_str(&strip_table_field(&json)).unwrap();

        // Verify f64 precision: re-serialize and check string representation
        let re_serialized = format!("{}", entry.confidence);
        // f64 Display gives enough digits; verify the value round-trips
        let parsed_back: f64 = re_serialized.parse().unwrap();
        assert!(
            (parsed_back - 0.8723456789012345_f64).abs() < 1e-15,
            "precision lost: got {parsed_back}"
        );
    }

    #[test]
    fn test_entry_row_confidence_boundaries() {
        for val in [0.0_f64, 1.0_f64] {
            let overrides = serde_json::json!({ "confidence": val });
            let json = make_entry_json(Some(&overrides));
            let entry: EntryRow = serde_json::from_str(&strip_table_field(&json)).unwrap();
            assert!(
                (entry.confidence - val).abs() < f64::EPSILON,
                "expected {val}, got {}",
                entry.confidence
            );
        }
    }

    // --- Column count guard (R-01) ---
    // This test validates that EntryRow has exactly 26 fields by checking
    // that a JSON object with all 26 fields deserializes successfully and
    // that removing any field causes a deserialization error.

    #[test]
    fn test_entry_row_field_count_matches_ddl() {
        // The canonical list of 26 entry field names (from DDL).
        let field_names = [
            "id",
            "title",
            "content",
            "topic",
            "category",
            "source",
            "status",
            "confidence",
            "created_at",
            "updated_at",
            "last_accessed_at",
            "access_count",
            "supersedes",
            "superseded_by",
            "correction_count",
            "embedding_dim",
            "created_by",
            "modified_by",
            "content_hash",
            "previous_hash",
            "version",
            "feature_cycle",
            "trust_source",
            "helpful_count",
            "unhelpful_count",
            "pre_quarantine_status",
        ];
        assert_eq!(
            field_names.len(),
            26,
            "DDL field list should have 26 entries"
        );

        // Full JSON with all 26 fields must deserialize
        let json = make_entry_json(None);
        let clean_json = strip_table_field(&json);
        assert!(
            serde_json::from_str::<EntryRow>(&clean_json).is_ok(),
            "full 26-field JSON should deserialize"
        );

        // Removing any required (non-Option) field should fail
        for field in &["id", "title", "content", "status", "confidence", "version"] {
            let mut val: serde_json::Value = serde_json::from_str(&clean_json).unwrap();
            val.as_object_mut().unwrap().remove(*field);
            let result = serde_json::from_value::<EntryRow>(val);
            assert!(
                result.is_err(),
                "removing required field '{field}' should cause error"
            );
        }
    }

    // --- GraphEdgeRow ---

    #[test]
    fn test_graph_edge_row_deserialize_all_fields() {
        let json = r#"{"_table":"graph_edges","source_id":1,"target_id":2,"relation_type":"Supports","weight":0.85,"created_at":1700000000,"created_by":"agent-x","source":"runtime","bootstrap_only":0,"metadata":"{}"}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::GraphEdge(r) => {
                assert_eq!(r.source_id, 1);
                assert_eq!(r.target_id, 2);
                assert_eq!(r.relation_type, "Supports");
                assert!((r.weight - 0.85).abs() < f64::EPSILON);
                assert_eq!(r.created_at, 1700000000);
                assert_eq!(r.created_by, "agent-x");
                assert_eq!(r.source, "runtime");
                assert_eq!(r.bootstrap_only, 0);
                assert_eq!(r.metadata, Some("{}".to_string()));
            }
            other => panic!("expected GraphEdge, got {other:?}"),
        }
    }

    #[test]
    fn test_graph_edge_row_nullable_metadata() {
        let json = r#"{"_table":"graph_edges","source_id":1,"target_id":2,"relation_type":"Supports","weight":0.85,"created_at":1700000000,"created_by":"agent-x","source":"runtime","bootstrap_only":0,"metadata":null}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::GraphEdge(r) => {
                assert!(r.metadata.is_none());
            }
            other => panic!("expected GraphEdge, got {other:?}"),
        }
    }

    #[test]
    fn test_graph_edge_weight_f64_precision() {
        let json = r#"{"_table":"graph_edges","source_id":1,"target_id":2,"relation_type":"T","weight":0.7777777777777,"created_at":0,"created_by":"a","source":"s","bootstrap_only":0,"metadata":null}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::GraphEdge(r) => {
                assert_eq!(
                    r.weight.to_bits(),
                    0.7777777777777_f64.to_bits(),
                    "f64 precision must be preserved without f32 truncation"
                );
            }
            other => panic!("expected GraphEdge, got {other:?}"),
        }
    }

    #[test]
    fn test_graph_edge_weight_zero() {
        let json = r#"{"_table":"graph_edges","source_id":1,"target_id":2,"relation_type":"T","weight":0.0,"created_at":0,"created_by":"a","source":"s","bootstrap_only":0,"metadata":null}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::GraphEdge(r) => {
                assert!((r.weight - 0.0).abs() < f64::EPSILON);
            }
            other => panic!("expected GraphEdge, got {other:?}"),
        }
    }

    #[test]
    fn test_graph_edge_i64_max_ids() {
        let json = format!(
            r#"{{"_table":"graph_edges","source_id":{},"target_id":{},"relation_type":"T","weight":1.0,"created_at":0,"created_by":"a","source":"s","bootstrap_only":0,"metadata":null}}"#,
            i64::MAX,
            i64::MAX
        );
        let row: ExportRow = serde_json::from_str(&json).unwrap();
        match row {
            ExportRow::GraphEdge(r) => {
                assert_eq!(r.source_id, i64::MAX);
                assert_eq!(r.target_id, i64::MAX);
            }
            other => panic!("expected GraphEdge, got {other:?}"),
        }
    }

    #[test]
    fn test_graph_edge_unicode_metadata() {
        let json = r#"{"_table":"graph_edges","source_id":1,"target_id":2,"relation_type":"T","weight":1.0,"created_at":0,"created_by":"a","source":"s","bootstrap_only":0,"metadata":"日本語テスト 🚀"}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::GraphEdge(r) => {
                assert_eq!(r.metadata.as_deref(), Some("日本語テスト 🚀"));
            }
            other => panic!("expected GraphEdge, got {other:?}"),
        }
    }

    #[test]
    fn test_graph_edge_row_field_count_guard() {
        // Removing any required field should cause a deserialization error
        let required_fields = [
            "source_id",
            "target_id",
            "relation_type",
            "weight",
            "created_at",
            "created_by",
            "source",
            "bootstrap_only",
        ];
        assert_eq!(
            required_fields.len(),
            8,
            "8 required + 1 optional = 9 total"
        );

        let base = serde_json::json!({
            "_table": "graph_edges",
            "source_id": 1,
            "target_id": 2,
            "relation_type": "Supports",
            "weight": 0.85,
            "created_at": 1700000000,
            "created_by": "agent-x",
            "source": "runtime",
            "bootstrap_only": 0,
            "metadata": null
        });

        // Full JSON must deserialize
        let full_json = serde_json::to_string(&base).unwrap();
        assert!(
            serde_json::from_str::<ExportRow>(&full_json).is_ok(),
            "full 9-field JSON should deserialize"
        );

        for field in &required_fields {
            let mut val = base.clone();
            val.as_object_mut().unwrap().remove(*field);
            let json = serde_json::to_string(&val).unwrap();
            let result = serde_json::from_str::<ExportRow>(&json);
            assert!(
                result.is_err(),
                "removing required field '{field}' should cause error"
            );
        }
    }

    // --- ObservationRow ---

    #[test]
    fn test_observation_row_deserialize_all_fields() {
        let json = r#"{"_table":"observations","id":5,"session_id":"sess-1","ts_millis":1700000000,"hook":"on_tool","tool":"context_store","input":"test input","response_size":1024,"response_snippet":"ok","topic_signal":"testing","phase":"active"}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::Observation(r) => {
                assert_eq!(r.id, 5);
                assert_eq!(r.session_id, "sess-1");
                assert_eq!(r.ts_millis, 1700000000);
                assert_eq!(r.hook, "on_tool");
                assert_eq!(r.tool, Some("context_store".to_string()));
                assert_eq!(r.input, Some("test input".to_string()));
                assert_eq!(r.response_size, Some(1024));
                assert_eq!(r.response_snippet, Some("ok".to_string()));
                assert_eq!(r.topic_signal, Some("testing".to_string()));
                assert_eq!(r.phase, Some("active".to_string()));
            }
            other => panic!("expected Observation, got {other:?}"),
        }
    }

    #[test]
    fn test_observation_row_nullable_fields() {
        let json = r#"{"_table":"observations","id":5,"session_id":"sess-1","ts_millis":1700000000,"hook":"on_tool","tool":null,"input":null,"response_size":null,"response_snippet":null,"topic_signal":null,"phase":null}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::Observation(r) => {
                assert_eq!(r.id, 5);
                assert_eq!(r.session_id, "sess-1");
                assert!(r.tool.is_none());
                assert!(r.input.is_none());
                assert!(r.response_size.is_none());
                assert!(r.response_snippet.is_none());
                assert!(r.topic_signal.is_none());
                assert!(r.phase.is_none());
            }
            other => panic!("expected Observation, got {other:?}"),
        }
    }

    #[test]
    fn test_observation_row_field_count_guard() {
        let required_fields = ["id", "session_id", "ts_millis", "hook"];
        assert_eq!(
            required_fields.len(),
            4,
            "4 required + 6 optional = 10 total"
        );

        let base = serde_json::json!({
            "_table": "observations",
            "id": 5,
            "session_id": "sess-1",
            "ts_millis": 1700000000,
            "hook": "on_tool",
            "tool": null,
            "input": null,
            "response_size": null,
            "response_snippet": null,
            "topic_signal": null,
            "phase": null
        });

        let full_json = serde_json::to_string(&base).unwrap();
        assert!(
            serde_json::from_str::<ExportRow>(&full_json).is_ok(),
            "full 10-field JSON should deserialize"
        );

        for field in &required_fields {
            let mut val = base.clone();
            val.as_object_mut().unwrap().remove(*field);
            let json = serde_json::to_string(&val).unwrap();
            let result = serde_json::from_str::<ExportRow>(&json);
            assert!(
                result.is_err(),
                "removing required field '{field}' should cause error"
            );
        }
    }

    #[test]
    fn test_observation_row_unicode_tool() {
        let json = r#"{"_table":"observations","id":1,"session_id":"s","ts_millis":0,"hook":"h","tool":"日本語ツール","input":null,"response_size":null,"response_snippet":null,"topic_signal":null,"phase":null}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::Observation(r) => {
                assert_eq!(r.tool.as_deref(), Some("日本語ツール"));
            }
            other => panic!("expected Observation, got {other:?}"),
        }
    }

    // --- CycleEventRow ---

    #[test]
    fn test_cycle_event_row_deserialize_all_fields() {
        let json = r#"{"_table":"cycle_events","id":10,"cycle_id":"nxs-012","seq":1,"event_type":"cycle_start","phase":"design","outcome":"complete","next_phase":"delivery","timestamp":1700000000,"goal":"extend export"}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::CycleEvent(r) => {
                assert_eq!(r.id, 10);
                assert_eq!(r.cycle_id, "nxs-012");
                assert_eq!(r.seq, 1);
                assert_eq!(r.event_type, "cycle_start");
                assert_eq!(r.phase, Some("design".to_string()));
                assert_eq!(r.outcome, Some("complete".to_string()));
                assert_eq!(r.next_phase, Some("delivery".to_string()));
                assert_eq!(r.timestamp, 1700000000);
                assert_eq!(r.goal, Some("extend export".to_string()));
            }
            other => panic!("expected CycleEvent, got {other:?}"),
        }
    }

    #[test]
    fn test_cycle_event_row_nullable_fields() {
        let json = r#"{"_table":"cycle_events","id":10,"cycle_id":"nxs-012","seq":1,"event_type":"cycle_start","phase":null,"outcome":null,"next_phase":null,"timestamp":1700000000,"goal":null}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::CycleEvent(r) => {
                assert_eq!(r.id, 10);
                assert!(r.phase.is_none());
                assert!(r.outcome.is_none());
                assert!(r.next_phase.is_none());
                assert!(r.goal.is_none());
            }
            other => panic!("expected CycleEvent, got {other:?}"),
        }
    }

    #[test]
    fn test_cycle_event_row_with_goal_embedding_key() {
        // Per ADR-004: goal_embedding is excluded from export. If a JSON line
        // includes "goal_embedding": null, serde's internally-tagged enum should
        // handle the unknown field gracefully (no deny_unknown_fields).
        let json = r#"{"_table":"cycle_events","id":10,"cycle_id":"nxs-012","seq":1,"event_type":"cycle_start","phase":null,"outcome":null,"next_phase":null,"timestamp":1700000000,"goal":null,"goal_embedding":null}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::CycleEvent(r) => {
                assert_eq!(r.id, 10);
                assert_eq!(r.cycle_id, "nxs-012");
            }
            other => panic!("expected CycleEvent, got {other:?}"),
        }
    }

    #[test]
    fn test_cycle_event_row_field_count_guard() {
        let required_fields = ["id", "cycle_id", "seq", "event_type", "timestamp"];
        assert_eq!(
            required_fields.len(),
            5,
            "5 required + 4 optional = 9 total"
        );

        let base = serde_json::json!({
            "_table": "cycle_events",
            "id": 10,
            "cycle_id": "nxs-012",
            "seq": 1,
            "event_type": "cycle_start",
            "phase": null,
            "outcome": null,
            "next_phase": null,
            "timestamp": 1700000000,
            "goal": null
        });

        let full_json = serde_json::to_string(&base).unwrap();
        assert!(
            serde_json::from_str::<ExportRow>(&full_json).is_ok(),
            "full 9-field JSON should deserialize"
        );

        for field in &required_fields {
            let mut val = base.clone();
            val.as_object_mut().unwrap().remove(*field);
            let json = serde_json::to_string(&val).unwrap();
            let result = serde_json::from_str::<ExportRow>(&json);
            assert!(
                result.is_err(),
                "removing required field '{field}' should cause error"
            );
        }
    }

    #[test]
    fn test_cycle_event_row_unicode_goal() {
        let json = r#"{"_table":"cycle_events","id":1,"cycle_id":"c","seq":0,"event_type":"t","phase":null,"outcome":null,"next_phase":null,"timestamp":0,"goal":"目標 🎯"}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::CycleEvent(r) => {
                assert_eq!(r.goal.as_deref(), Some("目標 🎯"));
            }
            other => panic!("expected CycleEvent, got {other:?}"),
        }
    }

    // --- ExportRow variant dispatch for new types ---

    #[test]
    fn test_export_row_graph_edge_variant() {
        let json = r#"{"_table":"graph_edges","source_id":1,"target_id":2,"relation_type":"Supports","weight":0.85,"created_at":1700000000,"created_by":"agent-x","source":"runtime","bootstrap_only":0,"metadata":null}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::GraphEdge(r) => assert_eq!(r.source_id, 1),
            other => panic!("expected GraphEdge, got {other:?}"),
        }
    }

    #[test]
    fn test_export_row_observation_variant() {
        let json = r#"{"_table":"observations","id":5,"session_id":"sess-1","ts_millis":1700000000,"hook":"on_tool","tool":null,"input":null,"response_size":null,"response_snippet":null,"topic_signal":null,"phase":null}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::Observation(r) => assert_eq!(r.id, 5),
            other => panic!("expected Observation, got {other:?}"),
        }
    }

    #[test]
    fn test_export_row_cycle_event_variant() {
        let json = r#"{"_table":"cycle_events","id":10,"cycle_id":"nxs-012","seq":1,"event_type":"cycle_start","phase":null,"outcome":null,"next_phase":null,"timestamp":1700000000,"goal":null}"#;
        let row: ExportRow = serde_json::from_str(json).unwrap();
        match row {
            ExportRow::CycleEvent(r) => assert_eq!(r.id, 10),
            other => panic!("expected CycleEvent, got {other:?}"),
        }
    }

    // --- Regression: unknown table still errors ---

    #[test]
    fn test_export_row_unknown_table_error() {
        let json = r#"{"_table":"unknown_table","data":1}"#;
        let result = serde_json::from_str::<ExportRow>(json);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("unknown_table"),
            "error should mention the unknown table name: {err}"
        );
    }

    // --- Test helpers ---

    /// Build a full entry JSON string with all 26 fields plus `_table`.
    /// Optional overrides replace default values for specified keys.
    fn make_entry_json(overrides: Option<&serde_json::Value>) -> String {
        let mut base = serde_json::json!({
            "_table": "entries",
            "id": 1,
            "title": "Test Entry",
            "content": "Test content body",
            "topic": "testing",
            "category": "pattern",
            "source": "human",
            "status": 0,
            "confidence": 0.85,
            "created_at": 1741000000,
            "updated_at": 1741000100,
            "last_accessed_at": 1741000200,
            "access_count": 5,
            "supersedes": null,
            "superseded_by": null,
            "correction_count": 0,
            "embedding_dim": 384,
            "created_by": "human",
            "modified_by": "human",
            "content_hash": "abc123",
            "previous_hash": "def456",
            "version": 1,
            "feature_cycle": "nxs-001",
            "trust_source": "direct",
            "helpful_count": 3,
            "unhelpful_count": 1,
            "pre_quarantine_status": null
        });

        if let Some(overrides) = overrides {
            if let Some(obj) = overrides.as_object() {
                for (k, v) in obj {
                    base[k] = v.clone();
                }
            }
        }

        serde_json::to_string(&base).unwrap()
    }

    /// Strip the `_table` field from a JSON string so it can be deserialized
    /// directly as a row struct (without the tagged enum wrapper).
    fn strip_table_field(json: &str) -> String {
        let mut val: serde_json::Value = serde_json::from_str(json).unwrap();
        val.as_object_mut().unwrap().remove("_table");
        serde_json::to_string(&val).unwrap()
    }
}
