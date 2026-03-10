## ADR-004: Explicit Tool-to-Field Mapping for File Path Extraction

### Context

Session summaries include `top_file_zones` (directories touched) and context reload rate requires knowing which files each session read. File paths are embedded in the `input` JSON field of ObservationRecords, but different tools store them under different field names:

- `Read`, `Edit`, `Write`: `input.file_path`
- `Glob`: `input.path` (the directory to search in)
- `Grep`: `input.path` (the directory/file to search in)

SR-04 warns that missing or unexpected tool schemas could produce silent data loss in file zone and reload metrics.

Three options:

**Option A**: Attempt to extract any string field that looks like a file path (heuristic: starts with `/` or contains `/`). Broad but noisy — may extract non-path strings.

**Option B**: Hardcode a tool-to-field mapping. Known tools extract from known fields. Unknown tools are skipped.

**Option C**: Attempt `input.file_path` first, then `input.path`, then skip. Ordered fallback without per-tool mapping.

### Decision

Option B — explicit tool-to-field mapping. Implemented as a match expression:

```rust
fn extract_file_path(tool: &str, input: &serde_json::Value) -> Option<String> {
    match tool {
        "Read" | "Edit" | "Write" => input.get("file_path")?.as_str().map(String::from),
        "Glob" | "Grep" => input.get("path")?.as_str().map(String::from),
        _ => None,
    }
}
```

For directory zone extraction, the full file path is reduced to its parent directory (up to 3 path components from the workspace root). Example: `/workspaces/unimatrix/crates/unimatrix-store/src/read.rs` becomes `crates/unimatrix-store/src`.

Unknown tools return `None` silently — no logging at debug level for every unknown tool call. The `other` category in tool distribution already captures these.

### Consequences

- **Easier**: Deterministic extraction — no false positives from heuristic matching. Test-friendly: known inputs produce known outputs.
- **Easier**: When Claude Code adds new file-touching tools, the mapping is a single match arm to update. Explicit rather than fragile.
- **Harder**: Tools not in the mapping contribute zero file zone data. This is acceptable because the mapping covers all file-touching tools in the current Claude Code tool set.
- **Neutral**: If a tool changes its input schema, the mapping silently returns None for that tool. This fails safely (undercounting) rather than dangerously (false data).
