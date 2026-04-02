# Agent Report: crt-038-agent-5-dead-code-removal

**Feature**: crt-038 — conf-boost-c Formula and NLI Dead-Code Removal  
**Agent**: crt-038-agent-5-dead-code-removal  
**Wave**: 2 (single agent — shared files)  
**Commit**: 420704b3

---

## Summary

Removed three dead NLI code paths across nli_detection.rs, store_ops.rs, services/mod.rs, and background.rs. All ACs verified.

---

## Files Modified

- `crates/unimatrix-server/src/services/nli_detection.rs` — complete rewrite; file reduced from 1,374 lines to 120 lines
- `crates/unimatrix-server/src/services/store_ops.rs` — removed NliStoreConfig struct/impl, nli_cfg field, nli_cfg parameter, tokio::spawn NLI block, run_post_store_nli import
- `crates/unimatrix-server/src/services/mod.rs` — removed NliStoreConfig import and nli_store_cfg construction block
- `crates/unimatrix-server/src/background.rs` — removed maybe_run_bootstrap_promotion import + call site, stale sequencing comment, NliQuarantineCheck enum, nli_auto_quarantine_allowed fn, parse_nli_contradiction_from_metadata fn, NLI guard block from process_auto_quarantine, and nli_enabled/nli_auto_quarantine_threshold from 4 function signatures; deleted 9 test functions
- `crates/unimatrix-server/src/main.rs` — removed 2 arguments from both call sites of spawn_background_tick (cargo fmt also touched whitespace)

---

## Deleted Symbols (verified absent by grep)

| Symbol | File | Status |
|--------|------|--------|
| `run_post_store_nli` | nli_detection.rs | Deleted (function); doc-comment reference in config.rs retained — InferenceConfig field comment, out of scope |
| `write_edges_with_cap` | nli_detection.rs | Deleted |
| `maybe_run_bootstrap_promotion` | nli_detection.rs | Deleted (function); doc-comment in nli_detection_tick.rs retained |
| `run_bootstrap_promotion` | nli_detection.rs | Deleted |
| `promote_bootstrap_edge` | nli_detection.rs | Deleted |
| `set_bootstrap_marker` | nli_detection.rs | Deleted |
| `NliStoreConfig` | store_ops.rs | Deleted |
| `nli_auto_quarantine_allowed` | background.rs | Deleted |
| `NliQuarantineCheck` | background.rs | Deleted |
| `parse_nli_contradiction_from_metadata` | background.rs | Deleted |

---

## Retained Symbols (AC-13 verified)

| Symbol | File | Visibility |
|--------|------|------------|
| `write_nli_edge` | nli_detection.rs:19 | pub(crate) |
| `format_nli_metadata` | nli_detection.rs:62 | pub(crate) |
| `current_timestamp_secs` | nli_detection.rs:73 | pub(crate) |

Import in nli_detection_tick.rs line 34 unchanged.

---

## Deleted Tests

**nli_detection.rs** (11 named in spec + 2 for write_edges_with_cap = 13 total, all confirmed gone):
- test_empty_embedding_skips_nli
- test_nli_not_ready_exits_immediately
- test_circuit_breaker_stops_at_cap
- test_circuit_breaker_counts_all_edge_types
- test_bootstrap_promotion_zero_rows_sets_marker
- test_maybe_bootstrap_promotion_skips_if_marker_present
- test_maybe_bootstrap_promotion_defers_when_nli_not_ready
- test_bootstrap_promotion_confirms_above_threshold
- test_bootstrap_promotion_refutes_below_threshold
- test_bootstrap_promotion_idempotent_second_run_no_duplicates
- test_bootstrap_promotion_nli_inference_runs_on_rayon_thread
- Mock structs FixedMockProvider, PanicOnCallProvider, ThreadRecordingProvider also deleted (they existed only to serve the deleted tests; count is 11 named functions)

**background.rs** (4 integration + 5 unit = 9 total):
- test_parse_nli_contradiction_from_metadata_valid_json (deleted — function deleted)
- test_parse_nli_contradiction_from_metadata_none_input (deleted — function deleted)
- test_parse_nli_contradiction_from_metadata_missing_field (deleted — function deleted)
- test_parse_nli_contradiction_from_metadata_malformed_json (deleted — function deleted)
- test_parse_nli_contradiction_from_metadata_zero_score (deleted — function deleted)
- test_nli_edges_below_auto_quarantine_threshold_no_quarantine (deleted — integration test)
- test_nli_edges_above_auto_quarantine_threshold_may_quarantine (deleted — integration test; spec listed as test_nli_edges_above_threshold_allow_quarantine)
- test_nli_mixed_edges_allow_quarantine (deleted — integration test; spec listed as test_nli_auto_quarantine_mixed_penalty_allowed)
- test_no_contradicts_edges_allows_quarantine (deleted — integration test; spec listed as test_nli_auto_quarantine_no_edges_allowed)

**Note on test name discrepancies**: The actual function names in background.rs differed from the spec's IMPLEMENTATION-BRIEF.md names (the spec used abbreviated names). All 4 integration tests were identified and deleted by reading the actual source. The 5 `parse_nli_contradiction_from_metadata` unit tests were discovered by reading the source — they are not explicitly listed in the brief but were required deletions since the function itself was deleted.

---

## Signature Cascade: nli_enabled / nli_auto_quarantine_threshold

**Yes — these params appeared in 5 function signatures, not just process_auto_quarantine.**

Functions cleaned:
1. `spawn_background_tick` (background.rs) — removed from public signature
2. `background_tick_loop` (background.rs) — removed from internal signature + pass-through call
3. `run_single_tick` (background.rs) — removed from internal signature + pass-through call
4. `maintenance_tick` (background.rs) — removed from internal signature + call to process_auto_quarantine
5. `process_auto_quarantine` (background.rs) — removed from signature + body guard block deleted

Call sites in main.rs (2 occurrences) — removed arguments at both spawn_background_tick call sites.

---

## Build and Test Results

- `cargo build --workspace`: PASS (zero errors; 16 pre-existing warnings in unimatrix-server, all pre-existing)
- `cargo test -p unimatrix-server --lib`: PASS — 2570 passed, 0 failed
- `cargo test --workspace`: PASS — 0 failures across all crates
- `cargo clippy --workspace -- -D warnings`: Pre-existing failures in unimatrix-engine and unimatrix-observe (collapsible_if, manual_pattern_char_comparison, etc.); zero new failures from our changes; zero clippy errors in unimatrix-server source

---

## Knowledge Stewardship

- Queried: mcp__unimatrix__context_briefing — surfaced entries #3985, #4008, #4006, #4005, #4007 (crt-038 ADRs), plus relevant NLI patterns. Briefing confirmed all key decisions: NliStoreConfig full deletion (ADR-002), write_edges_with_cap cascaded deletion (AC-11 requirement), 4-function parameter cascade for nli_enabled/nli_auto_quarantine_threshold.
- Stored: nothing novel to store — the parameter cascade was documented in the pseudocode (lines 374-399 of dead-code-removal.md). The test name discrepancy (spec used abbreviated names vs actual function names) is a documentation gap but not a pattern worth storing — it's a one-time artifact of spec writing, not a recurring trap.
