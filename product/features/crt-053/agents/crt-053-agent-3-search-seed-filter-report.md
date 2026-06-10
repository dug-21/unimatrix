# crt-053 Agent Report — search-seed-filter

**Agent**: crt-053-agent-3-search-seed-filter
**Component**: `SearchService::search` Phase 0 seed filter

## Files Modified
- `crates/unimatrix-server/src/services/search.rs` — the sole production edit: `.filter(|(e, _)| e.status == Status::Active)` on the `seed_ids` build inside `if self.ppr_expander_enabled`. No import (Status already in scope, search.rs:10). C-01 satisfied.
- `crates/unimatrix-server/src/test_support.rs` — test infrastructure (cumulative): added `TestHarness::new_with_expander(path, ppr_expander_enabled)` (existing `new()` now delegates with `false`), retained `vector_index`/`embed_handle` handles, added `embed_and_index(entry_ids)` to author real HNSW embeddings (the legacy `rebuild_embeddings` in pipeline_e2e.rs is a no-op).
- `crates/unimatrix-server/tests/pipeline_e2e.rs` — behavior-based acceptance tests (AC-01..AC-05 + edge cases).

## Tests
10 new crt-053 tests, all green (16/16 in pipeline_e2e.rs incl. 6 pre-existing):
- `test_seed_filter_excludes_deprecated_only_neighbor` (AC-01) + `_control` (R-04)
- `test_supersession_false_positive_guard` (AC-05) + `_control` (R-04)
- `test_seed_filter_retains_terminal_active_head` (AC-04, R-02 retention)
- `test_off_path_identical_to_baseline` (AC-02, expander OFF → no injection)
- `test_proposed_seed_excluded` (R-12/GATE-05: predicate is `== Active`, not `!= Deprecated`)
- `test_all_seeds_deprecated_no_panic` (empty-seed boundary)
- `test_superseded_but_active_is_retained` (discriminator is status, not superseded_by)

**R-04 control-arm form**: forcing the deprecated seed A to `Status::Active` (no second code path). In each control arm the deprecated-only neighbor (X) reappears once A is an eligible active seed, proving the real-arm absence is filter-caused, not unreachability (#4902).

**Acceptance surface**: Rust full-pipeline harness (OQ-1). Isolation technique — a TOPIC FILTER puts seeds in topic "k8s" (HNSW-eligible) and graph-injection-only neighbors in topic "ref" (excluded from HNSW), so a neighbor's presence is attributable solely to whether its seed survived the active-only filter. All PPR knobs at production defaults; no eval-harness metric gate (SR-01/GATE-04).

**Gate checks**: GATE-01 prod diff = filter clause only; GATE-02 no engine/graph_expand test edits; GATE-03 `:950` quarantine `is_quarantined` not in diff; GATE-04 no p@5/mrr/soft_gt; GATE-05 typed enum, no string compare; ANTI-AC-01 no deprecated-absence-in-Flexible assertion.

## Issues / Notes
- Full `cargo test --workspace`: 3879 passed, 1 failed — `http::token::tests::test_concurrent_creation_no_corruption` (pre-existing concurrency flake in HTTP credential code; PASSES in isolation; unrelated to crt-053).
- Pre-existing harness gotcha (NOT fixed — out of scope): `skip_if_no_model()` builds the model dir with `--` (model_id replace) while the downloader uses `_` (cache_subdir). When only the `_` dir exists, all pipeline_e2e tests silently SKIP. Validated locally via a temporary symlink (removed). Flag for a separate infra fix.
- Reverted two unrelated test-generated binding fixtures (`request_hookinput_flatten.json`, `request_record_event.json`) touched by the workspace test run.

## Knowledge Stewardship
- Queried: mcp__unimatrix__context_briefing / context_search / context_get — found ADR-001 #4917 (active-only seed predicate, behavior-based-only acceptance), #3637 (search.rs uses tracing:: qualified, Option fields need `?`), #3992 (PPR expander architecture). Applied to the edit and the test isolation strategy.
- Stored: entry #4918 "crt-053: PPR Phase 0 seed-filter behavior is observable only by excluding the neighbor from HNSW (topic-filter isolation)" via context_store (pattern). Captures the in_pool-dedup + truncation traps, the topic-filter isolation technique, the PPR-Phase-1-5-vs-Phase-0 separation, and the skip_if_no_model path-mismatch gotcha.
