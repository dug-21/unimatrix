# Agent Report: crt-049-agent-7-tools

**Component**: 4 — `compute_knowledge_reuse_for_sessions` in `crates/unimatrix-server/src/mcp/tools.rs`
**Feature**: crt-049 — Knowledge Reuse Metric: Explicit Read Signal

---

## Changes Made

### Files Modified

- `crates/unimatrix-server/src/mcp/tools.rs`

### Summary of Changes

1. **`EXPLICIT_READ_META_CAP: usize = 500`** — added as module-level constant immediately before `compute_knowledge_reuse_for_sessions` (ADR-004).

2. **Signature extended** — added `attributed: &[unimatrix_observe::ObservationRecord]` parameter to `compute_knowledge_reuse_for_sessions`.

3. **Stub replacement** — removed the `explicit_read_ids_empty` / `explicit_read_meta_empty` placeholder variables and their stub comment. Replaced with three real steps:
   - Step A: `extract_explicit_read_ids(attributed)` — in-memory, no DB.
   - Step B: Cardinality cap logic with `tracing::warn!` when `explicit_ids.len() > 500`. Cap applies only to the lookup input; `explicit_read_count` uses the full uncapped set.
   - Step C: `batch_entry_meta_lookup(store, &lookup_ids).await` for explicit read IDs, inserted before the existing ql+inj batch lookup.

4. **Call site updated** — `context_cycle_review` step 13-14 now passes `&attributed` as the fourth argument.

5. **Tests updated**:
   - `test_compute_knowledge_reuse_for_sessions_no_block_on_panic`: updated to pass `&[]` for `attributed`; added `assert_eq!(reuse.explicit_read_count, 0)`.
   - **AC-05** `test_compute_knowledge_reuse_for_sessions_explicit_read_count_from_attributed`: store-backed integration test; verifies `explicit_read_count == 1` for a PreToolUse/context_get observation with integer-form `id`.
   - **AC-05 variant** `test_compute_knowledge_reuse_for_sessions_prefixed_tool_name_normalized`: validates `mcp__unimatrix__` prefix normalization and `Value::String` hook input path.
   - **Structural** `test_explicit_read_meta_cap_constant_exists`: asserts `EXPLICIT_READ_META_CAP == 500`.

---

## Test Results

```
test result: ok. 2854 passed; 0 failed (unimatrix-server unit tests)
test result: ok. 46 passed; 0 failed (infra integration tests)
test result: ok. 16 passed; 0 failed (import integration tests)
test result: ok. 16 passed; 0 failed (mcp integration tests)
test result: ok. 7 passed; 0 failed (pipeline e2e tests)
Total: 2939 passed; 0 failed
```

---

## Issues / Blockers

None. The stubs left by the retrospective agent were clean replacements — no compile errors beyond the expected signature mismatch resolved by this component.

`unimatrix-observe` and `unimatrix-engine` have pre-existing clippy errors (from other agents' Wave 1 changes) that prevent `cargo clippy --workspace`. `unimatrix-server` itself has zero new clippy errors.

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — found ADR #4218 (extract_explicit_read_ids as standalone helper), ADR #4217 (cardinality cap 500, ADR-004), pattern #4213 (extract from attributed slice, not DB), pattern #3442 (chunked batch IN-clause). All applied directly.
- Stored: attempted `uni-store-pattern` for the insertion ordering / module-level constant / ObservationRecord-no-Default triad. MCP tool rejected `tags` parameter with serialization error in skill context — pattern content preserved here for retrospective extraction:
  - **What**: When extending `compute_knowledge_reuse_for_sessions` with a new `batch_entry_meta_lookup` call, insert it BEFORE the existing ql+inj call; place the cap constant at module level (not inside the function body).
  - **Why**: (1) Sequential awaits release the pool connection cleanly when new lookup precedes the existing one. (2) Module-level constants are reachable from test module for `assert_eq!` structural checks — function-body constants are not. (3) `ObservationRecord` has no `Default` derive; `..Default::default()` in test construction fails to compile — all 8 fields must be explicit.
  - **Scope**: `unimatrix-server/src/mcp/tools.rs`, `compute_knowledge_reuse_for_sessions`, any future agent extending this function.
