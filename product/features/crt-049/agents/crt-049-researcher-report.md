# crt-049 Researcher Report

## SCOPE.md Written

`product/features/crt-049/SCOPE.md`

## Key Findings

### The Signal Gap

`compute_knowledge_reuse_for_sessions` sources from `query_log.result_entry_ids` (search result sets) and `injection_log.entry_id` (hook-injected entries). Neither records `context_get` or `context_lookup` calls — these tools write nothing to `query_log`. Entry #3417 documents this gap explicitly. The result is that the metric labeled "distinct entries served" actually measures "distinct entries that appeared in a search result set or were injected," which overstates breadth (search sets can be large) and misses deliberate targeted reads entirely.

### Explicit Reads Are Already Recorded

`observations.input` stores `tool_input` as a JSON string (stringified in `listener.rs` at the `PreToolUse` arm). For `context_get` and single-ID `context_lookup`, `tool_input` = `{"id": <N>, ...}`, so `json_extract(input, '$.id')` or Rust-side `input["id"].as_u64()` reliably extracts the entry ID. Filter-based `context_lookup` (no `id` field) returns NULL — the correct exclusion. The `observations.phase` column (crt-043) is also present for future Group 10 phase-conditioned work.

### No Second DB Pass Needed

The `attributed: Vec<ObservationRecord>` slice is already in memory at the `context_cycle_review` handler before step 13. `compute_knowledge_reuse_for_sessions` currently ignores it entirely. Threading `attributed` into the function eliminates an extra query and keeps the computation consistent with the rest of the pipeline.

### Tool Name Normalization Is Required

`observations.tool` may be `"context_get"` or `"mcp__unimatrix__context_get"` depending on hook path vs. direct MCP call. `unimatrix_observe::normalize_tool_name()` strips the prefix. Already used in `session_metrics.rs` for identical matching purposes.

### SUMMARY_SCHEMA_VERSION Must Be Bumped

Currently `2` (`cycle_review_index.rs`). Adding `explicit_read_count` to `FeatureKnowledgeReuse` changes the stored JSON format. Must bump to `3`. Pattern #4178 mandates this for any new review-time aggregate field.

### Serde Alias Chain

`delivery_count` carries an existing `#[serde(alias = "tier1_reuse_count")]` alias. Renaming it to `search_exposure_count` requires adding `#[serde(alias = "delivery_count")]` to the chain, or stored `cycle_review_index` rows will fail deserialization silently.

## Proposed Scope Boundaries

**In scope:**
- Add `explicit_read_count` field to `FeatureKnowledgeReuse`
- Rename `delivery_count` → `search_exposure_count` with alias
- Extraction helper in `knowledge_reuse.rs`
- Thread `attributed` into `compute_knowledge_reuse_for_sessions`
- Update `render_knowledge_reuse` rendering
- Bump `SUMMARY_SCHEMA_VERSION` to 3

**Explicitly out of scope:**
- Phase-conditioned category affinity (Group 10, depends on this feature)
- `by_category` expansion to include explicit reads
- `cross_session_count` extension to explicit reads
- Any new DB table or schema migration

## Open Questions for Human

1. **`total_served` semantics** — Should it remain an alias of `search_exposure_count`, or become the union of search exposures + explicit reads? The field appears in the display line "Distinct entries served". The union is more accurate but changes existing behavior.
2. **`by_category` scope** — Keep sourced from search exposures only, or expand to union? Simpler to keep existing for now.
3. **`cross_session_count` scope** — Should explicit reads contribute to cross-session counting? Likely out of scope for crt-049 but worth confirming.

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing -- returned 19 entries; entries #864, #3417, #4178 directly relevant. Entry #3417 confirms query_log gap. Entry #4178 mandates SUMMARY_SCHEMA_VERSION bump pattern. Entry #864 confirms server-side compute boundary (ADR-001).
- Stored: entry #4213 "Extract explicit reads from attributed ObservationRecord slice, not from observations DB query" via /uni-store-pattern
