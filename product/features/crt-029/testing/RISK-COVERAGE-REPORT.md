# Risk Coverage Report: crt-029 — Background Graph Inference

GH Issue: #412

---

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 (by design) | False-positive `Contradicts` edges via tick | Grep gate: `grep -n 'Contradicts' nli_detection_tick.rs` — returns only comments and test assertions (no live write path); `test_write_inferred_edges_supports_only_no_contradicts` | PASS | Full |
| R-02 | `get_embedding` O(N) scan unbounded if cap not enforced before Phase 4 | `test_select_source_candidates_cap_enforced`, `test_select_source_candidates_priority_ordering_combined` (20 tick tests cover candidate selection) | PASS | Full |
| R-03 | Threshold boundary `>=` vs `>` — equal values pass validation | `test_validate_rejects_equal_thresholds`, `test_validate_rejects_candidate_above_edge`, `test_validate_accepts_candidate_below_edge`, `test_validate_rejects_candidate_threshold_zero`, `test_validate_rejects_candidate_threshold_one`, `test_validate_rejects_edge_threshold_zero`, `test_validate_rejects_edge_threshold_one` | PASS | Full |
| R-04 | `query_existing_supports_pairs()` full GRAPH_EDGES scan at scale | `test_query_existing_supports_pairs_empty`, `test_query_existing_supports_pairs_bootstrap_excluded`, `test_query_existing_supports_pairs_mixed_bootstrap`, `test_query_existing_supports_pairs_excludes_contradicts`, `test_query_existing_supports_pairs_normalization`, `test_query_existing_supports_pairs_supports_only` | PASS | Full |
| R-05 | Rayon pool starvation: tick and post-store NLI contend on the same pool | `test_concurrent_search_stability` (lifecycle integration), single-dispatch pattern verified by AC-08 grep gate | PASS | Partial (no explicit concurrent contention test; integration lifecycle passes) |
| R-06 | `compute_graph_cohesion_metrics` pool choice ambiguous (two conflicting ADRs) | Grep gate: `grep -n 'fetch_one' read.rs` at `compute_graph_cohesion_metrics` line 1025 confirms `read_pool()` is used; C-12 confirmed | PASS | Full |
| R-07 | `InferenceConfig` struct literal constructions not updated (52 occurrences) | Grep gate: 69 occurrences found; all include new fields or `..InferenceConfig::default()` tail; `cargo build --workspace` passes clean; `test_inference_config_defaults` | PASS | Full |
| R-08 | Cap logic inlined in `write_inferred_edges_with_cap` — untestable without ONNX | `test_write_inferred_edges_with_cap_cap_enforced`, `test_write_inferred_edges_cap_at_exact_count`, `test_write_inferred_edges_zero_eligible`, `test_write_inferred_edges_threshold_strict_greater` | PASS | Full |
| R-09 | Rayon closure calls `tokio::Handle::current()` — runtime panic | Grep gate: `grep -n 'Handle::current' nli_detection_tick.rs` — empty (no matches in live code); Manual inspection of rayon closure (lines 234–241): synchronous-only body, `.await` on line 242 is outside the closure on the tokio thread | PASS | Full |
| R-10 | W1-2 violated: `score_batch` via `spawn_blocking` | Grep gate: `grep -n 'spawn_blocking' nli_detection_tick.rs` — returns only comment on line 8 (no live calls) | PASS | Full |
| R-11 | `write_nli_edge` / `format_nli_metadata` / `current_timestamp_secs` not promoted to `pub(crate)` | Grep gate: `grep -n 'pub(crate) fn'` in `nli_detection.rs` — `write_nli_edge` (line 532), `format_nli_metadata` (line 628), `current_timestamp_secs` (line 639) all present; `cargo build --workspace` passes | PASS | Full |
| R-12 | Priority ordering not enforced at cap boundary | `test_select_source_candidates_priority_ordering_combined`, `test_select_source_candidates_isolated_second`, `test_select_source_candidates_cap_enforced`, `test_select_source_candidates_remainder_by_created_at` | PASS | Full |
| R-13 | Pre-filter `HashSet` stale under concurrent post-store NLI | `test_tick_idempotency`, `test_write_inferred_edges_insert_or_ignore_idempotency` | PASS | Full |

---

## Test Results

### Unit Tests

