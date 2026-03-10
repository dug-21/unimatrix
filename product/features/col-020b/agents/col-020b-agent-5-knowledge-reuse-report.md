# Agent Report: col-020b-agent-5-knowledge-reuse

## Component: C5 — Knowledge Reuse Semantics Revision

## Files Modified
- `crates/unimatrix-server/src/mcp/knowledge_reuse.rs`

## Changes Made

### Production Code
1. Updated module doc comment to reflect revised semantics (delivery + cross-session)
2. Renamed `compute_gaps` parameter from `reused_categories` to `delivered_categories`
3. Updated `compute_gaps` doc comment from "zero reuse" to "zero delivery"
4. Rewrote `compute_knowledge_reuse` function:
   - Return type: `FeatureKnowledgeReuse` (import already updated by C7 agent)
   - Step 3 early return: uses `FeatureKnowledgeReuse` with `delivery_count`, `cross_session_count`
   - Step 5: split into 5a (all_entry_ids) and 5b (cross_session_ids with 2+ sessions filter)
   - Step 6: resolves categories for ALL delivered entries (not just cross-session)
   - Step 6b: counts cross-session from resolved entries only
   - Step 7: category gaps based on all deliveries
5. Updated doc comment to describe new semantics

### Test Updates (existing tests)
- All `tier1_reuse_count` references changed to `delivery_count`
- Added `cross_session_count` assertions to all existing tests
- Renamed `test_knowledge_reuse_same_session_excluded` to `test_knowledge_reuse_single_session_not_cross_session` with revised semantics: `delivery_count == 1`, `cross_session_count == 0`

### New Tests (5 added)
1. `test_knowledge_reuse_single_session_delivery` — regression test for #193
2. `test_knowledge_reuse_delivery_vs_cross_session` — verifies delivery_count > cross_session_count
3. `test_knowledge_reuse_by_category_includes_single_session` — by_category reflects all deliveries
4. `test_knowledge_reuse_category_gaps_delivery_based` — gaps based on delivery not cross-session
5. `test_knowledge_reuse_dedup_across_query_and_injection_same_session` — dedup within same session

## Test Results
- 31 passed, 0 failed (all knowledge_reuse tests)
- cargo clippy: no warnings
- cargo fmt: applied

## Issues
- File is 811 lines (exceeds 500-line guideline). Production code is ~160 lines; remainder is tests. Acceptable given single responsibility and test plan requiring inline tests.
