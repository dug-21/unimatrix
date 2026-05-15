# Agent Report: vnc-015-agent-5-context-edge

**Agent ID**: vnc-015-agent-5-context-edge
**Component**: Component 8 — context_edge handler (13th MCP tool)
**Feature**: vnc-015

---

## Files Modified

- `crates/unimatrix-server/src/mcp/tools.rs` — EdgeParams struct, context_edge handler, stale_edges wiring, 6 unit tests
- `crates/unimatrix-server/src/mcp/edge_write.rs` — validate_target made pub(crate); removed 3x #[allow(dead_code)] and stale "not yet implemented" comments

---

## Tool Registration

context_edge registered as the 13th MCP tool via `#[tool(name = "context_edge", ...)]` in the `impl UnimatrixServer` block, immediately before context_cycle. Registration is automatic via the rmcp `#[tool]` proc-macro — no separate tool list update required.

---

## Validation Pipeline Step Order (implemented)

1. Capability gate: `Capability::Write` via `self.require_cap`
2. Source fetch: `self.entry_store.get(params.source_id)` — error → SourceNotFound (InvalidInput)
3. Source status: reject `Status::Quarantined` OR `Status::Deprecated` → SourceFrozen (InvalidInput)
4. Self-ref check: `params.source_id == params.target_id` → SelfReferentialEdge; for redirect, also `source_id == new_target_id`
5. new_target_id presence check (R-13): if mode is "add" or "remove" and `new_target_id.is_some()` → UnexpectedNewTargetId (InvalidInput)
6. Edge type resolution: `RelationType::from_str(&params.edge_type)` → UnknownEdgeType (InvalidInput) if None
7. Target validation: for add/remove call `validate_target(target_id)`; for redirect require new_target_id (MissingNewTargetId if absent) then call `validate_target(new_target_id)`; old target_id not validated (DELETE is idempotent)

---

## default_rules() Caller — Final State

**Before (placeholder):**
```rust
let rules = unimatrix_observe::default_rules(history_slice, vec![]);
```

**After (wired):**
```rust
let stale_edge_pairs = self
    .store
    .query_stale_prerequisite_edges_for_cycle(&feature_cycle)
    .await
    .unwrap_or_else(|e| {
        tracing::warn!(...);
        vec![]
    });
let rules = unimatrix_observe::default_rules(history_slice, stale_edge_pairs);
```

`query_stale_prerequisite_edges_for_cycle` already existed in `read.rs` (added by a prior wave agent). The query returns `Vec<(u64, u64)>` of `(source_id, target_id)` pairs for Prerequisite edges whose source entry is Deprecated and belongs to the given feature_cycle via the FEATURE_ENTRIES table.

---

## Tests

**Unit tests added (6):**
- `test_edge_params_deserializes_valid_add` — mode/source/edge/target correct, new_target_id None
- `test_edge_params_deserializes_valid_redirect` — new_target_id = Some(3)
- `test_edge_params_mode_strings_accepted` — "add", "remove", "redirect" all accepted
- `test_edge_params_new_target_id_defaults_to_none` — absent field → None
- `test_edge_params_missing_required_source_id_rejected` — serde error
- `test_edge_params_missing_required_target_id_rejected` — serde error

**Result:** 2985 unit + 46 + 16 + 16 + 7 = 3070 tests, all pass. Zero new failures.

---

## Issues / Deviations

None. Implementation follows pseudocode/context-edge-handler.md exactly, including the revised 7-step pipeline order with Step 5 (new_target_id presence check) before Step 6 (edge type resolution), per the Gate 3a revision noted in the spawn prompt.

One pre-existing intermittent failure observed (`uds::listener::tests::col018_topic_signal_from_feature_id`) during concurrent test run — known issue #303 (pool timeout), passes in isolation and on re-run. Not caused by this change.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned 12 relevant entries; entries #4430 (ADR-009), #4417 (write_graph_edge pattern), #4435 (Phase A inline validation), #1369 (6-step MCP pipeline) were most useful
- Stored: entry #4436 "Functions called from tools.rs into a sibling module (e.g. edge_write.rs) must be pub(crate), not private" via `/uni-store-pattern` — `validate_target` was private in edge_write.rs and required `pub(crate)` before context_edge could compile; this is a non-obvious trap not visible from the pseudocode
