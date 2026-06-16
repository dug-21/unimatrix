# Risk Coverage Report: vnc-037

Next-hop edge affordance on `context_get` — ranked, capped (≤3) typed depth-1 edges
with honest uncapped split totals. Stage 3c test execution.

> **Status: PASS.** All unit tests green; mandatory integration smoke gate green;
> all new + relevant integration suites green. No feature bugs found. The 4 initial
> integration failures were **test-seeding bugs** (semantic store-dedup collapsing
> near-identical seeded entries to one id) — fixed by seeding target entries via
> direct SQL; not feature defects. No new `xfail` markers, no GH Issues filed.
> AC-12 latency baseline recorded as **OPEN** (no measured baseline produced here).

## Coverage Summary

| Risk ID | Risk Description | Test(s) | Result | Coverage |
|---------|-----------------|---------|--------|----------|
| R-01 | Symmetric canonicalization miss (display AND totals, before cap AND count) | unit: `test_query_ranked_symmetric_collapses_to_one_row`, `test_count_symmetric_counted_once`, `test_count_symmetric_increments_both_never_inbound`, `test_query_ranked_canon_before_cap_authored_wins`, `test_query_ranked_asymmetric_untouched`, `test_count_before_canon_would_double`; integ: `test_get_symmetric_canonicalized_one_arrow[Contradicts/CoAccess/Informs]`, `test_get_edge_totals_symmetric_once` | PASS | Full |
| R-02 | Ranking ORDER BY silently wrong (proof outside cap, #3886) | unit: `test_query_ranked_by_target_confidence_proof_outside_cap`, `test_query_ranked_authored_priority_under_cap`, `test_query_ranked_inferred_fill_only_when_authored_lt_3`, `test_query_ranked_deterministic_tiebreak`; integ: `test_get_authored_priority_under_cap`, `test_get_inferred_fill_when_authored_lt_3` | PASS | Full |
| R-03 | Split COUNT divergence / canon mismatch (3-bucket + authored aggregate) | unit: `test_count_uncapped_exact`, `test_count_canon_parity_with_rank_query`, `test_count_direction_split_load_bearing`, `test_count_symmetric_increments_both_never_inbound`, `test_count_authored_aggregate_over_full_set`, `test_count_nested_shape_three_buckets`; integ: `test_get_edge_totals_symmetric_once`, `test_get_capped_pointer_and_uncapped_totals`, `test_get_high_degree_node_caps_at_three` | PASS | Full |
| R-04 | Rank-and-limit in Rust not SQL | unit (store boundary): `test_query_ranked_high_degree_returns_exactly_cap_rows`, `test_query_ranked_no_literal_three_and_positional_binds`; integ (MCP echo): `test_get_high_degree_node_caps_at_three` (50 edges → 3 displayed + uncapped totals) | PASS | Full |
| R-05 | Carried-forward / `context_edge` edges mis-classified as inferred | integ: `test_correct_then_get_carried_edge_classifies_authored` (carried edge → `authored=true`, ranks ahead of higher-confidence inferred); existing `test_correction_carries_outgoing_edges_visible_on_new_entry` | PASS | Full |
| R-06 | Confidence LEFT JOIN skews/drops on bad endpoints | unit: `test_query_ranked_join_is_left_not_inner`, `test_query_ranked_dangling_target_retained_nulls_last`, `test_query_ranked_null_confidence_deterministic`, `test_query_ranked_cold_start_uniform_zero_tiebreak`; integ: `test_get_dangling_title_null_retained` | PASS | Full |
| R-07 | Shared serializer byte-identity regression | unit: `test_none_edges_key_absent_structural`, `test_none_json_byte_identical_to_base_object`, `test_some_edges_injected_all_formats`; integ (real producer, #1268): `test_list_view_tools_no_edges_key` (search/lookup/store/correct × json+markdown) | PASS | Full |
| R-08 | Additive `source` breaks `context_graph` neighbors | unit (unedited, empirical #4876): `graph_read_neighbors` suite (6) + store `graph_queries` suite (46) green with zero edits | PASS | Full |
| R-09 | `authored` mislabel (exact-match) | unit: `test_authored_true_only_for_agent`, `test_authored_exact_match_no_near_miss`; integ: `test_get_authored_flag_agent_vs_inferred` | PASS | Full |
| R-10 | Direction / `target_id` projection inverted | unit: `test_projection_outbound_inbound_far_endpoint`, `test_projection_symmetric_both_no_arrow`, `test_projection_matches_edgerecord_mapping`; integ: `test_get_surfaces_ranked_edges_default` (outbound/inbound far-endpoint), `test_get_symmetric_canonicalized_one_arrow` (`both`, no arrow) | PASS | Full |
| R-11 | Untraced JOIN-heavy ranked query (vacuous coverage) | enforced as plan discipline: each R-01/R-02/R-06 store test carries an explicit per-edge rank trace in `graph_queries_ranked_tests.rs` (#3645); discriminating proof seeded outside cap (#3886) | PASS | Full |
| R-12 | `Supersedes` leak / double-representation | unit: `test_query_ranked_supersedes_absent`, `test_count_supersedes_not_counted`; integ: `test_get_supersedes_absent_display_and_totals` | PASS | Full |
| R-13 | AC-12 latency budget unbacked / hub regression | unit: read-pool + indexed-JOIN design verified; `test_get_high_degree_node_caps_at_three` proves hub bound. **Measured edge-free baseline NOT produced here** — see Gaps / AC-12 OPEN | PARTIAL | Partial |
| R-14 | Opt-out does not skip queries | unit: `test_include_edges_three_resolutions`, `test_include_edges_resolution` (handler `Some(false) => None`, never calls `build_edges_view`); integ: `test_get_include_edges_opt_out` (no `edges`/`edge_totals` key), `test_get_include_edges_true_surfaces` | PASS | Full |
| R-15 | Dangling target dropped or panics | unit: `test_dangling_title_null_retained_no_panic`, `test_mixed_resolved_and_dangling`, `test_dangling_title_renders_across_formats`; integ: `test_get_dangling_title_null_retained` | PASS | Full |
| R-16 | Edge-query / title-join failure handling (→ AC-14) | unit (RED, #4876): `test_edge_query_failure_fails_loud`, `test_count_query_failure_fails_loud`, `test_title_join_failure_fails_loud`, `test_zero_edges_is_success_distinct_from_failure`; grep: no `.unwrap()`/`.expect()` on edge path | PASS | Full |
| R-17 | Format-string drift from reframed contract | unit: `test_summary_digest_locked_byte_form`, `test_markdown_flat_ranked_no_subsplit`, `test_json_edge_totals_three_keys`, `test_markdown_capped_pointer_references_constant`, `test_summary_digest_zero_edge_sentinel`, `test_symmetric_renders_arrow_glyph_no_directional`; integ: `test_get_zero_edge_empty_state_all_formats`, `test_get_capped_pointer_and_uncapped_totals` | PASS | Full |
| R-18 | File-size limit (≤500) breached | file-check: all NEW/touched edge files ≤500 (see AC-OQ-B). `read.rs`/`tools.rs` pre-existing large files; vnc-037 added only a const + handler branch (logic routed to sibling modules per OQ-B) | PASS | Full |
| R-19 | Corrected-entry transient misread as bug | integ: `test_correct_then_get_carried_edge_classifies_authored` encodes the DNB-2 transient (authored carried fill slots first, inferred sparse) as expected behavior | PASS | Full |
| R-20 | Future non-statistical source mislabel | grep: C-10 NLI-dark precondition documented at the authored site (`graph_queries.rs:79-80`); `RawEdgeRow.source` string retained beneath the boolean | PASS | Full |

## Test Results

### Unit Tests (`cargo test --lib`, per-package per env constraint)
- `unimatrix-store --lib`: **389 passed, 0 failed** (includes 26 `graph_queries_ranked` + 46 `graph_queries`)
- `unimatrix-server --lib`: **4184 passed, 0 failed, 1 ignored** (includes 88 edge-specific: `get_edges` 11, `response::edges` 9, `response::edges_render` 12, plus `tools`/`response` edge tests; `graph_read_neighbors` 6 unedited)
- `cargo build --workspace`: PASS (AC-10 build half)

### Integration Tests (infra-001, MCP JSON-RPC via compiled binary)
- **Smoke (`-m smoke`) — MANDATORY GATE: 23 passed, 0 failed.**
- New suite `suites/test_get_edges.py` (vnc-037): **17 passed, 0 failed** (15 functions; `test_get_symmetric_canonicalized_one_arrow` ×3 params).
- New lifecycle test `test_correct_then_get_carried_edge_classifies_authored` + existing carry-forward: **2 passed, 0 failed.**
- Regression `contradiction` + `confidence`: **26 passed, 1 xfailed** (xfail = pre-existing GH#405, NOT vnc-037).
- `protocol` + `tools` (get/store/correct/search/lookup): see "Suite Runs" below.

| Suite | Selected because | Result |
|-------|------------------|--------|
| smoke | mandatory minimum gate | 23 passed |
| test_get_edges (new) | new `context_get` edge surfacing | 17 passed |
| lifecycle (carry-forward) | correction-chain carry-forward (R-05/DNB-2) | 2 passed |
| confidence | inferred rank key is `entries.confidence` | green (in 26 passed) |
| contradiction | `Contradicts` is a symmetric canon type — regression-only | green (in 26 passed) |
| protocol + tools | any server tool logic | 67 passed, 1 xfailed (`-k get/store/correct/search/lookup`; xfail = pre-existing GH#405) |

**New integration test count: 18 test cases** (17 in `test_get_edges.py` + 1 lifecycle).
All reuse the `server` fixture and the established direct-SQLite seeding pattern
(cumulative — no isolated scaffolding). The `context_get` harness client method was
extended with the additive `include_edges` kwarg (absent ⇒ default-on).

### Failure triage
4 initial `test_get_edges` failures (direction, authored-priority, two totals)
were diagnosed (per USAGE-PROTOCOL.md decision tree) as **bad-test bugs**: the
seeded "distinct" target entries were collapsed to a single id by the server's
**semantic store-dedup** (cosine ≥ ~0.93 on short templated content), so edges
pointed at one shared id. The corresponding store/server unit tests for the exact
same behaviors were already green, ruling out a feature defect. Fixed by seeding
target entries directly via SQL (`_seed_target`, pinned ids/confidence/title) so
the store-dedup cannot interfere — a test fix, documented here. No feature code
changed. No integration tests deleted or commented out.

## Gaps

- **R-13 / AC-12 — latency baseline: OPEN (obligation, not pass).** No measured
  edge-free `context_get` baseline on a representative store including a high-degree
  node was produced in this environment, so the provisional ≤5 ms p50 / ≤15 ms p95
  numbers are **NOT confirmed**. The rank-and-limit-in-SQL design (R-04, proven by
  `test_query_ranked_high_degree_returns_exactly_cap_rows` and the MCP hub test) is
  the structural mechanism that makes the budget reachable, but the number lock is
  gated on the measurement per C-9/OQ-C. **Recorded as the documented OPEN
  obligation — escalation path (relax / mandate OQ-03 opt-out / revisit default-on)
  is specified in the brief and goes to the human.** Not silently passed.
- **AC-13b cap-isolation override variant — not runtime-testable.**
  `GET_EDGE_DISPLAY_LIMIT` is a compile-time `const`, so the "override the constant,
  assert only the rendered set shrinks" runtime variant is not expressible without a
  test-only feature flag. The AC-13a **single-source** invariant (the load-bearing
  half) is fully proven: the SQL `LIMIT` binds `?2` to the constant (no literal),
  render uses `total > cap` / `total - cap` with `cap = GET_EDGE_DISPLAY_LIMIT`, and
  all tests reference the constant not `3`. Net: AC-13 PASS on the single-source
  guarantee; the override variant is a structural impossibility, not a coverage miss.

All other risks R-01..R-20 have full test coverage.

## Acceptance Criteria Verification

| AC-ID | Status | Evidence |
|-------|--------|----------|
| AC-01 | PASS | `test_get_surfaces_ranked_edges_default` (default-on, both directions), `test_get_edges_freshness_no_tick` (live read, no tick) |
| AC-02 | PASS | `test_get_surfaces_ranked_edges_default` (exact 5-field shape), `test_get_dangling_title_null_retained` (null title retained); unit `test_get_edge_exact_five_fields` |
| AC-03 | PASS | `test_get_authored_flag_agent_vs_inferred`; unit `test_authored_true_only_for_agent`, `test_authored_exact_match_no_near_miss`; no migration (schema `source` column pre-exists) |
| AC-04 | PASS | `test_get_supersedes_absent_display_and_totals`; unit `test_query_ranked_supersedes_absent`, `test_count_supersedes_not_counted` |
| AC-05 | PASS | (a) `test_get_authored_priority_under_cap` (b) `test_get_inferred_fill_when_authored_lt_3` (c) unit `test_query_ranked_by_target_confidence_proof_outside_cap` (weight does NOT decide) (d) `test_get_edge_totals_symmetric_once` (e) `test_get_capped_pointer_and_uncapped_totals` |
| AC-06 | PASS | `test_get_zero_edge_empty_state_all_formats` (all 3 formats SUCCESS); unit `test_markdown_zero_edge_empty_state`, `test_summary_digest_zero_edge_sentinel` |
| AC-07 | PASS | `test_list_view_tools_no_edges_key` (4 list-view tools, real producer #1268); unit `test_none_json_byte_identical_to_base_object`, `test_none_edges_key_absent_structural` |
| AC-08 | PASS | unit `test_summary_digest_locked_byte_form`, `test_markdown_flat_ranked_no_subsplit` (sub-split absent), `test_json_edge_totals_three_keys`; integ `test_get_capped_pointer_and_uncapped_totals` (capped pointer) |
| AC-09 | PASS | `graph_read_neighbors` (6) + store `graph_queries` (46) suites green UNEDITED (#4876); unit `test_projection_matches_edgerecord_mapping`; `↔`/canon confined to ranked variant (no leak) |
| AC-10 | PASS | `cargo build --workspace` green; `cargo test` per-package green (store 389, server 4184); named cases all present + asserting (discriminating) |
| AC-11 | PASS | `test_get_include_edges_opt_out` (no key, suppressed), `test_get_include_edges_true_surfaces`; unit `test_include_edges_three_resolutions`; handler `Some(false) => None` skips `build_edges_view` |
| AC-12 | **OPEN** | Hub bound proven (`test_get_high_degree_node_caps_at_three`, store-boundary `test_query_ranked_high_degree_returns_exactly_cap_rows`); **measured latency baseline NOT produced** — provisional numbers remain unconfirmed (C-9/OQ-C). Escalation to human per brief. |
| AC-13 | PASS | (a) single-source: SQL `LIMIT ?2` bound to `GET_EDGE_DISPLAY_LIMIT`, render `total > cap`/`total - cap`, no literal 3 (grep + unit `test_query_ranked_no_literal_three_and_positional_binds`, `test_markdown_capped_pointer_references_constant`). (b) runtime override not expressible for a `const` — see Gaps. |
| AC-14 | PASS | RED tests (#4876): `test_edge_query_failure_fails_loud`, `test_count_query_failure_fails_loud`, `test_title_join_failure_fails_loud` (each injects a real failure → `Err`); `test_zero_edges_is_success_distinct_from_failure` + integ `test_get_zero_edge_empty_state_all_formats` (zero ≠ failure); grep: no `.unwrap()`/`.expect()` on edge path; handler maps edge `Err` with the same `ServerError` as the primary read |

### Promoted scope-risk assertions
| Ref | Status | Evidence |
|-----|--------|----------|
| SR-08/C-6 (symmetric counted once, display AND totals, before cap+count) | PASS | display + totals asserted independently (unit + integ); order-of-ops `test_query_ranked_canon_before_cap_authored_wins` |
| SR-09/C-8 (locked ORDER BY, weight does NOT decide) | PASS | `test_query_ranked_by_target_confidence_proof_outside_cap` (proof outside cap, lower weight on high-conf target) |
| SR-14/C-7 (rank+count in SQL, not Rust) | PASS | `test_query_ranked_high_degree_returns_exactly_cap_rows` (store boundary), no `SELECT *`+slice |
| Security (positional binds; static LIMIT/ORDER BY/CASE) | PASS | grep: ranked/count bind anchor `?1` + cap `?2`; title `IN (…)` positional binds; no string-interpolated ids; `test_query_ranked_no_literal_three_and_positional_binds`, `test_count_uses_positional_binds` |
| C-11 (no `.unwrap()`/`.expect()` on edge path) | PASS | grep `graph_queries_ranked.rs` + `get_edges.rs` non-test: only doc-comment mentions, zero calls |
| OQ-B/R-18 (files ≤500) | PASS | new files: `graph_queries_ranked.rs` 255, `get_edges.rs` 158, `response/edges.rs` 322, `response/edges_render.rs` 346; touched `graph_queries.rs` 469, `response/entries.rs` 376. (`read.rs` 3789 / `tools.rs` 12312 are pre-existing large files; vnc-037 added only the const + handler branch, splitting new logic onto sibling modules per OQ-B.) |
| C-10/SR-05 (NLI-dark precondition documented; source retained) | PASS | `graph_queries.rs:79-80` documents NLI revival as the D-03 revisit trigger; `RawEdgeRow.source` retained |
| OQ-03 internal-caller opt-out | PASS (by construction) | Internal callers (hook path, briefing/by-ID loops) call `entry_store.get` directly — they never reach the MCP `context_get` handler or `build_edges_view`, so they incur zero edge cost without needing an explicit `Some(false)`. The agent-facing MCP tool stays default-on. Param-contract resolution unit-tested. |

## Knowledge Stewardship
- Queried: `mcp__unimatrix__context_briefing` -- MCP disconnected (non-blocking, per spawn note); proceeded without.
- Stored: nothing novel to store -- the dominant cross-feature lesson this run exercised
  ("an `ORDER BY ... LIMIT` ranking test must seed the discriminating value OUTSIDE the
  cap") already exists as #3886 and was applied verbatim; the integration-seeding gotcha
  ("MCP store semantic-dedup collapses near-identical seeded entries — seed target rows
  via direct SQL when you need many distinct ids/confidences") is a candidate pattern but
  is feature/harness-specific and single-occurrence so far. Will revisit at retro if a
  second feature hits the same dedup-vs-seeding trap.
