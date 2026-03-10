## ADR-003: Serde Alias for Unidirectional (Read-Old-With-New) Compatibility

### Context

col-020b renames several fields on serialized types (`knowledge_in` -> `knowledge_served`, `tier1_reuse_count` -> `delivery_count`, `knowledge_reuse` -> `feature_knowledge_reuse`). SR-01 from the scope risk assessment flags that `#[serde(alias)]` only covers deserialization (reading old data with new types). Serialization always uses the new field name. The question is whether bidirectional compatibility is required.

Analysis of how `RetrospectiveReport` flows through the system:
1. **Produced by:** `context_retrospective` tool handler in `tools.rs`. Serialized to JSON for MCP response.
2. **Consumed by:** LLM agents receiving the MCP response. They parse the JSON to extract metrics.
3. **Cached?** The report is NOT persisted to SQLite or disk. Each `context_retrospective` call recomputes from observation data. There is no stored report that needs to round-trip.
4. **Cross-version scenario:** Could an old agent (compiled against col-020 types) receive a col-020b report? No -- there is only one server binary, and it is always the latest version. The server produces the report and sends it as JSON text; the consumer is an LLM parsing JSON, not a Rust type.

The only deserialization scenario is: old JSON from test fixtures or logs being deserialized with new types. This is the "read-old-with-new" direction, which `serde(alias)` handles.

No `serde(alias)` currently exists in the codebase. This is the first use. No `serde(rename)` exists on any affected field (SR-02 cleared).

### Decision

Use `#[serde(alias = "old_name")]` for all renamed fields. This provides unidirectional backward compatibility: new types can deserialize old JSON. New serialization always uses the new field names.

New fields (`knowledge_curated`, `cross_session_count`) use `#[serde(default)]` so they default to 0 when absent.

Specific annotations:
```rust
// SessionSummary
#[serde(alias = "knowledge_in")]
pub knowledge_served: u64,
#[serde(alias = "knowledge_out")]
pub knowledge_stored: u64,
#[serde(default)]
pub knowledge_curated: u64,

// FeatureKnowledgeReuse (renamed from KnowledgeReuse)
#[serde(alias = "tier1_reuse_count")]
pub delivery_count: u64,
#[serde(default)]
pub cross_session_count: u64,

// RetrospectiveReport
#[serde(default, skip_serializing_if = "Option::is_none", alias = "knowledge_reuse")]
pub feature_knowledge_reuse: Option<FeatureKnowledgeReuse>,
```

Bidirectional compatibility is NOT required because:
- Reports are not persisted
- There is no cross-version consumer scenario
- The only consumer of serialized reports is LLM agents parsing JSON text

A serde test must verify both directions:
1. Old JSON (with `knowledge_in`, `tier1_reuse_count`, `knowledge_reuse`) deserializes correctly
2. New JSON (with `knowledge_served`, `delivery_count`, `feature_knowledge_reuse`) round-trips

### Consequences

- **Easier:** Existing test fixtures and log samples using old field names remain parseable.
- **Easier:** No need for `serde(rename)` which would force the old name in serialized output, confusing future readers.
- **Harder:** If someone tries to deserialize a col-020b-produced report with col-020 types, the renamed fields will be silently dropped (defaulting to 0). This is acceptable because no such consumer exists.