- **Total runs**: 3,792 tests across all workspace crates
- **Passed**: 3,792
- **Failed**: 0
- **Ignored**: 27

**crt-029-specific unit tests:**

| Module | Tests | Passed | Failed |
|--------|-------|--------|--------|
| `services::nli_detection_tick::tests` | 20 | 20 | 0 |
| `infra::config::tests` (crt-029 subset) | ~35 | ~35 | 0 |
| `read::tests::test_query_entries_without_edges*` | 6 | 6 | 0 |
| `read::tests::test_query_existing_supports_pairs*` | 6 | 6 | 0 |

**Full `services::nli_detection_tick::tests` test list (all PASS):**
- `test_select_source_candidates_empty_input`
- `test_select_source_candidates_max_sources_zero`
- `test_select_source_candidates_cap_enforced`
- `test_select_source_candidates_cap_larger_than_entries`
- `test_select_source_candidates_all_isolated`
- `test_select_source_candidates_isolated_second`
- `test_select_source_candidates_priority_ordering_combined`
- `test_select_source_candidates_remainder_by_created_at`
- `test_write_inferred_edges_with_cap_cap_enforced`
- `test_write_inferred_edges_cap_at_exact_count`
- `test_write_inferred_edges_zero_eligible`
- `test_write_inferred_edges_threshold_strict_greater`
- `test_write_inferred_edges_supports_only_no_contradicts`
- `test_write_inferred_edges_edge_source_nli`
- `test_write_inferred_edges_insert_or_ignore_idempotency`
- `test_tick_empty_entry_set_select_candidates`
- `test_tick_single_active_entry`
- `test_tick_pair_dedup_normalization`
- `test_tick_idempotency`
- `test_run_graph_inference_tick_nli_not_ready_no_op`

### Integration Tests

**Smoke suite (`-m smoke`):**
- Total: 20
- Passed: 20
- Failed: 0
- Duration: ~175s

**Lifecycle suite (`suites/test_lifecycle.py`):**
- Total: 41
- Passed: 38
- xfailed: 2 (pre-existing, see below)
- xpassed: 1 (pre-existing, see below)
- Failed: 0
- Duration: ~362s

**Tools suite (`suites/test_tools.py`):**
- Total: 95
- Passed: 93
- xfailed: 2 (pre-existing, see below)
- Failed: 0
- Duration: ~789s

### Pre-Merge Grep Gates

| Gate | Command | Result |
|------|---------|--------|
| AC-10a / R-01 — No Contradicts writes | `grep -n 'Contradicts' nli_detection_tick.rs` | PASS — only comments and test assertions; no live write path |
| R-09 / C-14 — No `Handle::current` | `grep -n 'Handle::current' nli_detection_tick.rs` | PASS — empty (all matches are comment-only) |
| R-10 / AC-08 — No `spawn_blocking` | `grep -n 'spawn_blocking' nli_detection_tick.rs` | PASS — empty in live code (line 8 is doc comment only) |
| R-11 — `pub(crate)` promotions | `grep -n 'pub(crate) fn write_nli_edge\|pub(crate) fn format_nli_metadata\|pub(crate) fn current_timestamp_secs' nli_detection.rs` | PASS — all three present at lines 532, 628, 639 |
| NFR-05 / C-08 — File size | `wc -l nli_detection_tick.rs` | PASS — 773 lines (≤ 800) |
| R-07 / AC-18† — InferenceConfig literals | `grep -rn 'InferenceConfig {' crates/unimatrix-server/src/` | PASS — 69 occurrences; all include new fields or `..default()` tail; build clean |
| C-12 / R-06 — Pool choice | `compute_graph_cohesion_metrics` in `read.rs` | PASS — confirmed `read_pool()` at line 1025 |
| AC-14 — Call site ordering | `background.rs` lines 668-676 | PASS — tick called after `maybe_run_bootstrap_promotion`, gated on `nli_enabled` |

### R-09 Independent Closure Inspection

As required by C-14 (independent validator requirement), this agent (crt-029-agent-7-tester, not the implementation author) performed manual inspection of the rayon closure:

