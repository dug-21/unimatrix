## ADR-001: Intermediate Struct for StatusReport JSON Serialization

### Context

StatusReport has 30+ flat fields but the existing JSON output uses nested structures: `correction_chains { entries_with_supersedes, ... }`, `security { trust_source_distribution, ... }`, `co_access { total_pairs, active_pairs, top_clusters: [...] }`, `outcomes { total, by_type, ... }`, `observation { file_count, ... }`. These nesting groups do not exist in the StatusReport domain struct.

Two options:
1. **Restructure StatusReport** to match the JSON nesting (sub-structs as fields). This changes the internal API — StatusService builds StatusReport, and all consumers would need updating.
2. **Intermediate serialization struct** (`StatusReportJson`) that maps StatusReport's flat fields into the nested JSON structure. StatusReport itself gets `#[derive(Serialize)]` for future use but the JSON format branch uses StatusReportJson.

Additionally, some fields are conditional in the JSON output (`contradictions` only when `contradiction_scan_performed`, `embedding_inconsistencies` only when `embedding_check_performed`, `outcomes` only when total > 0). The intermediate struct handles this with `#[serde(skip_serializing_if)]`.

### Decision

Use an intermediate `StatusReportJson` struct for the JSON format branch of `format_status_report()`.

- `StatusReport` gets `#[derive(serde::Serialize)]` (makes the flat representation available)
- `StatusReportJson` is a private struct in `mcp/response/status.rs` that maps StatusReport fields into the nested JSON shape
- `impl From<&StatusReport> for StatusReportJson` performs the mapping
- Conditional fields use `#[serde(skip_serializing_if = "Option::is_none")]`
- Vec-of-tuple fields (`category_distribution: Vec<(String, u64)>`) serialize as JSON objects (key-value maps) via a custom `serialize_as_map` helper, matching the existing `json!` output

The `format_status_report()` JSON branch becomes:
```rust
ResponseFormat::Json => {
    let json_report = StatusReportJson::from(report);
    let json = serde_json::to_string_pretty(&json_report).unwrap_or_default();
    CallToolResult::success(vec![Content::text(json)])
}
```

### Consequences

**Easier**:
- JSON format output is maintained by a struct + Serialize, not 130 lines of `json!` macros
- Adding new StatusReport fields only requires adding them to StatusReportJson (one place)
- Backward compatibility verified by struct shape, not manual inspection of json! blocks

**Harder**:
- Two representations of StatusReport exist (domain struct + JSON struct). Changes to StatusReport require updating StatusReportJson.
- The mapping code (`From<&StatusReport>`) is a new surface area. Must be tested.
- Summary and Markdown format branches remain manual string formatting (only JSON benefits from Serialize).
