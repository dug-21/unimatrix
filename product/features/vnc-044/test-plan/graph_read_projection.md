# Test Plan — `graph_read_projection.rs` (lean node/edge projection)

> Component: `NodeSummary`, `node_summary()`, `GraphSummaryProjection` trait + impls for `SubgraphResponse`, `ChainResult`, `CurrentResponse`, `InverseResponse`, `FilterResponse`.
> Owns R-07 (exact field set, present AND absent), R-03 (per-envelope metadata preservation — projection side), R-10 (tags/confidence/empty fidelity), R-14 (file placement). Consumes `content_preview`/`CONTENT_PREVIEW_BYTES` from verbosity.rs (proven separately).
> Pseudocode: pseudocode/graph_read_projection.md · AC-03 · R-03, R-07, R-10.

## Unit Test Expectations

`#[test]` in `graph_read_projection.rs`. Build `EntryRecord` fixtures with a helper; serialize the projection with `serde_json::to_value` and assert on the resulting `serde_json::Value` object key sets.

### R-07 — exact summary field set (present AND absent keys) — HIGH

The single most important assertion class. Absent-key assertions are **mandatory**, not just present-key.

**Node (`NodeSummary` → JSON):** key set MUST be **exactly**:
```
{ id, title, category, tags, status, confidence, content_preview, content_truncated }
```
- Present-key assertion: all 8 keys present.
- **Absent-key assertion:** assert these are NOT present — `content`, `content_hash`, `previous_hash`, `embedding_dim`, `created_at`, `updated_at`, `created_by`, `modified_by`, `access_count`, and every other `EntryRecord` count/timestamp field. Enumerate them explicitly; do not assert "len == 8" alone (a rename could keep the count and still leak).
- Do both: `assert_eq!(obj.keys().collect::<BTreeSet>(), expected_8)` AND explicit `assert!(!obj.contains_key("content"))` for the highest-value leaks (`content`, `content_hash`).
- `test_node_summary_exact_key_set`, `test_node_summary_omits_content_and_hashes`.

**Edge (projection `Value`) → JSON:** key set MUST be **exactly**:
```
{ source_id, target_id, relation_type, depth }
```
- **Absent-key assertion:** `direction` and `metadata` NOT present (C-2: `EdgeRecord` wire-locked, projection is a separate `Value`, not a filtered `EdgeRecord`).
- `test_edge_summary_exact_key_set`, `test_edge_summary_omits_direction_and_metadata`.

### R-07 — field value correctness

- `status` value is `status_str(entry.status)` → one of `active|deprecated|proposed|quarantined`. Assert with a fixture in each lifecycle state that the string matches; NOT capability delivery status (R-11 framing). `test_node_summary_status_is_lifecycle_string`.
- `id`, `title`, `category` copied verbatim.
- `content_preview` / `content_truncated` come from `content_preview(&entry.content)` — one fixture with >256B content confirms the wiring (full boundary matrix lives in verbosity.md; do not duplicate it here).

### R-03 — per-envelope `to_summary_json` metadata preservation (Critical, projection side)

Each of the five impls maps node bodies to `node_summary` AND preserves its own envelope metadata. One unit test per impl asserting node-body projection + metadata survival:

| Impl | Fixture | Assert nodes projected | Assert metadata preserved |
|------|---------|------------------------|---------------------------|
| `SubgraphResponse` | 2+ nodes, edges, `truncated=true`, `seed_ids=[..]`, `depth_reached=N` | `nodes[]` are 8-field summaries; `edges[]` are 4-field | `truncated`, `seed_ids`, `depth_reached` present + equal to input |
| `ChainResult` | entries + `Truncated` variant | `entries[]` projected | `truncated`/`Truncated` field present + equal |
| `CurrentResponse` | single `entry` | **single** projected node object (NOT an array) — pin the shape | envelope shape unchanged |
| `InverseResponse` | entries + `total_returned=K` | `entries[]` projected | `total_returned == K` |
| `FilterResponse` | entries + `total_returned=K` | `entries[]` projected | `total_returned == K` |

- `current` shape is the trap: `test_current_summary_is_single_node_not_array`.
- Every impl: `test_{mode}_summary_projects_nodes_preserves_metadata`.
- These prove metadata survival **at the projection level**; the through-wire proof is the integration layer (graph_read.md / OVERVIEW harness plan).

### R-10 — empty/boundary fidelity in the lean shape

- Node with empty `content` → `content_preview: ""`, `content_truncated: false`, valid JSON object. `test_node_summary_empty_content`.
- Node with multiple tags → `tags` array fully preserved (order + count) in the projection. `test_node_summary_preserves_all_tags`.
- Node with zero tags → `tags: []` (empty array, present key). `test_node_summary_zero_tags_empty_array`.
- `confidence: f64` → serialized as a JSON number, unmodified. `test_node_summary_confidence_is_number`.

### R-09 — neighbors/path have NO projection impl (accept-and-ignore, projection side)

- Confirm `NeighborsResponse` and `PathResponse` do **not** implement `GraphSummaryProjection` (compile-level: no impl exists; a code-review/inspection assertion, since detail is ignored via the always-full arm in graph_read.rs). No unit test needed beyond the resolver/dispatch tests in graph_read.md; note here so the reviewer confirms the trait was NOT implemented for edge-only envelopes.

## Integration Test Expectations

Through-wire coverage lives in `test_lifecycle.py` / `test_tools.py` (see OVERVIEW harness plan). This component's integration-visible contract:

1. A `detail=summary` subgraph pulled via MCP returns nodes whose JSON key set is exactly the 8-field set and edges the 4-field set — present AND absent keys asserted on the parsed dict (`test_graph_summary_node_field_set`, `test_graph_summary_edge_field_set`). Mirrors R-07 unit tests but proves it survives real serialization + JSON-RPC framing.
2. A stored entry with >256B multibyte content, pulled at `detail=summary`, yields valid-JSON `content_preview` and correct `content_truncated` (the through-wire manifestation of R-01/R-02).
3. Tags hydrated end-to-end: a multi-tag entry's summary carries all tags (guards against `fetch_nodes_batch` tag-hydration regression, SR-01 integration risk).

## Constraints Verified (static / review — R-14, R-06)

- Projection lives in `graph_read_projection.rs` (new module), **NOT** in the 742-line `graph_read_subgraph.rs` (C-7/SR-08). File-size review gate.
- `NodeSummary` is a **distinct type**; no `skip_serializing_if` added to `EntryRecord` (C-3/R-06). Edge summary is a separate `serde_json::Value`, not a mutated `EdgeRecord` (C-2/R-06). Grep/code-review gate — cross-referenced in graph_read.md static gates.

## Edge Cases Owned Here

- `current` single-node (non-`Vec`) projection shape.
- Empty content / zero tags / many tags.
- Subgraph at `truncated` cap — `truncated` flag survives projection.
- Each envelope's distinct metadata field (`seed_ids`/`depth_reached`/`total_returned`/`Truncated`).