```
nli_detection_tick.rs lines 233-242:
    let nli_result = rayon_pool
        .spawn(move || {
            // SYNC-ONLY CLOSURE — no .await, no Handle::current()
            let pairs_ref: Vec<(&str, &str)> = nli_pairs
                .iter()
                .map(|(q, p)| (q.as_str(), p.as_str()))
                .collect();
            provider_clone.score_batch(&pairs_ref)  // sync call only
        })
        .await;  // .await is OUTSIDE the closure on the tokio thread
```

Verdict: **PASS**. The closure body is synchronous-only. `score_batch` is a synchronous function. No `.await` inside the closure. No `Handle::current()`. The `.await` on line 242 is outside the closure, on the tokio thread awaiting the `rayon_pool.spawn()` future. C-14 constraint is satisfied.

---

## Pre-Existing xfail/xpass (not caused by crt-029)

### lifecycle suite

| Test | Status | Reason |
|------|--------|--------|
| `test_auto_quarantine_after_consecutive_bad_ticks` | XFAIL | Pre-existing: tick interval env var (`UNIMATRIX_TICK_INTERVAL_SECONDS`) not available in test harness; unit tests in `background.rs` cover trigger logic |
| `test_dead_knowledge_entries_deprecated_by_tick` | XFAIL | Pre-existing: dead-knowledge deprecation runs at 15-min interval; not driveable from test harness |
| `test_search_multihop_injects_terminal_active` | XPASS | GH#406 — `find_terminal_active` multi-hop traversal not implemented; was marked xfail but passes now; not caused by crt-029; marker should be removed and GH#406 updated |

### tools suite

| Test | Status | Reason |
|------|--------|--------|
| `test_confidence_deprecated_not_higher_than_active` | XFAIL | Pre-existing: GH#405 — deprecated confidence can exceed active due to background scoring timing |
| `test_retrospective_baseline_present` | XFAIL | Pre-existing: GH#305 — baseline_comparison null when synthetic features lack delivery counter registration |

None of these failures are caused by crt-029 changes. No new xfail markers added in this feature.

---

## Gaps

### New Integration Tests Required (Planned, Not Implemented)

The test plan (OVERVIEW.md) specifies three new lifecycle integration tests for behaviors visible only through the MCP interface. These tests were not implemented in the Stage 3b code delivery and are not yet in the harness:

1. `test_graph_inference_tick_writes_supports_edges` — verify `Supports` edges accumulate with `source = 'nli'` after background tick runs (AC-13, FR-07)
2. `test_graph_inference_tick_no_contradicts_edges` — verify zero tick-path `Contradicts` edges after tick runs (AC-10a, AC-19†, R-01)
3. `test_graph_inference_tick_nli_disabled` — verify `inferred_edge_count` stays 0 with `nli_enabled = false` (AC-14, FR-06)

**Gap reason**: The tick requires a running NLI model to produce observable graph edges. The integration harness does not have an ONNX model available in the test environment, so end-to-end tick-firing integration tests cannot be executed. The behaviors are fully covered by unit tests (`test_write_inferred_edges_edge_source_nli`, `test_write_inferred_edges_supports_only_no_contradicts`, `test_run_graph_inference_tick_nli_not_ready_no_op`) and by code review. The three planned integration tests would require an NLI-enabled server fixture.

**Risk classification**: Low. All safety-critical constraints (R-01, R-09, R-10, C-13, C-14) are verified by grep gates and code review. The behavioral gap (no live tick end-to-end MCP integration test) is an observability gap, not a correctness gap.

### R-06 ADR Conflict (Pre-existing Knowledge Housekeeping)

Unimatrix entries #3593 (write-pool) and #3595 (read-pool) remain conflicting for `compute_graph_cohesion_metrics`. The code confirms `read_pool()` is correct (line 1025). Entry #3593 should be deprecated — this is a knowledge housekeeping task, not a code risk.

---

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_inference_config_defaults` — all four fields at correct defaults (0.5, 0.7, 100, 10) |
| AC-02 | PASS | `test_validate_rejects_equal_thresholds` (0.7==0.7→Err), `test_validate_rejects_candidate_above_edge` (0.8>0.7→Err), `test_validate_accepts_candidate_below_edge` (0.69<0.7→Ok) |
| AC-03 | PASS | `test_validate_rejects_candidate_threshold_zero`, `test_validate_rejects_candidate_threshold_one`, `test_validate_rejects_edge_threshold_zero`, `test_validate_rejects_edge_threshold_one` |
| AC-04 | PASS | `test_validate_rejects_max_inference_zero` (0→Err), `test_validate_rejects_max_inference_over_limit` (1001→Err), `test_validate_accepts_max_inference_at_bounds` (1→Ok, 1000→Ok) |
| AC-04b | PASS | `test_validate_rejects_graph_inference_k_zero` (0→Err), `test_validate_rejects_graph_inference_k_over_limit` (101→Err), `test_validate_accepts_graph_inference_k_at_bounds` (1→Ok, 100→Ok) |
| AC-05 | PASS | `test_run_graph_inference_tick_nli_not_ready_no_op` — NliServiceHandle returning Err causes immediate return, 0 DB calls |
| AC-06 | PARTIAL | `test_select_source_candidates_*` cover Active-only selection; Deprecated exclusion not separately tested at integration level (NLI model unavailable) |
| AC-06b | PASS | `test_tick_idempotency` — pairs with existing Supports edges do not produce duplicate NLI work or duplicate edges |
| AC-06c | PASS | `test_select_source_candidates_cap_enforced` (cap output ≤ max_sources), `test_select_source_candidates_priority_ordering_combined` — embedding calls bounded by cap applied before Phase 4 |
| AC-07 | PASS | `test_select_source_candidates_priority_ordering_combined` — cross-category first, isolated second, remainder by recency |
| AC-08 | PASS | Grep gate: `grep -n 'spawn_blocking' nli_detection_tick.rs` → empty in live code; single-dispatch pattern confirmed by code review |
| AC-09 | PASS | `test_write_inferred_edges_threshold_strict_greater` — at-threshold (0.70 == 0.70) does not produce edge; above-threshold (0.71 > 0.70) produces edge |
| AC-10a | PASS | Grep gate: `grep -n 'Contradicts' nli_detection_tick.rs` → only comments/test assertions; `test_write_inferred_edges_supports_only_no_contradicts` — high contradiction score produces no Contradicts edge |
| AC-11 | PASS | `test_write_inferred_edges_with_cap_cap_enforced` — cap=3, 10 eligible pairs → exactly 3 written, return=3 |
| AC-13 | PASS | `test_write_inferred_edges_edge_source_nli` — all written edges carry `source = EDGE_SOURCE_NLI` ("nli") |
| AC-14 | PASS | Code review of `background.rs` lines 668-676: tick called after `maybe_run_bootstrap_promotion`, both gated on `inference_config.nli_enabled` |
| AC-15 | PASS | `test_query_entries_without_edges_empty_store`, `test_query_entries_without_edges_no_edges`, `test_query_entries_without_edges_bootstrap_only_ignored`, `test_query_entries_without_edges_with_edges`, `test_query_entries_without_edges_partial_coverage`, `test_query_entries_without_edges_inactive_excluded` |
| AC-16 | PASS | `test_tick_idempotency`, `test_write_inferred_edges_insert_or_ignore_idempotency` — duplicate pair → INSERT OR IGNORE, no duplicate rows |
| AC-17 | PASS | `test_inference_config_toml_defaults` — absent fields use spec'd defaults; `test_inference_config_toml_explicit_values` — explicit TOML values override defaults |
| AC-18† | PASS | Grep gate: 69 `InferenceConfig {` occurrences; Default impl at line 442-446 includes all four fields; production literal at line 1652 includes all four fields; `cargo build --workspace` clean |
| AC-19† | PASS | Grep gate: `grep -n 'Contradicts\|contradiction_threshold\|nli_contradiction' nli_detection_tick.rs` → only comments/test assertions; `write_inferred_edges_with_cap` signature has no `contradiction_threshold` parameter |
| AC-R09 | PASS | Grep gate + independent code review: rayon closure body (lines 235-241) is synchronous-only; `.await` on line 242 is outside closure on tokio thread; no `Handle::current()` anywhere |

---

## Knowledge Stewardship

- Queried: `mcp__unimatrix__context_briefing` — returned entries #229 (tester duties) and #222 (risk-strategist duties); no new testing patterns for crt-029 specifically.
- Stored: nothing novel to store — the integration test gap pattern (NLI-model-unavailable in harness → unit tests cover safety constraints, integration gap is observability-only) is not a new pattern. The rayon closure inspection methodology is already documented in Unimatrix entries #3339, #3353. No new cross-feature pattern discovered.
